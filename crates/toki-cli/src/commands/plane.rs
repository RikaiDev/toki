/// Plane.so integration command handler
use anyhow::Result;
use clap::Subcommand;
use toki_integrations::{PlaneClient, ProjectManagementSystem};
use toki_storage::Database;

use super::helpers::truncate_str;

#[derive(Subcommand, Debug)]
pub enum PlaneAction {
    /// List projects in the workspace
    Projects,
    /// List work items (issues) in a project
    Issues {
        /// Project identifier (e.g., PROJ)
        #[arg(short, long)]
        project: Option<String>,
        /// Search query
        #[arg(short, long)]
        search: Option<String>,
    },
    /// List work items assigned to you
    MyIssues,
    /// Test connection to Plane.so
    Test,
}

pub async fn handle_plane_command(action: PlaneAction) -> Result<()> {
    let db = Database::new(None)?;

    // Get Plane configuration
    let config = db.get_integration_config("plane")?.ok_or_else(|| {
        anyhow::anyhow!("Plane.so not configured. Run: toki config set plane.api_key <key>")
    })?;

    let workspace_slug = config.workspace_slug.clone().ok_or_else(|| {
        anyhow::anyhow!("Workspace not configured. Run: toki config set plane.workspace <slug>")
    })?;

    let client = PlaneClient::new(
        config.api_key.clone(),
        workspace_slug,
        Some(config.api_url.clone()),
    )?;

    match action {
        PlaneAction::Test => {
            println!("Testing Plane.so connection...");
            match client.validate_credentials().await {
                Ok(true) => println!("Connection successful!"),
                Ok(false) => println!("Connection failed: Invalid credentials"),
                Err(e) => println!("Connection failed: {e}"),
            }
        }
        PlaneAction::Projects => {
            println!("Fetching projects from Plane.so...\n");
            let projects = client.list_projects().await?;

            if projects.is_empty() {
                println!("No projects found.");
                return Ok(());
            }

            println!("{:<40} {:<12} TIME TRACKING", "NAME", "IDENTIFIER");
            println!("{}", "-".repeat(70));
            for project in projects {
                let time_tracking = if project.is_time_tracking_enabled {
                    "Enabled"
                } else {
                    "Disabled"
                };
                println!(
                    "{:<40} {:<12} {}",
                    project.name, project.identifier, time_tracking
                );
            }
        }
        PlaneAction::Issues { project, search } => {
            if let Some(query) = search {
                println!("Searching work items for: {query}\n");
                let items = client.search_work_items(&query).await?;

                if items.is_empty() {
                    println!("No work items found.");
                    return Ok(());
                }

                println!("{:<15} {:<50} STATUS", "ID", "TITLE");
                println!("{}", "-".repeat(80));
                for item in items {
                    let status = item
                        .state_detail
                        .map_or_else(|| "Unknown".to_string(), |s| s.name);
                    let project_id = item
                        .project_detail
                        .map_or_else(|| "???".to_string(), |p| p.identifier);
                    let title = truncate_str(&item.name, 47);
                    println!(
                        "{}-{:<10} {:<50} {}",
                        project_id, item.sequence_id, title, status
                    );
                }
            } else if let Some(project_id) = project {
                println!("Fetching work items from project: {project_id}\n");

                // First find the project UUID
                let projects = client.list_projects().await?;
                let target_project = projects
                    .iter()
                    .find(|p| p.identifier == project_id || p.id.to_string() == project_id)
                    .ok_or_else(|| anyhow::anyhow!("Project not found: {project_id}"))?;

                let response = client.list_work_items(&target_project.id, None).await?;

                if response.results.is_empty() {
                    println!("No work items found.");
                    return Ok(());
                }

                println!("{:<15} {:<50} STATUS", "ID", "TITLE");
                println!("{}", "-".repeat(80));
                for item in response.results {
                    let status = item
                        .state_detail
                        .map_or_else(|| "Unknown".to_string(), |s| s.name);
                    let title = truncate_str(&item.name, 47);
                    println!(
                        "{}-{:<10} {:<50} {}",
                        target_project.identifier, item.sequence_id, title, status
                    );
                }

                if response.next_page_results {
                    println!("\n(More results available, pagination not yet implemented)");
                }
            } else {
                println!("Please specify --project <id> or --search <query>");
                println!("\nExamples:");
                println!("  toki plane issues --project PROJ");
                println!("  toki plane issues --search \"bug fix\"");
            }
        }
        PlaneAction::MyIssues => {
            println!("Fetching your assigned work items...\n");
            let items = client.get_assigned_work_items().await?;

            if items.is_empty() {
                println!("No work items assigned to you.");
                return Ok(());
            }

            println!("{:<15} {:<50} STATUS", "ID", "TITLE");
            println!("{}", "-".repeat(80));
            for item in items {
                let status = item
                    .state_detail
                    .map_or_else(|| "Unknown".to_string(), |s| s.name);
                let project_id = item
                    .project_detail
                    .map_or_else(|| "???".to_string(), |p| p.identifier);
                let title = truncate_str(&item.name, 47);
                println!(
                    "{}-{:<10} {:<50} {}",
                    project_id, item.sequence_id, title, status
                );
            }
        }
    }

    Ok(())
}
