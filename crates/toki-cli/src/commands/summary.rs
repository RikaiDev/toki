//! Work summary generation commands
//!
//! Generates natural language summaries of work activity.

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::Subcommand;
use toki_ai::{SummaryPeriod, WorkSummaryGenerator};
use toki_storage::Database;

#[derive(Subcommand, Debug)]
pub enum SummaryAction {
    /// Generate summary for today
    Today {
        /// Output format: text, brief, json, markdown
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Generate summary for yesterday
    Yesterday {
        /// Output format: text, brief, json, markdown
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Generate summary for this week
    Week {
        /// Output format: text, brief, json, markdown
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Generate summary for this month
    Month {
        /// Output format: text, brief, json, markdown
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Generate summary for a custom date range
    Range {
        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        from: String,
        /// End date (YYYY-MM-DD)
        #[arg(long)]
        to: String,
        /// Output format: text, brief, json, markdown
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Generate summary for a specific project
    Project {
        /// Project name or path
        name: String,
        /// Time period: today, yesterday, week, month
        #[arg(short, long, default_value = "today")]
        period: String,
        /// Output format: text, brief, json, markdown
        #[arg(short, long, default_value = "text")]
        format: String,
    },
}

/// Handle summary commands
pub fn handle_summary_command(action: SummaryAction) -> Result<()> {
    match action {
        SummaryAction::Today { format } => generate_summary(SummaryPeriod::Today, &format),
        SummaryAction::Yesterday { format } => generate_summary(SummaryPeriod::Yesterday, &format),
        SummaryAction::Week { format } => generate_summary(SummaryPeriod::Week, &format),
        SummaryAction::Month { format } => generate_summary(SummaryPeriod::Month, &format),
        SummaryAction::Range { from, to, format } => {
            let start = NaiveDate::parse_from_str(&from, "%Y-%m-%d")
                .context("Invalid start date format. Use YYYY-MM-DD")?;
            let end = NaiveDate::parse_from_str(&to, "%Y-%m-%d")
                .context("Invalid end date format. Use YYYY-MM-DD")?;
            generate_summary(SummaryPeriod::Custom { start, end }, &format)
        }
        SummaryAction::Project { name, period, format } => {
            generate_project_summary(&name, &period, &format)
        }
    }
}

/// Generate and output a summary
fn generate_summary(period: SummaryPeriod, format: &str) -> Result<()> {
    let db = Arc::new(Database::new(None).context("Failed to open database")?);
    let generator = WorkSummaryGenerator::new(db);

    let summary = generator.generate(period)?;

    output_summary(&summary, format);
    Ok(())
}

/// Generate and output a project-specific summary
fn generate_project_summary(name_or_path: &str, period: &str, format: &str) -> Result<()> {
    let db = Arc::new(Database::new(None).context("Failed to open database")?);

    // Try to find project by name or path
    let project = db
        .get_project_by_name(name_or_path)?
        .or_else(|| db.get_project_by_path(name_or_path).ok().flatten())
        .ok_or_else(|| anyhow::anyhow!("Project not found: {name_or_path}"))?;

    let period = match period.to_lowercase().as_str() {
        "today" => SummaryPeriod::Today,
        "yesterday" => SummaryPeriod::Yesterday,
        "week" => SummaryPeriod::Week,
        "month" => SummaryPeriod::Month,
        _ => {
            eprintln!("Unknown period '{period}', using 'today'");
            SummaryPeriod::Today
        }
    };

    let generator = WorkSummaryGenerator::new(Arc::new(Database::new(None)?));
    let summary = generator.generate_for_project(&project.path, period)?;

    output_summary(&summary, format);
    Ok(())
}

/// Output summary in the requested format
fn output_summary(summary: &toki_ai::WorkSummary, format: &str) {
    match format.to_lowercase().as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&summary.to_json()).unwrap());
        }
        "brief" => {
            println!("{}", summary.generate_brief());
        }
        "markdown" | "md" => {
            println!("{}", summary.generate_text());
        }
        _ => {
            // Plain text version (strip markdown formatting)
            let md = summary.generate_text();
            let plain = md
                .lines()
                .map(|line| {
                    line.trim_start_matches('#')
                        .trim_start_matches('*')
                        .trim_start()
                })
                .collect::<Vec<_>>()
                .join("\n");
            println!("{plain}");
        }
    }
}
