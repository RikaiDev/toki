//! Productivity insights and anomaly detection command
//!
//! Analyzes work patterns over time to provide insights and detect anomalies.

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc};
use toki_storage::Database;

/// Productivity metrics for a time period
#[derive(Debug, Default)]
struct ProductivityMetrics {
    /// Total tracked time in seconds
    total_seconds: u32,
    /// Number of sessions
    session_count: u32,
    /// Average session length in seconds
    avg_session_seconds: u32,
    /// Total tool calls
    total_tool_calls: u32,
    /// Total prompts
    total_prompts: u32,
    /// Time per hour of day (0-23)
    hourly_distribution: [u32; 24],
    /// Time per day of week (0=Sun, 6=Sat)
    daily_distribution: [u32; 7],
    /// Number of projects worked on
    project_count: u32,
    /// Context switches (project changes within a day)
    context_switches: u32,
    /// Longest session in seconds
    longest_session: u32,
    /// Sessions per day
    sessions_per_day: HashMap<NaiveDate, u32>,
}

/// Anomaly detected in the data
#[derive(Debug)]
struct Anomaly {
    description: String,
    severity: AnomalySeverity,
    value: String,
    expected: String,
}

#[derive(Debug, Clone, Copy)]
enum AnomalySeverity {
    Info,
    Warning,
    Alert,
}

impl std::fmt::Display for AnomalySeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Alert => write!(f, "alert"),
        }
    }
}

/// Parse period string to date range
fn parse_period(period: &str) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let now = Utc::now();
    let today = now.date_naive();

    match period.to_lowercase().as_str() {
        "week" => {
            let start = today - Duration::days(7);
            Ok((
                start.and_hms_opt(0, 0, 0).unwrap().and_utc(),
                now,
            ))
        }
        "month" => {
            let start = today - Duration::days(30);
            Ok((
                start.and_hms_opt(0, 0, 0).unwrap().and_utc(),
                now,
            ))
        }
        "today" => {
            let start = today.and_hms_opt(0, 0, 0).unwrap().and_utc();
            Ok((start, now))
        }
        _ if period.contains(':') => {
            // Custom range: YYYY-MM-DD:YYYY-MM-DD
            let parts: Vec<&str> = period.split(':').collect();
            if parts.len() != 2 {
                anyhow::bail!("Invalid date range format. Use YYYY-MM-DD:YYYY-MM-DD");
            }
            let start = NaiveDate::parse_from_str(parts[0], "%Y-%m-%d")
                .context("Invalid start date")?
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc();
            let end = NaiveDate::parse_from_str(parts[1], "%Y-%m-%d")
                .context("Invalid end date")?
                .and_hms_opt(23, 59, 59)
                .unwrap()
                .and_utc();
            Ok((start, end))
        }
        _ => {
            anyhow::bail!("Unknown period: {period}. Use 'week', 'month', 'today', or YYYY-MM-DD:YYYY-MM-DD");
        }
    }
}

/// Collect productivity metrics from sessions
fn collect_metrics(db: &Database, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<ProductivityMetrics> {
    let sessions = db.get_claude_sessions(start, end)?;

    let mut metrics = ProductivityMetrics::default();
    let mut projects_seen: std::collections::HashSet<uuid::Uuid> = std::collections::HashSet::new();
    let mut last_project_per_day: HashMap<NaiveDate, Option<uuid::Uuid>> = HashMap::new();

    for session in &sessions {
        let duration = session.duration_seconds();
        metrics.total_seconds += duration;
        metrics.session_count += 1;
        metrics.total_tool_calls += session.tool_calls;
        metrics.total_prompts += session.prompt_count;

        if duration > metrics.longest_session {
            metrics.longest_session = duration;
        }

        // Track project
        if let Some(pid) = session.project_id {
            projects_seen.insert(pid);

            // Track context switches
            let day = session.started_at.date_naive();
            if let Some(last_pid) = last_project_per_day.get(&day) {
                if last_pid.is_some() && *last_pid != Some(pid) {
                    metrics.context_switches += 1;
                }
            }
            last_project_per_day.insert(day, Some(pid));
        }

        // Hourly distribution
        let hour = session.started_at.hour() as usize;
        metrics.hourly_distribution[hour] += duration;

        // Daily distribution
        let weekday = session.started_at.weekday().num_days_from_sunday() as usize;
        metrics.daily_distribution[weekday] += duration;

        // Sessions per day
        let day = session.started_at.date_naive();
        *metrics.sessions_per_day.entry(day).or_insert(0) += 1;
    }

    metrics.project_count = u32::try_from(projects_seen.len()).unwrap_or(u32::MAX);

    if metrics.session_count > 0 {
        metrics.avg_session_seconds = metrics.total_seconds / metrics.session_count;
    }

    Ok(metrics)
}

/// Detect anomalies by comparing current period with previous
fn detect_anomalies(current: &ProductivityMetrics, previous: Option<&ProductivityMetrics>) -> Vec<Anomaly> {
    let mut anomalies = Vec::new();

    // Check for unusual patterns in current period

    // Very long sessions (> 3 hours)
    if current.longest_session > 3 * 3600 {
        anomalies.push(Anomaly {
            description: "Unusually long session detected".to_string(),
            severity: AnomalySeverity::Info,
            value: format_duration(current.longest_session),
            expected: "< 3h".to_string(),
        });
    }

    // High context switches
    if current.context_switches > 10 {
        anomalies.push(Anomaly {
            description: "High number of context switches".to_string(),
            severity: AnomalySeverity::Warning,
            value: current.context_switches.to_string(),
            expected: "< 10".to_string(),
        });
    }

    // Compare with previous period if available
    if let Some(prev) = previous {
        // Significant change in total time (> 50% increase or decrease)
        if prev.total_seconds > 0 {
            let change = (f64::from(current.total_seconds) - f64::from(prev.total_seconds))
                / f64::from(prev.total_seconds);
            if change > 0.5 {
                anomalies.push(Anomaly {
                    description: "Work time increased significantly".to_string(),
                    severity: AnomalySeverity::Info,
                    value: format!("+{:.0}%", change * 100.0),
                    expected: "\u{b1}20%".to_string(),
                });
            } else if change < -0.5 {
                anomalies.push(Anomaly {
                    description: "Work time decreased significantly".to_string(),
                    severity: AnomalySeverity::Warning,
                    value: format!("{:.0}%", change * 100.0),
                    expected: "\u{b1}20%".to_string(),
                });
            }
        }

        // Change in session count
        if prev.session_count > 0 {
            let change = (f64::from(current.session_count) - f64::from(prev.session_count))
                / f64::from(prev.session_count);
            if change.abs() > 0.5 {
                anomalies.push(Anomaly {
                    description: "Session count changed significantly".to_string(),
                    severity: AnomalySeverity::Info,
                    value: format!("{:+.0}%", change * 100.0),
                    expected: "\u{b1}50%".to_string(),
                });
            }
        }

        // Context switch increase
        if prev.context_switches > 0 {
            let change = (f64::from(current.context_switches) - f64::from(prev.context_switches))
                / f64::from(prev.context_switches);
            if change > 1.0 {
                anomalies.push(Anomaly {
                    description: "Context switches increased dramatically".to_string(),
                    severity: AnomalySeverity::Alert,
                    value: format!("+{:.0}%", change * 100.0),
                    expected: "< +100%".to_string(),
                });
            }
        }
    }

    anomalies
}

/// Find peak productivity hours
fn find_peak_hours(metrics: &ProductivityMetrics) -> Vec<(usize, u32)> {
    let mut hours: Vec<(usize, u32)> = metrics.hourly_distribution
        .iter()
        .enumerate()
        .filter(|(_, &v)| v > 0)
        .map(|(h, &v)| (h, v))
        .collect();

    hours.sort_by(|a, b| b.1.cmp(&a.1));
    hours.truncate(3);
    hours
}

/// Generate suggestions based on patterns
fn generate_suggestions(metrics: &ProductivityMetrics, anomalies: &[Anomaly]) -> Vec<String> {
    let mut suggestions = Vec::new();

    // Peak hours suggestion
    let peak_hours = find_peak_hours(metrics);
    if !peak_hours.is_empty() {
        let peak = peak_hours[0].0;
        suggestions.push(format!(
            "Your most productive hour is {peak}:00 - consider protecting this time for deep work"
        ));
    }

    // Session length suggestion
    if metrics.avg_session_seconds > 0 && metrics.avg_session_seconds < 30 * 60 {
        suggestions.push("Average session is under 30 minutes - try longer focused sessions".to_string());
    }

    // Context switch suggestion
    if metrics.context_switches > 5 {
        suggestions.push("High context switching detected - try batching similar tasks together".to_string());
    }

    // Based on anomalies
    for anomaly in anomalies {
        match anomaly.severity {
            AnomalySeverity::Alert => {
                if anomaly.description.contains("context switches") {
                    suggestions.push("Investigate what caused the spike in context switches".to_string());
                }
            }
            AnomalySeverity::Warning => {
                if anomaly.description.contains("decreased") {
                    suggestions.push("Work time is down - is this intentional or a blocker?".to_string());
                }
            }
            AnomalySeverity::Info => {}
        }
    }

    suggestions
}

/// Print the summary section
fn print_summary(metrics: &ProductivityMetrics) {
    println!("Summary");
    println!("{}", "\u{2500}".repeat(40));
    println!("Total time:      {}", format_duration(metrics.total_seconds));
    println!("Sessions:        {}", metrics.session_count);
    println!("Avg session:     {}", format_duration(metrics.avg_session_seconds));
    println!("Projects:        {}", metrics.project_count);
    println!("Tool calls:      {}", metrics.total_tool_calls);
    println!("Prompts:         {}", metrics.total_prompts);
    println!("Context switches:{}", metrics.context_switches);
}

/// Print the anomalies section
fn print_anomalies(anomalies: &[Anomaly]) {
    if anomalies.is_empty() {
        return;
    }
    println!();
    println!("Anomalies Detected");
    println!("{}", "\u{2500}".repeat(40));
    for anomaly in anomalies {
        let icon = match anomaly.severity {
            AnomalySeverity::Alert => "!!",
            AnomalySeverity::Warning => "! ",
            AnomalySeverity::Info => "i ",
        };
        println!("[{}] {}", icon, anomaly.description);
        println!("     Value: {} (expected: {})", anomaly.value, anomaly.expected);
    }
}

/// Print the patterns section
fn print_patterns(metrics: &ProductivityMetrics) {
    println!();
    println!("Patterns");
    println!("{}", "\u{2500}".repeat(40));

    let peak_hours = find_peak_hours(metrics);
    if !peak_hours.is_empty() {
        print!("Peak hours: ");
        for (i, (hour, _)) in peak_hours.iter().enumerate() {
            if i > 0 {
                print!(", ");
            }
            print!("{hour}:00");
        }
        println!();
    }

    // Day of week pattern
    let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let mut best_day = 0;
    let mut best_day_time = 0;
    for (i, &time) in metrics.daily_distribution.iter().enumerate() {
        if time > best_day_time {
            best_day = i;
            best_day_time = time;
        }
    }
    if best_day_time > 0 {
        println!(
            "Most productive day: {} ({})",
            days[best_day],
            format_duration(best_day_time)
        );
    }

    println!("Longest session: {}", format_duration(metrics.longest_session));
}

/// Print the suggestions section
fn print_suggestions_section(suggestions: &[String]) {
    if suggestions.is_empty() {
        return;
    }
    println!();
    println!("Suggestions");
    println!("{}", "\u{2500}".repeat(40));
    for suggestion in suggestions {
        println!("- {suggestion}");
    }
}

/// Handle the insights command
pub fn handle_insights_command(
    period: &str,
    compare: bool,
    focus: Option<&str>,
) -> Result<()> {
    let db = Database::new(None).context("Failed to open database")?;

    // Parse current period
    let (start, end) = parse_period(period)?;
    let period_days = (end - start).num_days();

    println!("Productivity Insights");
    println!("{}", "\u{2550}".repeat(50));
    println!(
        "Period: {} to {}",
        start.format("%Y-%m-%d"),
        end.format("%Y-%m-%d")
    );
    println!();

    // Collect current metrics
    let current_metrics = collect_metrics(&db, start, end)?;

    if current_metrics.session_count == 0 {
        println!("No sessions found for this period.");
        println!("Start working with Claude Code to generate insights!");
        return Ok(());
    }

    // Collect previous period metrics if comparing
    let previous_metrics = if compare {
        let prev_start = start - Duration::days(period_days);
        let prev_end = start;
        Some(collect_metrics(&db, prev_start, prev_end)?)
    } else {
        None
    };

    // Handle focus mode
    match focus {
        Some("hours") => {
            print_hourly_analysis(&current_metrics);
            return Ok(());
        }
        Some("sessions") => {
            print_session_analysis(&current_metrics);
            return Ok(());
        }
        Some("context-switches") => {
            print_context_switch_analysis(&current_metrics);
            return Ok(());
        }
        Some(f) => {
            println!("Unknown focus: {f}. Use: hours, sessions, context-switches");
            return Ok(());
        }
        None => {}
    }

    print_summary(&current_metrics);

    if compare {
        if let Some(ref prev) = previous_metrics {
            println!();
            println!("vs Previous Period");
            println!("{}", "\u{2500}".repeat(40));
            print_comparison(&current_metrics, prev);
        }
    }

    let anomalies = detect_anomalies(&current_metrics, previous_metrics.as_ref());
    print_anomalies(&anomalies);
    print_patterns(&current_metrics);

    let suggestions = generate_suggestions(&current_metrics, &anomalies);
    print_suggestions_section(&suggestions);

    Ok(())
}

/// Print hourly analysis
fn print_hourly_analysis(metrics: &ProductivityMetrics) {
    println!("Hourly Distribution");
    println!("{}", "\u{2500}".repeat(40));

    let max_time = *metrics.hourly_distribution.iter().max().unwrap_or(&1);

    for (hour, &time) in metrics.hourly_distribution.iter().enumerate() {
        if time > 0 {
            // Use integer arithmetic: (time * 20) / max_time gives 0-20 range
            // Result is always 0-20, safe to convert to usize
            let bar_len = if max_time > 0 {
                let len_u64 = u64::from(time) * 20 / u64::from(max_time);
                usize::try_from(len_u64).unwrap_or(20)
            } else {
                0
            };
            let bar = "\u{2588}".repeat(bar_len);
            println!("{:02}:00 {} {}", hour, bar, format_duration(time));
        }
    }
}

/// Print session analysis
fn print_session_analysis(metrics: &ProductivityMetrics) {
    println!("Session Analysis");
    println!("{}", "\u{2500}".repeat(40));
    println!("Total sessions:  {}", metrics.session_count);
    println!("Average length:  {}", format_duration(metrics.avg_session_seconds));
    println!("Longest session: {}", format_duration(metrics.longest_session));
    println!();
    println!("Sessions per day:");

    let mut days: Vec<_> = metrics.sessions_per_day.iter().collect();
    days.sort_by_key(|(d, _)| *d);

    for (date, count) in days {
        println!("  {date}: {count} sessions");
    }
}

/// Print context switch analysis
fn print_context_switch_analysis(metrics: &ProductivityMetrics) {
    println!("Context Switch Analysis");
    println!("{}", "\u{2500}".repeat(40));
    println!("Total switches: {}", metrics.context_switches);
    println!("Projects:       {}", metrics.project_count);

    if metrics.session_count > 0 {
        let switch_rate = f64::from(metrics.context_switches) / f64::from(metrics.session_count);
        println!("Switch rate:    {switch_rate:.2} per session");
    }

    if metrics.context_switches > 5 {
        println!();
        println!("High context switching can reduce productivity.");
        println!("Consider batching work by project.");
    }
}

/// Print comparison between current and previous period
fn print_comparison(current: &ProductivityMetrics, previous: &ProductivityMetrics) {
    let time_change = if previous.total_seconds > 0 {
        ((f64::from(current.total_seconds) - f64::from(previous.total_seconds))
            / f64::from(previous.total_seconds))
            * 100.0
    } else {
        0.0
    };

    let session_change = if previous.session_count > 0 {
        ((f64::from(current.session_count) - f64::from(previous.session_count))
            / f64::from(previous.session_count))
            * 100.0
    } else {
        0.0
    };

    println!(
        "Time:     {:+.0}% ({} vs {})",
        time_change,
        format_duration(current.total_seconds),
        format_duration(previous.total_seconds)
    );
    println!(
        "Sessions: {:+.0}% ({} vs {})",
        session_change,
        current.session_count,
        previous.session_count
    );
}

/// Format duration in human-readable form
fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;

    if hours > 0 {
        if minutes > 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if minutes > 0 {
        format!("{minutes}m")
    } else if seconds > 0 {
        format!("{seconds}s")
    } else {
        "0m".to_string()
    }
}
