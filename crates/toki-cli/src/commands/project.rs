/// Project management command handler
use anyhow::Result;
use clap::Subcommand;
use std::sync::Arc;
use toki_integrations::plane::PlaneClient;
use toki_storage::Database;

use super::helpers::truncate_str;

#[derive(Subcommand, Debug)]
pub enum ProjectAction {
    /// List all tracked projects
    List,
    /// Link a local project to a PM system (Plane or Notion)
    Link {
        /// Local project name
        #[arg(short, long)]
        project: String,
        /// Plane project identifier (e.g., "HYGIE")
        #[arg(long)]
        plane_project: Option<String>,
        /// Notion database ID
        #[arg(long)]
        notion_database: Option<String>,
    },
    /// Unlink a project from PM system
    Unlink {
        /// Project name
        project: String,
    },
    /// Auto-detect and suggest project links (AI-powered)
    AutoLink {
        /// Minimum confidence threshold (0.0-1.0, default: 0.8)
        #[arg(short, long, default_value = "0.8")]
        min_confidence: f32,
        /// Actually apply the links (without this, only shows suggestions)
        #[arg(long)]
        apply: bool,
    },
}

pub async fn handle_project_command(action: ProjectAction) -> Result<()> {
    let db = Database::new(None)?;

    match action {
        ProjectAction::List => {
            let projects = db.get_all_projects()?;

            if projects.is_empty() {
                println!("No projects tracked yet.");
                println!("Projects are automatically detected when you work in an IDE.");
                return Ok(());
            }

            println!("\nTracked Projects:");
            println!("{:\u{2500}<60}", "");
            println!(
                "{:<20} {:<15} {:<15} PATH",
                "NAME", "PM SYSTEM", "PM PROJECT"
            );
            println!("{:\u{2500}<60}", "");

            for project in &projects {
                let pm_system = project.pm_system.as_deref().unwrap_or("-");
                let pm_project = project.pm_project_id.as_deref().unwrap_or("-");
                let path = truncate_str(&project.path, 30);

                println!(
                    "{:<20} {:<15} {:<15} {}",
                    truncate_str(&project.name, 18),
                    pm_system,
                    pm_project,
                    path
                );
            }

            println!(
                "\nTo link a project: toki project link --project <name> --plane-project <IDENTIFIER>"
            );
        }

        ProjectAction::Link {
            project,
            plane_project,
            notion_database,
        } => {
            // Find local project by name
            let local_project = db.get_project_by_name(&project)?;
            let local_project = if let Some(p) = local_project {
                p
            } else {
                println!("Project not found: {project}");
                println!("Run 'toki project list' to see available projects.");
                return Ok(());
            };

            // Determine which PM system to link
            match (plane_project, notion_database) {
                (Some(plane_id), None) => {
                    // Link to Plane.so
                    let config = db.get_integration_config("plane")?;
                    let config = if let Some(c) = config {
                        c
                    } else {
                        println!("Plane.so is not configured.");
                        println!("Run 'toki config set plane.api_key <your-api-key>' first.");
                        return Ok(());
                    };

                    let workspace_slug = config.workspace_slug.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("Plane workspace not configured")
                    })?;

                    // Verify Plane project exists
                    let plane_client = PlaneClient::new(
                        config.api_key.clone(),
                        workspace_slug.clone(),
                        Some(config.api_url.clone()),
                    )?;

                    let plane_projects = plane_client.list_projects().await?;
                    let matching_project = plane_projects
                        .iter()
                        .find(|p| p.identifier.to_uppercase() == plane_id.to_uppercase());

                    let plane_proj = if let Some(p) = matching_project {
                        p
                    } else {
                        println!("Plane project not found: {plane_id}");
                        println!("\nAvailable projects:");
                        for p in &plane_projects {
                            println!("  {} - {}", p.identifier, p.name);
                        }
                        return Ok(());
                    };

                    // Link the project
                    db.link_project_to_pm(
                        local_project.id,
                        "plane",
                        &plane_proj.id.to_string(),
                        Some(workspace_slug),
                    )?;

                    println!(
                        "Linked '{project}' -> Plane project '{}'",
                        plane_proj.identifier
                    );
                    println!("\nNow run 'toki issue-sync' to fetch issues for AI matching.");
                }
                (None, Some(db_id)) => {
                    // Link to Notion database
                    use toki_integrations::NotionClient;

                    let config = db.get_integration_config("notion")?;
                    let config = if let Some(c) = config {
                        c
                    } else {
                        println!("Notion is not configured.");
                        println!(
                            "Run 'toki config set notion.api_key <your-integration-token>' first."
                        );
                        return Ok(());
                    };

                    let notion_client = NotionClient::new(config.api_key.clone())?;

                    // Verify database exists and is accessible
                    let notion_db = match notion_client.get_database(&db_id).await {
                        Ok(d) => d,
                        Err(e) => {
                            println!("Failed to access Notion database: {e}");
                            println!("\nMake sure:");
                            println!("  1. The database ID is correct");
                            println!("  2. Your integration has access to the database");
                            println!("     (Open database -> ... menu -> Add connections)");
                            return Ok(());
                        }
                    };

                    let db_title = notion_db
                        .title
                        .first()
                        .map_or("Untitled", |t| t.plain_text.as_str());

                    // Link the project
                    db.link_project_to_pm(local_project.id, "notion", &db_id, None)?;

                    println!("Linked '{project}' -> Notion database '{db_title}'");
                    println!("\nNow run 'toki issue-sync' to fetch issues for AI matching.");
                }
                (Some(_), Some(_)) => {
                    println!("Error: Cannot specify both --plane-project and --notion-database.");
                    println!("Choose one PM system to link.");
                }
                (None, None) => {
                    println!("Error: Please specify either --plane-project or --notion-database.");
                    println!("\nExamples:");
                    println!("  toki project link --project myapp --plane-project PROJ");
                    println!("  toki project link --project myapp --notion-database abc123...");
                }
            }
        }

        ProjectAction::Unlink { project } => {
            let local_project = db.get_project_by_name(&project)?;
            let local_project = if let Some(p) = local_project {
                p
            } else {
                println!("Project not found: {project}");
                return Ok(());
            };

            db.link_project_to_pm(local_project.id, "", "", None)?;
            println!("Unlinked '{project}' from PM system.");
        }

        ProjectAction::AutoLink {
            min_confidence,
            apply,
        } => {
            use toki_ai::AutoLinker;

            // Get Plane configuration
            let config = db.get_integration_config("plane")?;
            let config = if let Some(c) = config {
                c
            } else {
                println!("Plane.so is not configured.");
                println!("Run 'toki config set plane.api_key <your-api-key>' first.");
                return Ok(());
            };

            let workspace_slug = config
                .workspace_slug
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Plane workspace not configured"))?;

            let plane_client = PlaneClient::new(
                config.api_key.clone(),
                workspace_slug.clone(),
                Some(config.api_url.clone()),
            )?;

            let db_arc = Arc::new(Database::new(None)?);
            let auto_linker = AutoLinker::new(db_arc);

            println!("Analyzing projects for auto-linking...\n");

            // Get suggestions from name matching
            let suggestions = auto_linker
                .suggest_from_name_matching(&plane_client)
                .await?;

            if suggestions.is_empty() {
                println!("No auto-link suggestions found.");
                println!("\nPossible reasons:");
                println!("  - All projects are already linked");
                println!("  - No matching project names found in Plane.so");
                return Ok(());
            }

            println!("Found {} potential link(s):\n", suggestions.len());
            println!(
                "{:<20} {:<15} {:<20} {:<10} REASON",
                "LOCAL PROJECT", "PM IDENTIFIER", "PM PROJECT NAME", "CONFIDENCE"
            );
            println!("{}", "-".repeat(80));

            let applicable: Vec<_> = suggestions
                .iter()
                .filter(|s| s.confidence >= min_confidence)
                .collect();

            for s in &suggestions {
                let conf_str = format!("{:.0}%", s.confidence * 100.0);
                let marker = if s.confidence >= min_confidence {
                    "*"
                } else {
                    " "
                };
                println!(
                    "{}{:<19} {:<15} {:<20} {:<10} {}",
                    marker,
                    truncate_str(&s.local_project_name, 18),
                    s.pm_project_identifier,
                    truncate_str(&s.pm_project_name, 19),
                    conf_str,
                    s.reason
                );
            }

            println!(
                "\n* = above {:.0}% confidence threshold",
                min_confidence * 100.0
            );

            if apply {
                if applicable.is_empty() {
                    println!("\nNo suggestions meet the confidence threshold.");
                    println!("Lower the threshold with --min-confidence or link manually.");
                } else {
                    println!("\nApplying {} link(s)...", applicable.len());

                    for s in applicable {
                        if let Err(e) = auto_linker.apply_suggestion(s, workspace_slug) {
                            println!("  Failed to link '{}': {e}", s.local_project_name);
                        } else {
                            println!(
                                "  Linked '{}' -> {}",
                                s.local_project_name, s.pm_project_identifier
                            );
                        }
                    }

                    println!("\nRun 'toki issue-sync' to fetch issues for AI matching.");
                }
            } else if !applicable.is_empty() {
                println!("\nTo apply these links, run:");
                println!("  toki project auto-link --apply");
            }
        }
    }

    Ok(())
}
