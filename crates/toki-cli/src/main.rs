mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use toki_core::config::get_data_dir;

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
        /// Show report grouped by outcomes (commits, issues, PRs) instead of time
        #[arg(long)]
        by_outcome: bool,
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
        /// PM system type (plane, notion, gitlab, github, jira)
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
        action: commands::plane::PlaneAction,
    },
    /// Notion integration commands
    Notion {
        #[command(subcommand)]
        action: commands::notion::NotionAction,
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
        action: commands::learn::LearnAction,
    },
    /// Sync issues from PM system for AI matching
    IssueSync {
        /// Force full resync (recompute all embeddings)
        #[arg(short, long)]
        force: bool,
    },
    /// Estimate issue complexity (AI-assisted)
    Estimate {
        /// Issue ID to estimate (e.g., 43, PROJ-123)
        issue: String,
        /// Set complexity manually: trivial, simple, moderate, complex, epic
        #[arg(short, long)]
        set: Option<String>,
        /// Issue tracking system (github, notion, plane, jira)
        #[arg(long, default_value = "github")]
        system: String,
    },
    /// Project management commands
    Project {
        #[command(subcommand)]
        action: commands::project::ProjectAction,
    },
    /// Suggest issues based on current git context
    SuggestIssue {
        /// Path to analyze (defaults to current directory)
        #[arg(short, long)]
        path: Option<std::path::PathBuf>,
        /// Maximum number of suggestions
        #[arg(short = 'n', long, default_value = "5")]
        max: usize,
        /// Automatically link to the best match
        #[arg(short, long)]
        apply: bool,
    },
    /// Claude Code session management (for hooks integration)
    Session {
        #[command(subcommand)]
        action: commands::session::SessionAction,
    },
    /// Generate work summary
    Summary {
        #[command(subcommand)]
        action: commands::summary::SummaryAction,
    },
    /// Generate standup report (yesterday/today/blockers)
    Standup {
        /// Output format: text, markdown, slack, discord, teams, json
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Date to generate standup for (YYYY-MM-DD format, defaults to today)
        #[arg(short, long)]
        date: Option<String>,
    },
    /// Suggest the next task to work on
    Next {
        /// Maximum time available (e.g., 30m, 2h)
        #[arg(short, long)]
        time: Option<String>,
        /// Focus level: deep, normal, low
        #[arg(short, long)]
        focus: Option<String>,
        /// Number of suggestions to show
        #[arg(short = 'n', long, default_value = "3")]
        count: usize,
    },
    /// Analyze productivity patterns and detect anomalies
    Insights {
        /// Time period: week, month, or custom range (YYYY-MM-DD:YYYY-MM-DD)
        #[arg(short, long, default_value = "week")]
        period: String,
        /// Compare with previous period
        #[arg(short, long)]
        compare: bool,
        /// Focus on specific aspect: hours, sessions, context-switches
        #[arg(long)]
        focus: Option<String>,
    },
    /// Check for updates and install new version
    Update {
        /// Only check for updates, don't install
        #[arg(short, long)]
        check: bool,
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if !matches!(cli.command, Commands::DaemonInternalStart) {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp_secs()
            .init();
    }

    let data_dir = get_data_dir()?;

    match cli.command {
        Commands::Init { encrypt } => commands::init::init_command(encrypt),
        Commands::Start => commands::daemon::start_daemon(&data_dir),
        Commands::DaemonInternalStart => commands::daemon::run_daemon_process().await,
        Commands::Stop => commands::daemon::stop_daemon(&data_dir).await,
        Commands::Status => commands::daemon::show_status(&data_dir).await,
        Commands::Report { period, by_outcome } => {
            commands::report::handle_report_command(&period, by_outcome)
        }
        Commands::Categories => commands::report::handle_categories_command(),
        Commands::Data { action } => match action {
            DataAction::Export { format, output } => {
                commands::data::handle_data_export(format, output)
            }
            DataAction::Delete { period } => commands::data::handle_data_delete(period),
        },
        Commands::Privacy { action } => {
            use commands::privacy::PrivacyActionType;
            let action_type = match action {
                Some(PrivacyAction::Pause) => Some(PrivacyActionType::Pause),
                Some(PrivacyAction::Resume) => Some(PrivacyActionType::Resume),
                Some(PrivacyAction::ListExcluded) => Some(PrivacyActionType::ListExcluded),
                Some(PrivacyAction::Exclude { app }) => Some(PrivacyActionType::Exclude { app }),
                None => None,
            };
            commands::privacy::handle_privacy_command(action_type)
        }
        Commands::Sync {
            system,
            dry_run,
            reviewed,
        } => commands::sync::handle_sync_command(system, dry_run, reviewed).await,
        Commands::Config { action } => match action {
            ConfigAction::Get { key } => commands::config::handle_config_get(&key),
            ConfigAction::Set { key, value } => commands::config::handle_config_set(&key, &value),
            ConfigAction::List => commands::config::handle_config_list(),
        },
        Commands::Plane { action } => commands::plane::handle_plane_command(action).await,
        Commands::Review { date, verbose, confirm_all } => {
            commands::review::handle_review_command(date, verbose, confirm_all).await
        }
        Commands::Learn { action } => commands::learn::handle_learn_command(action),
        Commands::IssueSync { force } => commands::issue_sync::handle_issue_sync_command(force).await,
        Commands::Estimate { issue, set, system } => {
            commands::estimate::handle_estimate_command(&issue, set.as_deref(), &system)
        }
        Commands::Project { action } => commands::project::handle_project_command(action).await,
        Commands::Notion { action } => commands::notion::handle_notion_command(action).await,
        Commands::SuggestIssue { path, max, apply } => {
            commands::suggest::run(path, max, apply)
        }
        Commands::Session { action } => commands::session::handle_session_command(action).await,
        Commands::Summary { action } => commands::summary::handle_summary_command(action),
        Commands::Standup { format, date } => {
            commands::standup::handle_standup_command(&format, date.as_deref())
        }
        Commands::Next { time, focus, count } => {
            commands::next::handle_next_command(time.as_deref(), focus.as_deref(), count)
        }
        Commands::Insights { period, compare, focus } => {
            commands::insights::handle_insights_command(&period, compare, focus.as_deref())
        }
        Commands::Update { check } => commands::update::handle_update_command(check),
    }
}
