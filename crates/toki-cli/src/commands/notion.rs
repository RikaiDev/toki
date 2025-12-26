/// Notion integration command handler
use std::sync::Arc;

use anyhow::Result;
use clap::Subcommand;
use toki_ai::{NotionIssueSyncService, SyncOptions, SyncOutcome};
use toki_integrations::{GitHubClient, GitLabClient, NotionClient, ProjectManagementSystem};
use toki_storage::Database;

use super::helpers::truncate_str;

#[derive(Subcommand, Debug)]
pub enum NotionAction {
    /// Test API connection
    Test,
    /// List accessible databases
    Databases,
    /// List pages in a database
    Pages {
        /// Database ID
        #[arg(short, long)]
        database: String,
        /// Show detailed schema information
        #[arg(short, long)]
        schema: bool,
    },
    /// Sync Notion database to GitHub Issues
    SyncToGithub {
        /// Notion database ID
        #[arg(short, long)]
        database: String,
        /// GitHub repository (owner/repo format)
        #[arg(short, long)]
        repo: String,
        /// Dry run - show what would be synced without creating issues
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Force sync even for already-synced pages
        #[arg(short, long)]
        force: bool,
        /// Maximum number of issues to create
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Sync Notion database to GitLab Issues
    SyncToGitlab {
        /// Notion database ID
        #[arg(short, long)]
        database: String,
        /// GitLab project (ID or path like group/project)
        #[arg(short, long)]
        project: String,
        /// GitLab API base URL (for self-hosted, defaults to gitlab.com)
        #[arg(long)]
        gitlab_url: Option<String>,
        /// Dry run - show what would be synced without creating issues
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Force sync even for already-synced pages
        #[arg(short, long)]
        force: bool,
        /// Maximum number of issues to create
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Show sync history for a database
    SyncStatus {
        /// Notion database ID
        #[arg(short, long)]
        database: String,
    },
    /// Test updating a page property (for debugging)
    TestUpdate {
        /// Page ID (full UUID)
        #[arg(short, long)]
        page: String,
        /// Property name
        #[arg(long)]
        property: String,
        /// Value to set (number)
        #[arg(short, long)]
        value: f64,
    },
}

pub async fn handle_notion_command(action: NotionAction) -> Result<()> {
    let db = Database::new(None)?;

    // Get Notion configuration
    let config = db.get_integration_config("notion")?;
    let config = if let Some(c) = config {
        c
    } else {
        println!("Notion is not configured.");
        println!("Run 'toki config set notion.api_key <your-integration-token>' first.");
        println!("\nTo get a Notion integration token:");
        println!("  1. Go to https://www.notion.so/my-integrations");
        println!("  2. Create a new integration");
        println!("  3. Copy the Internal Integration Token");
        return Ok(());
    };

    let client = NotionClient::new(config.api_key.clone())?;

    match action {
        NotionAction::Test => {
            println!("Testing Notion API connection...");
            match client.validate_credentials().await {
                Ok(true) => {
                    println!("Connection successful!");
                    // Also show accessible databases count
                    match client.list_databases().await {
                        Ok(databases) => {
                            println!("  Accessible databases: {}", databases.len());
                        }
                        Err(e) => {
                            println!("  (Could not list databases: {e})");
                        }
                    }
                }
                Ok(false) => println!("Connection failed: Invalid credentials"),
                Err(e) => println!("Connection failed: {e}"),
            }
        }
        NotionAction::Databases => {
            println!("Fetching accessible Notion databases...\n");
            let databases = client.list_databases().await?;

            if databases.is_empty() {
                println!("No databases found.");
                println!("\nMake sure you've:");
                println!("  1. Created an integration at https://www.notion.so/my-integrations");
                println!("  2. Connected the integration to your database(s)");
                println!("     (Open database -> ... menu -> Add connections -> Your integration)");
                return Ok(());
            }

            println!("{:<36} {:<40} PROPERTIES", "ID", "TITLE");
            println!("{}", "-".repeat(90));
            for db_item in &databases {
                let title = db_item
                    .title
                    .first()
                    .map(|t| t.plain_text.as_str())
                    .unwrap_or("Untitled");
                let prop_count = db_item.properties.len();
                println!(
                    "{:<36} {:<40} {} props",
                    db_item.id,
                    truncate_str(title, 38),
                    prop_count
                );
            }

            println!("\nTo link a project to a Notion database:");
            println!("  toki project link --project <name> --notion-database <ID>");
        }
        NotionAction::Pages { database, schema } => {
            if schema {
                // Show database schema
                println!("Fetching database schema...\n");
                let db_info = client.get_database(&database).await?;

                let title = db_info
                    .title
                    .first()
                    .map(|t| t.plain_text.as_str())
                    .unwrap_or("Untitled");
                println!("Database: {title}");
                println!("ID: {}", db_info.id);
                println!("\nProperties ({}):", db_info.properties.len());
                println!("{:<30} {:<15} DETECTED AS", "NAME", "TYPE");
                println!("{}", "-".repeat(60));

                // Detect property mapping
                let mapping = db_info.detect_property_mapping(None);

                for (name, schema) in &db_info.properties {
                    let detected = if mapping.title.as_ref() == Some(name) {
                        "-> title"
                    } else if mapping.status.as_ref() == Some(name) {
                        "-> status"
                    } else if mapping.description.as_ref() == Some(name) {
                        "-> description"
                    } else if mapping.time.as_ref() == Some(name) {
                        "-> time"
                    } else if mapping.priority.as_ref() == Some(name) {
                        "-> priority"
                    } else if mapping.assignee.as_ref() == Some(name) {
                        "-> assignee"
                    } else if mapping.due_date.as_ref() == Some(name) {
                        "-> due_date"
                    } else {
                        ""
                    };
                    println!("{:<30} {:<15} {}", name, &schema.property_type, detected);
                }

                println!("\nDetected mapping:");
                println!("  Title: {:?}", mapping.title);
                println!("  Status: {:?}", mapping.status);
                println!("  Description: {:?}", mapping.description);
                println!("  Time: {:?}", mapping.time);
            } else {
                // List pages
                println!("Fetching pages from database...\n");
                let pages = client.query_database_all(&database).await?;

                if pages.is_empty() {
                    println!("No pages found in this database.");
                    return Ok(());
                }

                // Get database info for property mapping
                let db_info = client.get_database(&database).await?;
                let mapping = db_info.detect_property_mapping(None);

                println!("{:<15} {:<50} STATUS", "ID", "TITLE");
                println!("{}", "-".repeat(80));

                for page in &pages {
                    // Extract external_id
                    let external_id = NotionClient::generate_external_id(&database, &page.id);

                    // Extract title using the as_plain_text method
                    let title = mapping
                        .title
                        .as_ref()
                        .and_then(|prop_name| page.properties.get(prop_name))
                        .and_then(|v| v.as_plain_text())
                        .unwrap_or_else(|| "Untitled".to_string());

                    // Extract status using the as_select_name method
                    let status = mapping
                        .status
                        .as_ref()
                        .and_then(|prop_name| page.properties.get(prop_name))
                        .and_then(|v| v.as_select_name())
                        .unwrap_or_else(|| "-".to_string());

                    println!(
                        "{:<15} {:<50} {}",
                        external_id,
                        truncate_str(&title, 48),
                        status
                    );
                }

                println!("\nTotal: {} pages", pages.len());
            }
        }
        NotionAction::SyncToGithub {
            database,
            repo,
            dry_run,
            force,
            limit,
        } => {
            // Get GitHub token
            let github_config = db.get_integration_config("github")?;
            let github_token = if let Some(c) = github_config {
                c.api_key
            } else {
                println!("GitHub is not configured.");
                println!("Run 'toki config set github.token <your-token>' first.");
                return Ok(());
            };

            let github_client = GitHubClient::new(github_token, repo.clone())?;
            let notion_client = Arc::new(client);
            let db_arc = Arc::new(db);
            let sync_service = NotionIssueSyncService::new(notion_client, db_arc);

            let options = SyncOptions {
                dry_run,
                force,
                limit,
                status_filter: Vec::new(),
            };

            println!(
                "{}Syncing Notion database {} to GitHub repo {}...\n",
                if dry_run { "[DRY RUN] " } else { "" },
                database,
                repo
            );

            let (report, results) = sync_service
                .sync_database(&database, &github_client, &repo, None, &options)
                .await?;

            // Print results
            for result in &results {
                match &result.outcome {
                    SyncOutcome::Created { issue_number, issue_url } => {
                        println!("  [CREATED] #{} {} -> {}", issue_number, result.title, issue_url);
                    }
                    SyncOutcome::Skipped { reason } => {
                        println!("  [SKIPPED] {} - {}", result.title, reason);
                    }
                    SyncOutcome::Failed { error } => {
                        println!("  [FAILED] {} - {}", result.title, error);
                    }
                    SyncOutcome::WouldCreate => {
                        println!("  [WOULD CREATE] {}", result.title);
                    }
                }
            }

            println!("\nSummary:");
            println!("  Created: {}", report.created);
            println!("  Skipped: {}", report.skipped);
            println!("  Failed:  {}", report.failed);
        }
        NotionAction::SyncToGitlab {
            database,
            project,
            gitlab_url,
            dry_run,
            force,
            limit,
        } => {
            // Get GitLab token
            let gitlab_config = db.get_integration_config("gitlab")?;
            let gitlab_token = if let Some(c) = gitlab_config {
                c.api_key
            } else {
                println!("GitLab is not configured.");
                println!("Run 'toki config set gitlab.token <your-token>' first.");
                return Ok(());
            };

            let gitlab_client = if let Some(url) = gitlab_url {
                GitLabClient::with_base_url(gitlab_token, project.clone(), url)?
            } else {
                GitLabClient::new(gitlab_token, project.clone())?
            };

            let notion_client = Arc::new(client);
            let db_arc = Arc::new(db);
            let sync_service = NotionIssueSyncService::new(notion_client, db_arc);

            let options = SyncOptions {
                dry_run,
                force,
                limit,
                status_filter: Vec::new(),
            };

            println!(
                "{}Syncing Notion database {} to GitLab project {}...\n",
                if dry_run { "[DRY RUN] " } else { "" },
                database,
                project
            );

            let (report, results) = sync_service
                .sync_database(&database, &gitlab_client, &project, None, &options)
                .await?;

            // Print results
            for result in &results {
                match &result.outcome {
                    SyncOutcome::Created { issue_number, issue_url } => {
                        println!("  [CREATED] #{} {} -> {}", issue_number, result.title, issue_url);
                    }
                    SyncOutcome::Skipped { reason } => {
                        println!("  [SKIPPED] {} - {}", result.title, reason);
                    }
                    SyncOutcome::Failed { error } => {
                        println!("  [FAILED] {} - {}", result.title, error);
                    }
                    SyncOutcome::WouldCreate => {
                        println!("  [WOULD CREATE] {}", result.title);
                    }
                }
            }

            println!("\nSummary:");
            println!("  Created: {}", report.created);
            println!("  Skipped: {}", report.skipped);
            println!("  Failed:  {}", report.failed);
        }
        NotionAction::SyncStatus { database } => {
            let synced = db.get_synced_issues_for_database(&database)?;

            if synced.is_empty() {
                println!("No sync history found for database {}.", database);
                return Ok(());
            }

            println!(
                "Sync history for database {} ({} issues synced):\n",
                database,
                synced.len()
            );

            println!(
                "{:<15} {:<10} {:<30} {:<40}",
                "ISSUE #", "TARGET", "TITLE", "URL"
            );
            println!("{}", "-".repeat(95));

            for issue in &synced {
                println!(
                    "{:<15} {:<10} {:<30} {:<40}",
                    format!("#{}", issue.target_issue_number),
                    &issue.target_system,
                    truncate_str(&issue.title, 28),
                    truncate_str(&issue.target_issue_url, 38)
                );
            }
        }
        NotionAction::TestUpdate {
            page,
            property,
            value,
        } => {
            use toki_integrations::notion::NotionPropertyUpdate;

            println!("Testing property update...");
            println!("  Page: {page}");
            println!("  Property: {property}");
            println!("  Value: {value}");

            let update = NotionPropertyUpdate::Number(value);
            match client.update_page_property(&page, &property, update).await {
                Ok(_page) => {
                    println!("\nUpdate successful!");
                }
                Err(e) => {
                    println!("\nUpdate failed: {e}");
                }
            }
        }
    }

    Ok(())
}
