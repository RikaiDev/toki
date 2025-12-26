mod commands;

use anyhow::Result;
use chrono::Duration;
use clap::{Parser, Subcommand};
use std::{env, fs, io, path::Path, process::Command, thread::sleep, time};
use sysinfo::{Pid, System};
use tabled::{Table, Tabled};
use toki_ai::InsightsGenerator;
use std::sync::Arc;
use toki_core::{
    config::get_data_dir,
    ipc::{IpcClient, IpcRequest, IpcResponse},
    Daemon,
};
use toki_storage::Database;

/// Safely truncate a string to a maximum number of characters (not bytes).
/// This avoids panics when slicing multi-byte UTF-8 characters.
fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...")
    } else {
        s.to_string()
    }
}

#[derive(Parser)]
#[command(name = "toki")]
#[command(about = "Time tracking daemon", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize toki (first-time setup)
    Init {
        /// Enable database encryption
        #[arg(short, long)]
        encrypt: bool,
    },
    /// Start the tracking daemon
    Start,
    /// (Internal) Run the daemon process
    #[command(hide = true)]
    DaemonInternalStart,
    /// Stop the tracking daemon
    Stop,
    /// Check daemon status and today's statistics
    Status,
    /// Generate time tracking report
    Report {
        /// Time period: today, week, month, or custom
        #[arg(default_value = "today")]
        period: String,
    },
    /// Manage category rules
    Categories,
    /// Data management commands
    Data {
        #[command(subcommand)]
        action: DataAction,
    },
    /// Privacy settings
    Privacy {
        #[command(subcommand)]
        action: Option<PrivacyAction>,
    },
    /// Synchronize time entries to PM system
    Sync {
        /// PM system type (plane, github, jira)
        #[arg(default_value = "plane")]
        system: String,
        /// Dry run (don't actually sync)
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Only sync confirmed/reviewed time blocks
        #[arg(short, long)]
        reviewed: bool,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Plane.so integration commands
    Plane {
        #[command(subcommand)]
        action: PlaneAction,
    },
    /// Notion integration commands
    Notion {
        #[command(subcommand)]
        action: NotionAction,
    },
    /// Review and confirm daily activity for syncing
    Review {
        /// Date to review (YYYY-MM-DD format, defaults to today)
        #[arg(short, long)]
        date: Option<String>,
        /// Show detailed breakdown
        #[arg(short, long)]
        verbose: bool,
        /// Confirm all suggested blocks (save for syncing)
        #[arg(long)]
        confirm_all: bool,
    },
    /// Teach toki to classify activities (deprecated, use auto-inference)
    #[command(hide = true)]
    Learn {
        #[command(subcommand)]
        action: LearnAction,
    },
    /// Sync issues from PM system for AI matching
    IssueSync {
        /// Force full resync (recompute all embeddings)
        #[arg(short, long)]
        force: bool,
    },
    /// Project management commands
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
}

#[derive(Subcommand, Debug)]
enum ProjectAction {
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

#[derive(Subcommand, Debug)]
enum LearnAction {
    /// Add a classification rule
    Add {
        /// Pattern to match (domain, keyword, etc.)
        pattern: String,
        /// Category to assign (Break, Research, Coding, etc.)
        category: String,
        /// Pattern type: domain, `window_title`, `bundle_id`, `url_path`
        #[arg(short = 't', long, default_value = "window_title")]
        pattern_type: String,
    },
    /// List all learned rules
    List,
    /// Delete a learned rule by ID or pattern
    Delete {
        /// Rule ID or pattern to delete
        identifier: String,
    },
}

#[derive(Subcommand, Debug)]
enum DataAction {
    /// Export data to JSON or CSV
    Export {
        /// Output format: json or csv
        format: String,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Delete data for specified period
    Delete {
        /// Time period to delete
        period: String,
    },
}

#[derive(Subcommand, Debug)]
enum PrivacyAction {
    /// Pause tracking
    Pause,
    /// Resume tracking
    Resume,
    /// List excluded apps
    ListExcluded,
    /// Add app to exclusion list
    Exclude {
        /// App bundle ID or name
        app: String,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// Get a configuration value
    Get {
        /// Configuration key (e.g., `plane.api_key`)
        key: String,
    },
    /// Set a configuration value
    Set {
        /// Configuration key (e.g., `plane.api_key`)
        key: String,
        /// Value to set
        value: String,
    },
    /// List all configuration
    List,
}

#[derive(Subcommand, Debug)]
enum PlaneAction {
    /// List all projects in the workspace
    Projects,
    /// List work items (issues) in a project
    Issues {
        /// Project identifier (e.g., "PROJ" or project UUID)
        #[arg(short, long)]
        project: Option<String>,
        /// Search query
        #[arg(short, long)]
        search: Option<String>,
    },
    /// Show my assigned work items
    MyIssues,
    /// Test API connection
    Test,
}

#[derive(Subcommand, Debug)]
enum NotionAction {
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
}

#[derive(Tabled)]
struct CategoryStats {
    #[tabled(rename = "Category")]
    category: String,
    #[tabled(rename = "Time (minutes)")]
    time_minutes: u32,
    #[tabled(rename = "Percentage")]
    percentage: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    use chrono::Utc;
    let cli = Cli::parse();

    if !matches!(cli.command, Commands::DaemonInternalStart) {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp_secs()
            .init();
    }

    let data_dir = get_data_dir()?;

    match cli.command {
        Commands::Init { encrypt } => commands::init::init_command(encrypt),
        Commands::Start => start_daemon(&data_dir),
        Commands::DaemonInternalStart => run_daemon_process().await,
        Commands::Stop => stop_daemon(&data_dir).await,
        Commands::Status => show_status(&data_dir).await,
        Commands::Report { period } => {
            let db = Database::new(None)?;

            let (start, end) = match period.as_str() {
                "today" => {
                    let start = Utc::now()
                        .date_naive()
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc();
                    let end = Utc::now();
                    (start, end)
                }
                "week" => {
                    let end = Utc::now();
                    let start = end - Duration::days(7);
                    (start, end)
                }
                "month" => {
                    let end = Utc::now();
                    let start = end - Duration::days(30);
                    (start, end)
                }
                _ => {
                    println!("Unknown period: {period}. Use 'today', 'week', or 'month'");
                    return Ok(());
                }
            };

            // Use activity_spans for more accurate data
            let spans = db.get_activity_spans(start, end)?;

            if spans.is_empty() {
                println!("No activities recorded for period: {period}");
                return Ok(());
            }

            let category_time = InsightsGenerator::time_per_category_from_spans(&spans);
            let total_time: u32 = category_time.values().sum();

            println!("\nTime Tracking Report: {period}");
            println!("\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");

            let mut stats: Vec<CategoryStats> = category_time
                .into_iter()
                .map(|(category, seconds)| {
                    let minutes = seconds / 60;
                    let percentage = if total_time > 0 {
                        format!(
                            "{:.1}%",
                            (f64::from(seconds) / f64::from(total_time)) * 100.0
                        )
                    } else {
                        String::from("0%")
                    };
                    CategoryStats {
                        category,
                        time_minutes: minutes,
                        percentage,
                    }
                })
                .collect();

            stats.sort_by(|a, b| b.time_minutes.cmp(&a.time_minutes));

            let table = Table::new(stats).to_string();
            println!("\n{table}");

            println!("\nTotal tracked time: {} minutes", total_time / 60);

            Ok(())
        }
        Commands::Categories => {
            let db = Database::new(None)?;
            let categories = db.get_categories()?;

            println!("Category Rules");
            println!("\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
            for category in categories {
                println!("\n{}", category.name);
                println!("  Pattern: {}", category.pattern);
                if let Some(desc) = category.description {
                    println!("  Description: {desc}");
                }
            }

            Ok(())
        }
        Commands::Data { action } => match action {
            DataAction::Export { format, output } => {
                let db = Database::new(None)?;
                let end = Utc::now();
                let start = end - Duration::days(365);

                let output_path = output.unwrap_or_else(|| format!("toki_export.{format}"));

                match format.as_str() {
                    "json" => {
                        let activities = db.get_activities(start, end)?;
                        let json = serde_json::to_string_pretty(&activities)?;
                        std::fs::write(&output_path, json)?;
                        println!("Exported {} activities to {output_path}", activities.len());
                    }
                    "csv" => {
                        // Export activity spans (more detailed data)
                        let spans = db.get_activity_spans(start, end)?;
                        let mut csv_content = String::from("id,app_bundle_id,category,start_time,end_time,duration_seconds,work_item_id,session_id\n");

                        for span in &spans {
                            csv_content.push_str(&format!(
                                "{},{},{},{},{},{},{},{}\n",
                                span.id,
                                escape_csv(&span.app_bundle_id),
                                escape_csv(&span.category),
                                span.start_time.to_rfc3339(),
                                span.end_time.map(|t| t.to_rfc3339()).unwrap_or_default(),
                                span.duration_seconds,
                                span.work_item_id
                                    .map(|id| id.to_string())
                                    .unwrap_or_default(),
                                span.session_id.map(|id| id.to_string()).unwrap_or_default(),
                            ));
                        }

                        std::fs::write(&output_path, csv_content)?;
                        println!("Exported {} activity spans to {output_path}", spans.len());
                    }
                    _ => {
                        println!("Unknown format: {format}. Use 'json' or 'csv'");
                    }
                }

                Ok(())
            }
            DataAction::Delete { period } => {
                let db = Database::new(None)?;

                let (start, end) = match period.as_str() {
                    "today" => {
                        let start = Utc::now()
                            .date_naive()
                            .and_hms_opt(0, 0, 0)
                            .unwrap()
                            .and_utc();
                        let end = Utc::now();
                        (start, end)
                    }
                    "week" => {
                        let end = Utc::now();
                        let start = end - Duration::days(7);
                        (start, end)
                    }
                    "all" => {
                        let end = Utc::now();
                        let start = Utc::now() - Duration::days(3650); // 10 years
                        (start, end)
                    }
                    _ => {
                        println!("Unknown period: {period}. Use 'today', 'week', or 'all'");
                        return Ok(());
                    }
                };

                let deleted = db.delete_activities(start, end)?;
                println!("Deleted {deleted} activities");

                Ok(())
            }
        },
        Commands::Privacy { action } => {
            let db = Database::new(None)?;
            let mut settings = db.get_settings()?;

            match action {
                Some(PrivacyAction::Pause) => {
                    settings.pause_tracking = true;
                    db.update_settings(&settings)?;
                    println!("Tracking paused");
                }
                Some(PrivacyAction::Resume) => {
                    settings.pause_tracking = false;
                    db.update_settings(&settings)?;
                    println!("Tracking resumed");
                }
                Some(PrivacyAction::ListExcluded) => {
                    println!("Excluded applications:");
                    for app in &settings.excluded_apps {
                        println!("  - {app}");
                    }
                }
                Some(PrivacyAction::Exclude { app }) => {
                    if settings.excluded_apps.contains(&app) {
                        println!("'{app}' already in exclusion list");
                    } else {
                        settings.excluded_apps.push(app.clone());
                        db.update_settings(&settings)?;
                        println!("Added '{app}' to exclusion list");
                    }
                }
                None => {
                    println!("Privacy Settings");
                    println!("\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
                    println!(
                        "Tracking: {}",
                        if settings.pause_tracking {
                            "PAUSED"
                        } else {
                            "ACTIVE"
                        }
                    );
                    println!(
                        "Idle threshold: {} seconds",
                        settings.idle_threshold_seconds
                    );
                    println!("Excluded apps: {}", settings.excluded_apps.len());
                }
            }

            Ok(())
        }
        Commands::Sync { system, dry_run, reviewed } => {
            use toki_integrations::{PlaneClient, ProjectManagementSystem, TimeEntry};

            let db = Database::new(None)?;

            let config = db
                .get_integration_config(&system)?
                .ok_or_else(|| anyhow::anyhow!("No configuration found for system: {system}"))?;

            println!("Synchronizing with {}...", config.system_type);

            if dry_run {
                println!("  (Dry run mode - no actual changes will be made)");
            }
            if reviewed {
                println!("  (Syncing only reviewed/confirmed time blocks)");
            }

            let sync_result = match config.system_type.as_str() {
                "plane" => {
                    let workspace_slug = config.workspace_slug.clone().ok_or_else(|| {
                        anyhow::anyhow!(
                            "Workspace not configured. Run: toki config set plane.workspace <slug>"
                        )
                    })?;

                    let client = PlaneClient::new(
                        config.api_key.clone(),
                        workspace_slug,
                        Some(config.api_url.clone()),
                    )?;

                    let mut time_entries = Vec::new();

                    if reviewed {
                        // Only sync confirmed time blocks
                        let time_blocks = db.get_confirmed_time_blocks()?;
                        
                        for block in time_blocks {
                            // Get the first associated work item
                            if let Some(work_item_id) = block.work_item_ids.first() {
                                if let Some(work_item) = db.get_work_item_by_id(*work_item_id)? {
                                    time_entries.push(TimeEntry {
                                        work_item_id: work_item.external_id.clone(),
                                        start_time: block.start_time,
                                        duration_seconds: (block.end_time - block.start_time).num_seconds() as u32,
                                        description: block.description.clone(),
                                        category: block.tags.first().cloned().unwrap_or_else(|| "Development".to_string()),
                                    });
                                }
                            }
                        }
                    } else {
                        // Original behavior - sync all work items with activities
                        let work_items = db.get_all_work_items()?;

                        for work_item in work_items {
                            let activities = db.get_activities_by_work_item(work_item.id)?;

                            if activities.is_empty() {
                                continue;
                            }

                            for activity in &activities {
                                time_entries.push(TimeEntry {
                                    work_item_id: work_item.external_id.clone(),
                                    start_time: activity.timestamp,
                                    duration_seconds: activity.duration_seconds,
                                    description: format!("Auto-tracked by Toki: {}", activity.category),
                                    category: activity.category.clone(),
                                });
                            }
                        }
                    }

                    if time_entries.is_empty() {
                        if reviewed {
                            println!("No confirmed time blocks to sync.");
                            println!("Run 'toki review' to review and confirm time blocks first.");
                        } else {
                            println!("No time entries to sync.");
                        }
                        return Ok(());
                    }

                    println!("Found {} time entries to sync", time_entries.len());

                    if dry_run {
                        use toki_integrations::SyncReport;
                        Ok(SyncReport::new(0))
                    } else {
                        client.batch_sync(time_entries).await
                    }
                }
                _ => {
                    anyhow::bail!("Unsupported PM system: {}", config.system_type);
                }
            }?;

            if !dry_run {
                println!("Sync complete!");
                println!("  Success: {}", sync_result.successful);
                println!("  Failed: {}", sync_result.failed);
                if !sync_result.errors.is_empty() {
                    println!("\nErrors:");
                    for error in sync_result.errors {
                        println!("  - {error}");
                    }
                }
            }

            Ok(())
        }
        Commands::Config { action } => {
            let db = Database::new(None)?;

            match action {
                ConfigAction::Get { key } => {
                    let value = get_config_value(&db, &key)?;
                    match value {
                        Some(v) => println!("{key} = {v}"),
                        None => println!("{key} is not set"),
                    }
                }
                ConfigAction::Set { key, value } => {
                    set_config_value(&db, &key, &value)?;
                    println!("Set {key} = {value}");
                }
                ConfigAction::List => {
                    println!("Configuration:");
                    println!("\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");

                    // List Plane integration config
                    if let Ok(Some(config)) = db.get_integration_config("plane") {
                        println!("\n[plane]");
                        println!("  api_url = {}", config.api_url);
                        println!(
                            "  api_key = {}***",
                            &config.api_key.chars().take(8).collect::<String>()
                        );
                        if let Some(ws) = config.workspace_slug {
                            println!("  workspace = {ws}");
                        }
                    }

                    // List Notion integration config
                    if let Ok(Some(config)) = db.get_integration_config("notion") {
                        println!("\n[notion]");
                        if !config.api_key.is_empty() {
                            println!(
                                "  api_key = {}***",
                                &config.api_key.chars().take(8).collect::<String>()
                            );
                        }
                        if let Some(db_id) = &config.project_id {
                            println!("  database_id = {db_id}");
                        }
                        // time_property is stored in workspace_slug for Notion
                        if let Some(tp) = &config.workspace_slug {
                            println!("  time_property = {tp}");
                        }
                    }

                    // List settings
                    let settings = db.get_settings()?;
                    println!("\n[settings]");
                    println!(
                        "  idle_threshold_seconds = {}",
                        settings.idle_threshold_seconds
                    );
                    println!(
                        "  work_item_tracking = {}",
                        settings.enable_work_item_tracking
                    );
                    println!("  capture_window_title = {}", settings.capture_window_title);
                }
            }

            Ok(())
        }
        Commands::Plane { action } => handle_plane_command(action).await,
        Commands::Review { date, verbose, confirm_all } => {
            handle_review_command(date, verbose, confirm_all).await
        }
        Commands::Learn { action } => handle_learn_command(action),
        Commands::IssueSync { force } => handle_issue_sync_command(force).await,
        Commands::Project { action } => handle_project_command(action).await,
        Commands::Notion { action } => handle_notion_command(action).await,
    }
}

fn start_daemon(data_dir: &Path) -> Result<()> {
    let pid_file_path = data_dir.join("toki.pid");
    let sock_path = data_dir.join("toki.sock");

    // 1. Check if daemon is already running
    if pid_file_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(&pid_file_path) {
            if let Ok(pid) = pid_str.trim().parse::<usize>() {
                let mut sys = System::new();
                if sys.refresh_process(Pid::from(pid)) {
                    log::info!("Daemon is already running (PID: {pid}).");
                    return Ok(());
                }
            }
        }
        // If pid file is stale, remove it
        log::warn!("Removing stale PID file.");
        let _ = fs::remove_file(&pid_file_path);
    }

    // 2. Clean up old socket if it exists
    if sock_path.exists() {
        log::warn!("Removing stale socket file.");
        fs::remove_file(&sock_path)?;
    }

    log::info!("Starting Toki daemon...");

    // 3. Spawn a new process for the daemon
    let current_exe = env::current_exe()?;
    let current_dir = env::current_dir()?;
    let child = Command::new(current_exe)
        .arg("daemon-internal-start")
        .current_dir(current_dir) // Explicitly set the working directory for the daemon
        .spawn()?;

    // 4. In parent process, write PID and exit
    log::info!("Daemon process started with PID: {}", child.id());
    fs::write(&pid_file_path, child.id().to_string())?;

    Ok(())
}

async fn run_daemon_process() -> Result<()> {
    // This is the detached daemon process
    // We must set up logging here, as this is a new process.
    if let Err(e) = setup_daemon_logging() {
        // If logging fails, we have no way to report errors. Panicking is the only option.
        panic!("Failed to set up daemon logging: {e}");
    }
    log::info!("Daemon process started internally.");

    if let Err(e) = daemon_main_logic().await {
        log::error!("Daemon main logic exited with a fatal error: {e:#}");
        return Err(e);
    }

    Ok(())
}

async fn daemon_main_logic() -> Result<()> {
    let db = Database::new(None)?;
    let mut daemon = Daemon::new(db, 10)?;
    daemon.run_with_signals().await
}

async fn stop_daemon(data_dir: &Path) -> Result<()> {
    let pid_file_path = data_dir.join("toki.pid");
    let sock_path = data_dir.join("toki.sock");

    if !pid_file_path.exists() {
        log::info!("Daemon is not running (no PID file).");
        // Also remove socket if it exists for consistency
        if sock_path.exists() {
            fs::remove_file(&sock_path)?;
        }
        return Ok(());
    }

    let pid_str = fs::read_to_string(&pid_file_path)?;
    let pid = pid_str
        .trim()
        .parse::<usize>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    log::info!("Stopping Toki daemon (PID: {pid})...");
    let client = IpcClient::new(&sock_path);

    match client.send_command(IpcRequest::Shutdown).await {
        Ok(IpcResponse::Shutdown) => {
            log::info!("Daemon shutdown signal sent. Waiting for process to exit...");
            sleep(time::Duration::from_secs(2));

            let mut sys = System::new();
            if sys.refresh_process(Pid::from(pid)) {
                log::warn!("Daemon did not stop gracefully. Force killing...");
                if let Some(process) = sys.process(Pid::from(pid)) {
                    process.kill();
                }
            } else {
                log::info!("Daemon stopped successfully.");
            }
        }
        Ok(resp) => log::error!("Received unexpected response from daemon: {resp:?}"),
        Err(e) => {
            log::error!("Failed to send shutdown command: {e}. Forcing cleanup.");
            let mut sys = System::new();
            if sys.refresh_process(Pid::from(pid)) {
                if let Some(process) = sys.process(Pid::from(pid)) {
                    process.kill();
                    log::info!("Process killed.");
                }
            }
        }
    }

    // Cleanup
    fs::remove_file(&pid_file_path)?;
    if sock_path.exists() {
        fs::remove_file(&sock_path)?;
    }

    Ok(())
}

async fn show_status(data_dir: &Path) -> Result<()> {
    let sock_path = data_dir.join("toki.sock");

    if !sock_path.exists() {
        println!("Daemon Status: Not running");
        return Ok(());
    }

    let client = IpcClient::new(&sock_path);
    match client.send_command(IpcRequest::Status).await {
        Ok(IpcResponse::Status {
            running,
            current_window,
            current_issue,
            session_duration,
        }) => {
            println!(
                "Daemon Status: {}",
                if running { "Running" } else { "Stopped" }
            );
            println!("\nCurrent Activity:");
            println!(
                "  Window: {}",
                current_window.unwrap_or_else(|| "None".to_string())
            );
            println!(
                "  Issue: {}",
                current_issue.unwrap_or_else(|| "None".to_string())
            );

            let hours = session_duration / 3600;
            let minutes = (session_duration % 3600) / 60;
            let seconds = session_duration % 60;
            println!("\nSession Duration: {hours:02}:{minutes:02}:{seconds:02}");
        }
        Ok(_) => anyhow::bail!("Unexpected response from daemon"),
        Err(e) => {
            log::error!("Failed to get status: {e}");
            println!("Daemon Status: Not running (or not responding)");
        }
    }
    Ok(())
}

fn setup_daemon_logging() -> Result<()> {
    use std::fs::{create_dir_all, OpenOptions};

    let log_path = get_data_dir()?.join("toki.log");

    if let Some(parent) = log_path.parent() {
        create_dir_all(parent)?;
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    env_logger::Builder::from_default_env()
        .target(env_logger::Target::Pipe(Box::new(log_file)))
        .filter_level(log::LevelFilter::Debug)
        .init();

    Ok(())
}

fn get_config_value(db: &Database, key: &str) -> Result<Option<String>> {
    let parts: Vec<&str> = key.split('.').collect();

    if parts.len() != 2 {
        anyhow::bail!("Invalid key format. Use: <section>.<key> (e.g., plane.api_key)");
    }

    let section = parts[0];
    let field = parts[1];

    match section {
        "plane" | "github" | "jira" | "notion" => {
            if let Some(config) = db.get_integration_config(section)? {
                let value = match field {
                    "api_url" => Some(config.api_url),
                    "api_key" => Some(config.api_key),
                    "workspace" | "workspace_slug" => config.workspace_slug.clone(),
                    "project" | "project_id" | "database_id" => config.project_id.clone(),
                    // For Notion, time_property is stored in workspace_slug field
                    "time_property" if section == "notion" => config.workspace_slug.clone(),
                    _ => None,
                };
                Ok(value)
            } else {
                Ok(None)
            }
        }
        "settings" => {
            let settings = db.get_settings()?;
            let value = match field {
                "idle_threshold" | "idle_threshold_seconds" => {
                    Some(settings.idle_threshold_seconds.to_string())
                }
                "work_item_tracking" | "enable_work_item_tracking" => {
                    Some(settings.enable_work_item_tracking.to_string())
                }
                "capture_window_title" => Some(settings.capture_window_title.to_string()),
                _ => None,
            };
            Ok(value)
        }
        _ => anyhow::bail!(
            "Unknown section: {section}. Valid sections: plane, github, jira, notion, settings"
        ),
    }
}

fn set_config_value(db: &Database, key: &str, value: &str) -> Result<()> {
    use toki_storage::IntegrationConfig;

    let parts: Vec<&str> = key.split('.').collect();

    if parts.len() != 2 {
        anyhow::bail!("Invalid key format. Use: <section>.<key> (e.g., plane.api_key)");
    }

    let section = parts[0];
    let field = parts[1];

    match section {
        "plane" | "github" | "jira" => {
            let mut config = db.get_integration_config(section)?.unwrap_or_else(|| {
                IntegrationConfig::new(section.to_string(), String::new(), String::new())
            });

            match field {
                "api_url" => config.api_url = value.to_string(),
                "api_key" => config.api_key = value.to_string(),
                "workspace" | "workspace_slug" => config.workspace_slug = Some(value.to_string()),
                "project" | "project_id" => config.project_id = Some(value.to_string()),
                _ => anyhow::bail!(
                    "Unknown field: {field}. Valid fields: api_url, api_key, workspace, project"
                ),
            }

            config.updated_at = chrono::Utc::now();
            db.upsert_integration_config(&config)?;
        }
        "notion" => {
            let mut config = db.get_integration_config(section)?.unwrap_or_else(|| {
                IntegrationConfig::new(section.to_string(), String::new(), String::new())
            });

            match field {
                "api_key" => config.api_key = value.to_string(),
                "database_id" => config.project_id = Some(value.to_string()),
                // Store time_property in workspace_slug field (reused for Notion)
                "time_property" => config.workspace_slug = Some(value.to_string()),
                _ => anyhow::bail!(
                    "Unknown field: {field}. Valid fields: api_key, database_id, time_property"
                ),
            }

            config.updated_at = chrono::Utc::now();
            db.upsert_integration_config(&config)?;
        }
        "settings" => {
            let mut settings = db.get_settings()?;

            match field {
                "idle_threshold" | "idle_threshold_seconds" => {
                    settings.idle_threshold_seconds = value
                        .parse()
                        .map_err(|_| anyhow::anyhow!("Invalid number"))?;
                }
                "work_item_tracking" | "enable_work_item_tracking" => {
                    settings.enable_work_item_tracking = value == "true" || value == "1";
                }
                "capture_window_title" => {
                    settings.capture_window_title = value == "true" || value == "1";
                }
                _ => anyhow::bail!("Unknown field: {field}"),
            }

            db.update_settings(&settings)?;
        }
        _ => anyhow::bail!("Unknown section: {section}. Valid sections: plane, github, jira, notion, settings"),
    }

    Ok(())
}

/// Escape a string for CSV format
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Handle Plane.so subcommands
async fn handle_plane_command(action: PlaneAction) -> Result<()> {
    use toki_integrations::{PlaneClient, ProjectManagementSystem};

    let db = Database::new(None)?;

    // Get Plane configuration
    let config = db
        .get_integration_config("plane")?
        .ok_or_else(|| anyhow::anyhow!("Plane.so not configured. Run: toki config set plane.api_key <key>"))?;

    let workspace_slug = config
        .workspace_slug
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Workspace not configured. Run: toki config set plane.workspace <slug>"))?;

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
                println!("{:<40} {:<12} {}", project.name, project.identifier, time_tracking);
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
                    println!("{}-{:<10} {:<50} {}", project_id, item.sequence_id, title, status);
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
                    println!("{}-{:<10} {:<50} {}", target_project.identifier, item.sequence_id, title, status);
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
                println!("{}-{:<10} {:<50} {}", project_id, item.sequence_id, title, status);
            }
        }
    }

    Ok(())
}

/// Handle the review command - show daily activity summary with AI suggestions
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::too_many_lines)]
async fn handle_review_command(date: Option<String>, verbose: bool, confirm_all: bool) -> Result<()> {
    use chrono::{NaiveDate, TimeZone, Utc};
    use toki_ai::{ActivitySegment, ActivitySignals, SmartIssueMatcher, SuggestedIssue, TimeAnalyzer};
    use toki_storage::TimeBlock;

    let db = Arc::new(Database::new(None)?);

    // Parse date or use today
    let target_date = if let Some(date_str) = date {
        NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("Invalid date format. Use YYYY-MM-DD"))?
    } else {
        Utc::now().date_naive()
    };

    // Get time range for the date
    let start = target_date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid time"))?;
    let end = target_date
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| anyhow::anyhow!("Invalid time"))?;

    let start_utc = Utc.from_utc_datetime(&start);
    let end_utc = Utc.from_utc_datetime(&end);

    // Fetch activity spans for the day
    let spans = db.get_activity_spans(start_utc, end_utc)?;

    if spans.is_empty() {
        println!("No activity recorded for {target_date}");
        return Ok(());
    }

    // Convert ActivitySpan to ActivitySegment for AI analysis
    let segments: Vec<ActivitySegment> = spans
        .iter()
        .filter_map(|span| {
            let end_time = span.end_time?;
            Some(ActivitySegment {
                start_time: span.start_time,
                end_time,
                project_name: None, // Would need to look up project
                category: span.category.clone(),
                edited_files: span
                    .context
                    .as_ref()
                    .map(|c| c.edited_files.clone())
                    .unwrap_or_default(),
                git_commits: span
                    .context
                    .as_ref()
                    .map(|c| c.git_commits.clone())
                    .unwrap_or_default(),
                git_branch: span.context.as_ref().and_then(|c| c.git_branch.clone()),
                browser_urls: span
                    .context
                    .as_ref()
                    .map(|c| c.browser_urls.clone())
                    .unwrap_or_default(),
            })
        })
        .collect();

    // Analyze with AI
    let analyzer = TimeAnalyzer::new();
    let mut summary = analyzer.generate_daily_summary(target_date, &segments);

    // Compute Gravity/Relevance for unclassified or generic activities
    // This is the "Quiet Tech" magic: infer relevance without rules
    if let Ok(gravity_calc) = toki_ai::GravityCalculator::new(db.clone()) {
        // We need to compute gravity for each segment against its likely project
        // For simplicity in this phase, we'll just check against the most active project of the day
        let top_project_id = if let Ok(projects) = db.get_project_time_for_date(
            &target_date.format("%Y-%m-%d").to_string()
        ) {
            projects.first().map(|(p, _)| p.id) // Correctly extract project ID
        } else {
            None
        };

        if let Some(pid) = top_project_id {
            // Check unclassified segments
            // Note: Ideally we would update the summary structure to include relevance info
            // For now, we will just log it or print it during the review
            // In a full implementation, this would re-classify segments in the summary
            println!("\n(AI Context Gravity initialized. Project context: {pid})");
            
            for block in &mut summary.suggested_blocks {
                // Check relevance of the block description
                if let Ok(score) = gravity_calc.calculate_gravity(&block.suggested_description, pid) {
                    let status = toki_ai::RelevanceStatus::from_score(score);
                    
                    // If score is low but it was classified as work, flag it
                    if status == toki_ai::RelevanceStatus::Break && block.confidence > 0.7 {
                        block.reasoning.push(format!("Warning: Low semantic relevance to project (Gravity: {score:.2})"));
                    } else if status == toki_ai::RelevanceStatus::Focus {
                        block.reasoning.push(format!("Confirmed high relevance (Gravity: {score:.2})"));
                        // Boost confidence
                        block.confidence = (block.confidence + 0.2).min(1.0);
                    }
                }
            }
        }
    }

    // Display header
    println!("\n{}", "=".repeat(60));
    println!("Daily Activity Review: {target_date}");
    println!("{}", "=".repeat(60));

    // Total time
    let total_hours = summary.total_active_seconds / 3600;
    let total_mins = (summary.total_active_seconds % 3600) / 60;
    println!(
        "\nTotal active time: {total_hours}h {total_mins}m"
    );

    // Classified vs unclassified
    let classified_pct = if summary.total_active_seconds > 0 {
        (summary.classified_seconds as f32 / summary.total_active_seconds as f32) * 100.0
    } else {
        0.0
    };
    println!(
        "Classified: {}m ({:.0}%), Unclassified: {}m",
        summary.classified_seconds / 60,
        classified_pct,
        summary.unclassified_seconds / 60
    );

    // Project breakdown from project_time table (accurate multi-window tracking)
    let date_str = target_date.format("%Y-%m-%d").to_string();
    let project_times = db.get_project_time_for_date(&date_str)?;

    if !project_times.is_empty() {
        let total_project_secs: u32 = project_times.iter().map(|(_, s)| *s).sum();
        let total_h = total_project_secs / 3600;
        let total_m = (total_project_secs % 3600) / 60;
        println!("\nProject breakdown (total: {total_h}h {total_m}m):");
        for (project, seconds) in &project_times {
            let hours = seconds / 3600;
            let mins = (seconds % 3600) / 60;
            let pct = if total_project_secs > 0 {
                (*seconds as f32 / total_project_secs as f32) * 100.0
            } else {
                0.0
            };
            println!("  - {}: {}h {}m ({:.0}%)", project.name, hours, mins, pct);
        }
    } else if !summary.project_breakdown.is_empty() {
        // Fallback to AI-analyzed breakdown if no project_time data
        println!("\nProject breakdown:");
        let mut projects: Vec<_> = summary.project_breakdown.iter().collect();
        projects.sort_by(|a, b| b.1.cmp(a.1));
        for (project, seconds) in projects.iter().take(5) {
            let hours = *seconds / 3600;
            let mins = (*seconds % 3600) / 60;
            println!("  - {project}: {hours}h {mins}m");
        }
    }

    // AI Suggested time blocks - enhanced with SmartIssueMatcher
    if !summary.suggested_blocks.is_empty() {
        println!("\n{}", "-".repeat(60));
        println!("AI Suggested Time Blocks:");
        println!("{}", "-".repeat(60));

        // Try to initialize SmartIssueMatcher for AI-based issue matching
        let smart_matcher = SmartIssueMatcher::new(db.clone()).ok();

        // Get the top project for smart matching
        let date_str_for_matching = target_date.format("%Y-%m-%d").to_string();
        let top_project_for_matching = db
            .get_project_time_for_date(&date_str_for_matching)
            .ok()
            .and_then(|projects| projects.first().map(|(p, _)| p.clone()));

        for (i, block) in summary.suggested_blocks.iter_mut().enumerate() {
            let start = block.start_time.format("%H:%M");
            let end = block.end_time.format("%H:%M");
            let duration_mins = block.duration_seconds / 60;
            let confidence_pct = (block.confidence * 100.0) as u32;

            println!(
                "\n{}. {} - {} ({}m) - {}% confidence",
                i + 1,
                start,
                end,
                duration_mins,
                confidence_pct
            );
            println!("   {}", block.suggested_description);

            // If no issues detected, try SmartIssueMatcher
            if block.suggested_issues.is_empty() {
                if let (Some(matcher), Some(project)) = (&smart_matcher, &top_project_for_matching) {
                    // Check if project has issue candidates
                    if let Ok(candidates) = db.get_active_issue_candidates(project.id) {
                        if !candidates.is_empty() {
                            // Collect context from segments within this block's time range
                            let block_segments: Vec<_> = segments
                                .iter()
                                .filter(|s| s.start_time >= block.start_time && s.end_time <= block.end_time)
                                .collect();

                            // Build ActivitySignals - use block description as fallback context
                            let signals = ActivitySignals {
                                git_branch: block_segments
                                    .iter()
                                    .find_map(|s| s.git_branch.clone()),
                                recent_commits: block_segments
                                    .iter()
                                    .flat_map(|s| s.git_commits.clone())
                                    .take(5)
                                    .collect(),
                                edited_files: block_segments
                                    .iter()
                                    .flat_map(|s| s.edited_files.clone())
                                    .take(10)
                                    .collect(),
                                browser_urls: block_segments
                                    .iter()
                                    .flat_map(|s| s.browser_urls.clone())
                                    .take(5)
                                    .collect(),
                                // Use block description as window title context for semantic matching
                                window_titles: vec![block.suggested_description.clone()],
                            };

                            // Find matches using AI
                            if let Ok(matches) = matcher.find_best_matches(&signals, project.id, 3) {
                                for m in matches {
                                    block.suggested_issues.push(SuggestedIssue {
                                        issue_id: m.issue_id.clone(),
                                        confidence: m.confidence,
                                        reason: SmartIssueMatcher::format_reasons(&m.match_reasons),
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Display suggested issues
            if block.suggested_issues.is_empty() {
                println!("   No issue matches (general development time)");
                if top_project_for_matching.as_ref().is_none_or(|p| p.pm_project_id.is_none()) {
                    println!("   Tip: Link project to Plane.so with 'toki project link' then 'toki issue-sync'");
                }
            } else {
                println!("   AI Suggested Issues:");
                for issue in &block.suggested_issues {
                    let conf_level = if issue.confidence >= 0.8 {
                        "[HIGH]"
                    } else if issue.confidence >= 0.5 {
                        "[MED] "
                    } else {
                        "[LOW] "
                    };
                    println!(
                        "     {} {} - {:.0}% - {}",
                        conf_level,
                        issue.issue_id,
                        issue.confidence * 100.0,
                        issue.reason
                    );
                }
            }

            if verbose {
                println!("   Reasoning:");
                for reason in &block.reasoning {
                    println!("     - {reason}");
                }
            }
        }
    }

    // Save time blocks if --confirm-all is set
    if confirm_all && !summary.suggested_blocks.is_empty() {
        println!("\n{}", "-".repeat(60));
        println!("Saving {} time blocks...", summary.suggested_blocks.len());

        let mut saved_count = 0;
        for suggested in &summary.suggested_blocks {
            // Convert suggested issues to work item UUIDs
            let work_item_ids: Vec<_> = suggested
                .suggested_issues
                .iter()
                .filter_map(|si| {
                    // Try to find the work item by external ID
                    db.get_work_item(&si.issue_id, "plane")
                        .ok()
                        .flatten()
                        .map(|wi| wi.id)
                })
                .collect();

            // Use the ai_suggested constructor, then mark as confirmed
            let mut time_block = TimeBlock::ai_suggested(
                suggested.start_time,
                suggested.end_time,
                suggested.suggested_description.clone(),
                work_item_ids,
                suggested.confidence,
            );
            time_block.confirmed = true; // Mark as confirmed since user used --confirm-all

            if let Err(e) = db.save_time_block(&time_block) {
                log::error!("Failed to save time block: {e}");
            } else {
                saved_count += 1;
            }
        }

        println!("Saved {saved_count} confirmed time blocks.");
        println!("\nTo sync to Plane.so:");
        println!("  toki sync plane --reviewed");
    } else {
        println!("\n{}", "=".repeat(60));
        println!("To confirm and save all blocks:");
        println!("  toki review --confirm-all");
        println!("\nTo sync confirmed blocks to Plane.so:");
        println!("  toki sync plane --reviewed");
        println!("{}", "=".repeat(60));
    }

    Ok(())
}

/// Handle learn command - teach toki to classify activities
fn handle_learn_command(action: LearnAction) -> Result<()> {
    use toki_storage::{ClassificationRule, PatternType};

    let db = Database::new(None)?;

    match action {
        LearnAction::Add {
            pattern,
            category,
            pattern_type,
        } => {
            let pt: PatternType = pattern_type
                .parse()
                .map_err(|e: String| anyhow::anyhow!(e))?;

            // Check if rule already exists
            if let Some(existing) = db.find_rule_by_pattern(&pattern, &pt)? {
                println!(
                    "Rule already exists: '{}' -> '{}' (hits: {})",
                    existing.pattern, existing.category, existing.hit_count
                );
                println!("Delete it first with: toki learn delete {}", existing.id);
                return Ok(());
            }

            let rule = ClassificationRule::from_correction(pattern.clone(), pt, category.clone());
            db.save_classification_rule(&rule)?;

            println!("Learned: '{pattern}' -> '{category}'");
            println!("This rule will be applied from now on.");
            println!("\nTo test, restart the daemon: toki stop && toki start");
        }

        LearnAction::List => {
            let rules = db.get_classification_rules()?;

            if rules.is_empty() {
                println!("No learned rules yet.");
                println!("\nTeach toki with:");
                println!("  toki learn add \"instagram\" Break --type domain");
                println!("  toki learn add \"Cake\" Research --type window_title");
                return Ok(());
            }

            println!("Learned classification rules:\n");
            println!(
                "{:<36} {:<20} {:<15} {:<12} HITS",
                "ID", "PATTERN", "TYPE", "CATEGORY"
            );
            println!("{}", "-".repeat(95));

            for rule in rules {
                let short_id = &rule.id.to_string()[..8];
                println!(
                    "{:<36} {:<20} {:<15} {:<12} {}",
                    short_id,
                    truncate_str(&rule.pattern, 18),
                    format!("{:?}", rule.pattern_type),
                    rule.category,
                    rule.hit_count
                );
            }

            println!("\nTo delete a rule: toki learn delete <ID or pattern>");
        }

        LearnAction::Delete { identifier } => {
            let rules = db.get_classification_rules()?;

            // Try to find by ID prefix or pattern
            let to_delete = rules.iter().find(|r| {
                r.id.to_string().starts_with(&identifier)
                    || r.pattern.to_lowercase() == identifier.to_lowercase()
            });

            if let Some(rule) = to_delete {
                db.delete_classification_rule(rule.id)?;
                println!("Deleted rule: '{}' -> '{}'", rule.pattern, rule.category);
            } else {
                println!("Rule not found: {identifier}");
                println!("Use 'toki learn list' to see all rules.");
            }
        }
    }

    Ok(())
}

// ============================================================================
// Issue Sync Command
// ============================================================================

async fn handle_issue_sync_command(force: bool) -> Result<()> {
    use toki_ai::IssueSyncService;
    use toki_integrations::{NotionClient, PlaneClient};

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
    let plane_count = linked_projects.iter().filter(|p| p.pm_system.as_deref() == Some("plane")).count();
    let notion_count = linked_projects.iter().filter(|p| p.pm_system.as_deref() == Some("notion")).count();

    println!("Syncing issues from PM systems...");
    println!("  Linked projects: {} (Plane: {}, Notion: {})", linked_projects.len(), plane_count, notion_count);

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

// ============================================================================
// Project Command
// ============================================================================

async fn handle_project_command(action: ProjectAction) -> Result<()> {
    use toki_integrations::plane::PlaneClient;

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

            println!("\nTo link a project: toki project link --project <name> --plane-project <IDENTIFIER>");
        }

        ProjectAction::Link { project, plane_project, notion_database } => {
            // Find local project by name
            let local_project = db.get_project_by_name(&project)?;
            let local_project = if let Some(p) = local_project { p } else {
                println!("Project not found: {project}");
                println!("Run 'toki project list' to see available projects.");
                return Ok(());
            };

            // Determine which PM system to link
            match (plane_project, notion_database) {
                (Some(plane_id), None) => {
                    // Link to Plane.so
                    let config = db.get_integration_config("plane")?;
                    let config = if let Some(c) = config { c } else {
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

                    let plane_proj = if let Some(p) = matching_project { p } else {
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

                    println!("Linked '{project}' -> Plane project '{}'", plane_proj.identifier);
                    println!("\nNow run 'toki issue-sync' to fetch issues for AI matching.");
                }
                (None, Some(db_id)) => {
                    // Link to Notion database
                    use toki_integrations::NotionClient;

                    let config = db.get_integration_config("notion")?;
                    let config = if let Some(c) = config { c } else {
                        println!("Notion is not configured.");
                        println!("Run 'toki config set notion.api_key <your-integration-token>' first.");
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
                        .map(|t| t.plain_text.as_str())
                        .unwrap_or("Untitled");

                    // Link the project
                    db.link_project_to_pm(
                        local_project.id,
                        "notion",
                        &db_id,
                        None,
                    )?;

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
            let local_project = if let Some(p) = local_project { p } else {
                println!("Project not found: {project}");
                return Ok(());
            };

            db.link_project_to_pm(local_project.id, "", "", None)?;
            println!("Unlinked '{project}' from PM system.");
        }

        ProjectAction::AutoLink { min_confidence, apply } => {
            use toki_ai::AutoLinker;

            // Get Plane configuration
            let config = db.get_integration_config("plane")?;
            let config = if let Some(c) = config { c } else {
                println!("Plane.so is not configured.");
                println!("Run 'toki config set plane.api_key <your-api-key>' first.");
                return Ok(());
            };

            let workspace_slug = config.workspace_slug.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Plane workspace not configured")
            })?;

            let plane_client = PlaneClient::new(
                config.api_key.clone(),
                workspace_slug.clone(),
                Some(config.api_url.clone()),
            )?;

            let db_arc = Arc::new(Database::new(None)?);
            let auto_linker = AutoLinker::new(db_arc);

            println!("Analyzing projects for auto-linking...\n");

            // Get suggestions from name matching
            let suggestions = auto_linker.suggest_from_name_matching(&plane_client).await?;

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
                let marker = if s.confidence >= min_confidence { "*" } else { " " };
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

            println!("\n* = above {:.0}% confidence threshold", min_confidence * 100.0);

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
                            println!("  Linked '{}' -> {}", s.local_project_name, s.pm_project_identifier);
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

// ============================================================================
// Notion Command
// ============================================================================

async fn handle_notion_command(action: NotionAction) -> Result<()> {
    use toki_integrations::{NotionClient, ProjectManagementSystem};

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
    }

    Ok(())
}
