/// Notion integration command handler
use std::sync::Arc;

use anyhow::Result;
use clap::Subcommand;
use toki_ai::{NotionIssueSyncService, SyncOptions, SyncOutcome, SyncResult};
use toki_integrations::{GitHubClient, GitLabClient, IssueSyncReport, NotionClient, ProjectManagementSystem};
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

/// Print sync results
fn print_sync_results(results: &[SyncResult], report: &IssueSyncReport) {
    for result in results {
        match &result.outcome {
            SyncOutcome::Created { issue_number, issue_url } => {
                println!("  [CREATED] #{issue_number} {} -> {issue_url}", result.title);
            }
            SyncOutcome::Skipped { reason } => {
                println!("  [SKIPPED] {} - {reason}", result.title);
            }
            SyncOutcome::Failed { error } => {
                println!("  [FAILED] {} - {error}", result.title);
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

async fn handle_test(client: &NotionClient) {
    println!("Testing Notion API connection...");
    match client.validate_credentials().await {
        Ok(true) => {
            println!("Connection successful!");
            if let Ok(databases) = client.list_databases().await {
                println!("  Accessible databases: {}", databases.len());
            }
        }
        Ok(false) => println!("Connection failed: Invalid credentials"),
        Err(e) => println!("Connection failed: {e}"),
    }
}

async fn handle_databases(client: &NotionClient) -> Result<()> {
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
        let title = db_item.title.first().map_or("Untitled", |t| t.plain_text.as_str());
        println!(
            "{:<36} {:<40} {} props",
            db_item.id,
            truncate_str(title, 38),
            db_item.properties.len()
        );
    }

    println!("\nTo link a project to a Notion database:");
    println!("  toki project link --project <name> --notion-database <ID>");
    Ok(())
}

async fn handle_pages_schema(client: &NotionClient, database: &str) -> Result<()> {
    println!("Fetching database schema...\n");
    let db_info = client.get_database(database).await?;

    let title = db_info.title.first().map_or("Untitled", |t| t.plain_text.as_str());
    println!("Database: {title}");
    println!("ID: {}", db_info.id);
    println!("\nProperties ({}):", db_info.properties.len());
    println!("{:<30} {:<15} DETECTED AS", "NAME", "TYPE");
    println!("{}", "-".repeat(60));

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
        println!("{:<30} {:<15} {detected}", name, &schema.property_type);
    }

    println!("\nDetected mapping:");
    println!("  Title: {:?}", mapping.title);
    println!("  Status: {:?}", mapping.status);
    println!("  Description: {:?}", mapping.description);
    println!("  Time: {:?}", mapping.time);
    Ok(())
}

async fn handle_pages_list(client: &NotionClient, database: &str) -> Result<()> {
    println!("Fetching pages from database...\n");
    let pages = client.query_database_all(database).await?;

    if pages.is_empty() {
        println!("No pages found in this database.");
        return Ok(());
    }

    let db_info = client.get_database(database).await?;
    let mapping = db_info.detect_property_mapping(None);

    println!("{:<15} {:<50} STATUS", "ID", "TITLE");
    println!("{}", "-".repeat(80));

    for page in &pages {
        let external_id = NotionClient::generate_external_id(database, &page.id);
        let title = mapping.title.as_ref()
            .and_then(|prop_name| page.properties.get(prop_name))
            .and_then(toki_integrations::NotionPropertyValue::as_plain_text)
            .unwrap_or_else(|| "Untitled".to_string());
        let status = mapping.status.as_ref()
            .and_then(|prop_name| page.properties.get(prop_name))
            .and_then(toki_integrations::NotionPropertyValue::as_select_name)
            .unwrap_or_else(|| "-".to_string());

        println!("{:<15} {:<50} {status}", external_id, truncate_str(&title, 48));
    }

    println!("\nTotal: {} pages", pages.len());
    Ok(())
}

/// Common sync parameters
struct SyncParams {
    database: String,
    dry_run: bool,
    force: bool,
    limit: Option<usize>,
}

async fn handle_sync_to_github(
    client: NotionClient,
    db: Database,
    params: SyncParams,
    repo: String,
) -> Result<()> {
    let Some(github_config) = db.get_integration_config("github")? else {
        println!("GitHub is not configured.");
        println!("Run 'toki config set github.token <your-token>' first.");
        return Ok(());
    };
    let github_token = github_config.api_key;

    let github_client = GitHubClient::new(&github_token, repo.clone())?;
    let sync_service = NotionIssueSyncService::new(Arc::new(client), Arc::new(db));
    let options = SyncOptions {
        dry_run: params.dry_run,
        force: params.force,
        limit: params.limit,
        status_filter: Vec::new(),
    };

    println!(
        "{}Syncing Notion database {} to GitHub repo {repo}...\n",
        if params.dry_run { "[DRY RUN] " } else { "" },
        params.database
    );

    let (report, results) = sync_service
        .sync_database(&params.database, &github_client, &repo, None, &options)
        .await?;

    print_sync_results(&results, &report);
    Ok(())
}

async fn handle_sync_to_gitlab(
    client: NotionClient,
    db: Database,
    params: SyncParams,
    project: String,
    gitlab_url: Option<String>,
) -> Result<()> {
    let Some(gitlab_config) = db.get_integration_config("gitlab")? else {
        println!("GitLab is not configured.");
        println!("Run 'toki config set gitlab.token <your-token>' first.");
        return Ok(());
    };
    let gitlab_token = gitlab_config.api_key;

    let gitlab_client = match &gitlab_url {
        Some(url) => GitLabClient::with_base_url(&gitlab_token, &project, url)?,
        None => GitLabClient::new(&gitlab_token, &project)?,
    };

    let sync_service = NotionIssueSyncService::new(Arc::new(client), Arc::new(db));
    let options = SyncOptions {
        dry_run: params.dry_run,
        force: params.force,
        limit: params.limit,
        status_filter: Vec::new(),
    };

    println!(
        "{}Syncing Notion database {} to GitLab project {project}...\n",
        if params.dry_run { "[DRY RUN] " } else { "" },
        params.database
    );

    let (report, results) = sync_service
        .sync_database(&params.database, &gitlab_client, &project, None, &options)
        .await?;

    print_sync_results(&results, &report);
    Ok(())
}

fn handle_sync_status(db: &Database, database: &str) -> Result<()> {
    let synced = db.get_synced_issues_for_database(database)?;

    if synced.is_empty() {
        println!("No sync history found for database {database}.");
        return Ok(());
    }

    println!("Sync history for database {database} ({} issues synced):\n", synced.len());
    println!("{:<15} {:<10} {:<30} {:<40}", "ISSUE #", "TARGET", "TITLE", "URL");
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
    Ok(())
}

async fn handle_test_update(client: &NotionClient, page: &str, property: &str, value: f64) {
    use toki_integrations::notion::NotionPropertyUpdate;

    println!("Testing property update...");
    println!("  Page: {page}");
    println!("  Property: {property}");
    println!("  Value: {value}");

    let update = NotionPropertyUpdate::Number(value);
    match client.update_page_property(page, property, update).await {
        Ok(_) => println!("\nUpdate successful!"),
        Err(e) => println!("\nUpdate failed: {e}"),
    }
}

pub async fn handle_notion_command(action: NotionAction) -> Result<()> {
    let db = Database::new(None)?;

    // Get Notion configuration
    let Some(config) = db.get_integration_config("notion")? else {
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
        NotionAction::Test => handle_test(&client).await,
        NotionAction::Databases => handle_databases(&client).await?,
        NotionAction::Pages { database, schema } => {
            if schema {
                handle_pages_schema(&client, &database).await?;
            } else {
                handle_pages_list(&client, &database).await?;
            }
        }
        NotionAction::SyncToGithub { database, repo, dry_run, force, limit } => {
            let params = SyncParams { database, dry_run, force, limit };
            handle_sync_to_github(client, db, params, repo).await?;
        }
        NotionAction::SyncToGitlab { database, project, gitlab_url, dry_run, force, limit } => {
            let params = SyncParams { database, dry_run, force, limit };
            handle_sync_to_gitlab(client, db, params, project, gitlab_url).await?;
        }
        NotionAction::SyncStatus { database } => handle_sync_status(&db, &database)?,
        NotionAction::TestUpdate { page, property, value } => {
            handle_test_update(&client, &page, &property, value).await;
        }
    }

    Ok(())
}
