/// Report and categories command handlers
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use tabled::{Table, Tabled};
use toki_ai::InsightsGenerator;
use toki_storage::models::OutcomeSummary;
use toki_storage::Database;

#[derive(Tabled)]
struct CategoryStats {
    #[tabled(rename = "Category")]
    category: String,
    #[tabled(rename = "Time (minutes)")]
    time_minutes: u32,
    #[tabled(rename = "Percentage")]
    percentage: String,
}

#[derive(Tabled)]
struct SessionOutcomeRow {
    #[tabled(rename = "Session")]
    session: String,
    #[tabled(rename = "Project")]
    project: String,
    #[tabled(rename = "Duration")]
    duration: String,
    #[tabled(rename = "Outcomes")]
    outcomes: String,
}

pub fn handle_report_command(period: &str, by_outcome: bool) -> Result<()> {
    let db = Database::new(None)?;

    let (start, end) = match period {
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

    if by_outcome {
        handle_outcome_report(&db, period, start, end)
    } else {
        handle_time_report(&db, period, start, end)
    }
}

/// Generate time-based report (default)
fn handle_time_report(
    db: &Database,
    period: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<()> {
    // Use activity_spans for more accurate data
    let spans = db.get_activity_spans(start, end)?;

    if spans.is_empty() {
        println!("No activities recorded for period: {period}");
        return Ok(());
    }

    let category_time = InsightsGenerator::time_per_category_from_spans(&spans);
    let total_time: u32 = category_time.values().sum();

    println!("\nTime Tracking Report: {period}");
    println!("{}", "\u{2550}".repeat(28));

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

/// Generate outcome-based report
fn handle_outcome_report(
    db: &Database,
    period: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<()> {
    // Get all sessions in the period
    let sessions = db.get_claude_sessions(start, end)?;

    if sessions.is_empty() {
        println!("No Claude sessions found for period: {period}");
        return Ok(());
    }

    // Aggregate outcomes across all sessions
    let mut total_summary = OutcomeSummary::default();
    let mut session_rows = Vec::new();

    for session in &sessions {
        let outcomes = db.get_session_outcomes(session.id)?;
        let summary = OutcomeSummary::from_outcomes(&outcomes);

        // Add to total
        total_summary.commits += summary.commits;
        total_summary.issues_closed += summary.issues_closed;
        total_summary.prs_merged += summary.prs_merged;
        total_summary.prs_created += summary.prs_created;
        total_summary.files_changed += summary.files_changed;

        // Only include sessions with outcomes in the table
        if !summary.is_empty() {
            let project_name = session
                .project_id
                .and_then(|pid| db.get_project(pid).ok().flatten()).map_or_else(|| "-".to_string(), |p| p.name);

            session_rows.push(SessionOutcomeRow {
                session: truncate_id(&session.session_id),
                project: project_name,
                duration: format_duration(session.duration_seconds()),
                outcomes: summary.to_string(),
            });
        }
    }

    println!("\nOutcome Report: {period}");
    println!("{}", "\u{2550}".repeat(28));

    // Summary section
    println!("\nSummary:");
    println!("  Commits:        {:>4}", total_summary.commits);
    println!("  Issues Closed:  {:>4}", total_summary.issues_closed);
    println!("  PRs Created:    {:>4}", total_summary.prs_created);
    println!("  PRs Merged:     {:>4}", total_summary.prs_merged);

    if !session_rows.is_empty() {
        println!("\nSessions with Outcomes:");
        let table = Table::new(session_rows).to_string();
        println!("{table}");
    }

    println!(
        "\nTotal: {} sessions, {} outcomes",
        sessions.len(),
        total_summary.total()
    );

    Ok(())
}

/// Format duration in human-readable form
fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;

    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Truncate session ID for display
fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..12])
    } else {
        id.to_string()
    }
}

pub fn handle_categories_command() -> Result<()> {
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
