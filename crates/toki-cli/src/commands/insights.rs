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
            anyhow::bail!("Unknown period: {}. Use 'week', 'month', 'today', or YYYY-MM-DD:YYYY-MM-DD", period);
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

    metrics.project_count = projects_seen.len() as u32;

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
            let change = (current.total_seconds as f32 - prev.total_seconds as f32) / prev.total_seconds as f32;
            if change > 0.5 {
                anomalies.push(Anomaly {
                    description: "Work time increased significantly".to_string(),
                    severity: AnomalySeverity::Info,
                    value: format!("+{:.0}%", change * 100.0),
                    expected: "±20%".to_string(),
                });
            } else if change < -0.5 {
                anomalies.push(Anomaly {
                    description: "Work time decreased significantly".to_string(),
                    severity: AnomalySeverity::Warning,
                    value: format!("{:.0}%", change * 100.0),
                    expected: "±20%".to_string(),
                });
            }
        }

        // Change in session count
        if prev.session_count > 0 {
            let change = (current.session_count as f32 - prev.session_count as f32) / prev.session_count as f32;
            if change.abs() > 0.5 {
                anomalies.push(Anomaly {
                    description: "Session count changed significantly".to_string(),
                    severity: AnomalySeverity::Info,
                    value: format!("{:+.0}%", change * 100.0),
                    expected: "±50%".to_string(),
                });
            }
        }

        // Context switch increase
        if prev.context_switches > 0 {
            let change = (current.context_switches as f32 - prev.context_switches as f32) / prev.context_switches as f32;
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
            "Your most productive hour is {}:00 - consider protecting this time for deep work",
            peak
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
            _ => {}
        }
    }

    suggestions
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
    println!("{}", "═".repeat(50));
    println!("Period: {} to {}", start.format("%Y-%m-%d"), end.format("%Y-%m-%d"));
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
            println!("Unknown focus: {}. Use: hours, sessions, context-switches", f);
            return Ok(());
        }
        None => {}
    }

    // Print summary
    println!("Summary");
    println!("{}", "─".repeat(40));
    println!("Total time:      {}", format_duration(current_metrics.total_seconds));
    println!("Sessions:        {}", current_metrics.session_count);
    println!("Avg session:     {}", format_duration(current_metrics.avg_session_seconds));
    println!("Projects:        {}", current_metrics.project_count);
    println!("Tool calls:      {}", current_metrics.total_tool_calls);
    println!("Prompts:         {}", current_metrics.total_prompts);
    println!("Context switches:{}", current_metrics.context_switches);

    if compare {
        if let Some(ref prev) = previous_metrics {
            println!();
            println!("vs Previous Period");
            println!("{}", "─".repeat(40));
            print_comparison(&current_metrics, prev);
        }
    }

    // Detect anomalies
    let anomalies = detect_anomalies(&current_metrics, previous_metrics.as_ref());
    if !anomalies.is_empty() {
        println!();
        println!("Anomalies Detected");
        println!("{}", "─".repeat(40));
        for anomaly in &anomalies {
            let icon = match anomaly.severity {
                AnomalySeverity::Alert => "!!",
                AnomalySeverity::Warning => "! ",
                AnomalySeverity::Info => "i ",
            };
            println!("[{}] {}", icon, anomaly.description);
            println!("     Value: {} (expected: {})", anomaly.value, anomaly.expected);
        }
    }

    // Print patterns
    println!();
    println!("Patterns");
    println!("{}", "─".repeat(40));

    let peak_hours = find_peak_hours(&current_metrics);
    if !peak_hours.is_empty() {
        print!("Peak hours: ");
        for (i, (hour, _)) in peak_hours.iter().enumerate() {
            if i > 0 { print!(", "); }
            print!("{}:00", hour);
        }
        println!();
    }

    // Day of week pattern
    let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let mut best_day = 0;
    let mut best_day_time = 0;
    for (i, &time) in current_metrics.daily_distribution.iter().enumerate() {
        if time > best_day_time {
            best_day = i;
            best_day_time = time;
        }
    }
    if best_day_time > 0 {
        println!("Most productive day: {} ({})", days[best_day], format_duration(best_day_time));
    }

    println!("Longest session: {}", format_duration(current_metrics.longest_session));

    // Suggestions
    let suggestions = generate_suggestions(&current_metrics, &anomalies);
    if !suggestions.is_empty() {
        println!();
        println!("Suggestions");
        println!("{}", "─".repeat(40));
        for suggestion in &suggestions {
            println!("- {suggestion}");
        }
    }

    Ok(())
}

/// Print hourly analysis
fn print_hourly_analysis(metrics: &ProductivityMetrics) {
    println!("Hourly Distribution");
    println!("{}", "─".repeat(40));

    let max_time = *metrics.hourly_distribution.iter().max().unwrap_or(&1);

    for (hour, &time) in metrics.hourly_distribution.iter().enumerate() {
        if time > 0 {
            let bar_len = if max_time > 0 { (time as f32 / max_time as f32 * 20.0) as usize } else { 0 };
            let bar = "█".repeat(bar_len);
            println!("{:02}:00 {} {}", hour, bar, format_duration(time));
        }
    }
}

/// Print session analysis
fn print_session_analysis(metrics: &ProductivityMetrics) {
    println!("Session Analysis");
    println!("{}", "─".repeat(40));
    println!("Total sessions:  {}", metrics.session_count);
    println!("Average length:  {}", format_duration(metrics.avg_session_seconds));
    println!("Longest session: {}", format_duration(metrics.longest_session));
    println!();
    println!("Sessions per day:");

    let mut days: Vec<_> = metrics.sessions_per_day.iter().collect();
    days.sort_by_key(|(d, _)| *d);

    for (date, count) in days {
        println!("  {}: {} sessions", date, count);
    }
}

/// Print context switch analysis
fn print_context_switch_analysis(metrics: &ProductivityMetrics) {
    println!("Context Switch Analysis");
    println!("{}", "─".repeat(40));
    println!("Total switches: {}", metrics.context_switches);
    println!("Projects:       {}", metrics.project_count);

    if metrics.session_count > 0 {
        let switch_rate = metrics.context_switches as f32 / metrics.session_count as f32;
        println!("Switch rate:    {:.2} per session", switch_rate);
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
        ((current.total_seconds as f32 - previous.total_seconds as f32) / previous.total_seconds as f32) * 100.0
    } else {
        0.0
    };

    let session_change = if previous.session_count > 0 {
        ((current.session_count as f32 - previous.session_count as f32) / previous.session_count as f32) * 100.0
    } else {
        0.0
    };

    println!("Time:     {:+.0}% ({} vs {})",
        time_change,
        format_duration(current.total_seconds),
        format_duration(previous.total_seconds)
    );
    println!("Sessions: {:+.0}% ({} vs {})",
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
