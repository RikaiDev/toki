/// Issue sync command handler - sync issues from PM systems for AI matching
use anyhow::Result;
use std::sync::Arc;
use toki_ai::IssueSyncService;
use toki_integrations::{NotionClient, PlaneClient};
use toki_storage::Database;

pub async fn handle_issue_sync_command(force: bool) -> Result<()> {
    let db = Arc::new(Database::new(None)?);

    // Check if we have any linked projects
    let linked_projects = db.get_projects_with_pm_link()?;
    if linked_projects.is_empty() {
        println!("No projects linked to any PM system.");
        println!("\nLink a project first:");
        println!("  toki project link --project <name> --plane-project <IDENTIFIER>");
        println!("  toki project link --project <name> --notion-database <ID>");
        return Ok(());
    }

    // Count projects by PM system
    let plane_count = linked_projects
        .iter()
        .filter(|p| p.pm_system.as_deref() == Some("plane"))
        .count();
    let notion_count = linked_projects
        .iter()
        .filter(|p| p.pm_system.as_deref() == Some("notion"))
        .count();

    println!("Syncing issues from PM systems...");
    println!(
        "  Linked projects: {} (Plane: {}, Notion: {})",
        linked_projects.len(),
        plane_count,
        notion_count
    );

    // Initialize clients based on what's configured and needed
    let plane_client = if plane_count > 0 {
        if let Some(config) = db.get_integration_config("plane")? {
            if let Some(workspace_slug) = &config.workspace_slug {
                println!("  Plane workspace: {workspace_slug}");
                Some(PlaneClient::new(
                    config.api_key.clone(),
                    workspace_slug.clone(),
                    Some(config.api_url.clone()),
                )?)
            } else {
                println!("  Warning: Plane workspace not configured");
                None
            }
        } else {
            println!("  Warning: Plane not configured");
            None
        }
    } else {
        None
    };

    let notion_client = if notion_count > 0 {
        if let Some(config) = db.get_integration_config("notion")? {
            if !config.api_key.is_empty() {
                println!("  Notion: configured");
                Some(NotionClient::new(config.api_key.clone())?)
            } else {
                println!("  Warning: Notion API key not set");
                None
            }
        } else {
            println!("  Warning: Notion not configured");
            None
        }
    } else {
        None
    };

    // Create sync service
    let sync_service = IssueSyncService::new(db.clone())?;

    // Sync all linked projects (both Plane and Notion)
    let stats = sync_service
        .sync_all_linked_projects_multi(plane_client.as_ref(), notion_client.as_ref())
        .await?;

    println!("\nSync complete:");
    println!("  Issues synced: {}", stats.issues_synced);
    println!("  Issues updated: {}", stats.issues_updated);
    println!("  Embeddings computed: {}", stats.embeddings_computed);

    if !stats.errors.is_empty() {
        println!("\nWarnings:");
        for err in &stats.errors {
            println!("  - {err}");
        }
    }

    // If force, recompute missing embeddings
    if force {
        println!("\nRecomputing missing embeddings...");
        let computed = sync_service.recompute_missing_embeddings()?;
        println!("  Computed: {computed} embeddings");
    }

    Ok(())
}
