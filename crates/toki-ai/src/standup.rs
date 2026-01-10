//! Standup Report Generator
//!
//! Generates standup reports from tracked activity, ready to paste into Slack/Discord/Teams.
//!
//! Standup format:
//! - **Yesterday:** Completed work summary
//! - **Today:** Current in-progress tasks
//! - **Blockers:** Detected issues or None

use chrono::{DateTime, Duration, NaiveDate, Utc};
use std::collections::HashMap;
use std::sync::Arc;

use toki_storage::{ClaudeSession, Database, Project};

/// Standup output format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandupFormat {
    /// Plain text output
    Text,
    /// Markdown format
    Markdown,
    /// Slack mrkdwn format
    Slack,
    /// Discord markdown format
    Discord,
    /// Microsoft Teams format
    Teams,
    /// JSON format
    Json,
}

impl StandupFormat {
    /// Parse format from string
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "slack" => Self::Slack,
            "discord" => Self::Discord,
            "teams" => Self::Teams,
            "markdown" | "md" => Self::Markdown,
            "json" => Self::Json,
            _ => Self::Text,
        }
    }
}

/// Project work item for standup
#[derive(Debug, Clone)]
pub struct ProjectStandupItem {
    pub project: Project,
    pub total_seconds: u32,
    pub session_count: u32,
    pub tool_calls: u32,
    pub prompt_count: u32,
    pub description: Option<String>,
}

/// Standup report data
#[derive(Debug)]
pub struct StandupReport {
    /// Work completed (yesterday or specified past date)
    pub yesterday_work: Vec<ProjectStandupItem>,
    /// Total time yesterday
    pub yesterday_total_seconds: u32,
    /// Work in progress (today)
    pub today_work: Vec<ProjectStandupItem>,
    /// Total time today
    pub today_total_seconds: u32,
    /// Detected blockers
    pub blockers: Vec<String>,
    /// Date of the standup
    pub date: NaiveDate,
}

impl StandupReport {
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

    /// Generate output in the specified format
    #[must_use]
    pub fn format(&self, format: StandupFormat) -> String {
        match format {
            StandupFormat::Text => self.format_text(),
            StandupFormat::Markdown => self.format_markdown(),
            StandupFormat::Slack => self.format_slack(),
            StandupFormat::Discord => self.format_discord(),
            StandupFormat::Teams => self.format_teams(),
            StandupFormat::Json => self.format_json(),
        }
    }

    /// Generate plain text output
    #[must_use]
    pub fn format_text(&self) -> String {
        let mut output = String::new();

        // Yesterday
        output.push_str("Yesterday: ");
        if self.yesterday_work.is_empty() {
            output.push_str("No tracked activity\n");
        } else {
            let items: Vec<String> = self
                .yesterday_work
                .iter()
                .map(|item| {
                    let time = Self::format_duration(item.total_seconds);
                    if let Some(desc) = &item.description {
                        format!("{} on {} ({})", desc, item.project.name, time)
                    } else {
                        format!("Worked on {} ({})", item.project.name, time)
                    }
                })
                .collect();
            output.push_str(&items.join(", "));
            output.push('\n');
        }

        // Today
        output.push_str("Today: ");
        if self.today_work.is_empty() {
            output.push_str("Will continue previous work\n");
        } else {
            let items: Vec<String> = self
                .today_work
                .iter()
                .map(|item| {
                    if let Some(desc) = &item.description {
                        format!("{} on {}", desc, item.project.name)
                    } else {
                        format!("Working on {}", item.project.name)
                    }
                })
                .collect();
            output.push_str(&items.join(", "));
            output.push('\n');
        }

        // Blockers
        output.push_str("Blockers: ");
        if self.blockers.is_empty() {
            output.push_str("None\n");
        } else {
            output.push_str(&self.blockers.join(", "));
            output.push('\n');
        }

        output
    }

    /// Generate markdown output
    #[must_use]
    pub fn format_markdown(&self) -> String {
        let mut output = String::new();

        // Yesterday
        output.push_str("**Yesterday:** ");
        if self.yesterday_work.is_empty() {
            output.push_str("No tracked activity\n");
        } else {
            let items: Vec<String> = self
                .yesterday_work
                .iter()
                .map(|item| {
                    let time = Self::format_duration(item.total_seconds);
                    if let Some(desc) = &item.description {
                        format!("{} on {} ({})", desc, item.project.name, time)
                    } else {
                        format!("Worked on {} ({})", item.project.name, time)
                    }
                })
                .collect();
            output.push_str(&items.join(", "));
            output.push('\n');
        }

        // Today
        output.push_str("**Today:** ");
        if self.today_work.is_empty() {
            output.push_str("Will continue previous work\n");
        } else {
            let items: Vec<String> = self
                .today_work
                .iter()
                .map(|item| {
                    if let Some(desc) = &item.description {
                        format!("{} on {}", desc, item.project.name)
                    } else {
                        format!("Working on {}", item.project.name)
                    }
                })
                .collect();
            output.push_str(&items.join(", "));
            output.push('\n');
        }

        // Blockers
        output.push_str("**Blockers:** ");
        if self.blockers.is_empty() {
            output.push_str("None\n");
        } else {
            output.push_str(&self.blockers.join(", "));
            output.push('\n');
        }

        output
    }

    /// Generate Slack mrkdwn format
    #[must_use]
    pub fn format_slack(&self) -> String {
        let mut output = String::new();

        // Yesterday
        output.push_str("*Yesterday:* ");
        if self.yesterday_work.is_empty() {
            output.push_str("No tracked activity\n");
        } else {
            let items: Vec<String> = self
                .yesterday_work
                .iter()
                .map(|item| {
                    let time = Self::format_duration(item.total_seconds);
                    if let Some(desc) = &item.description {
                        format!("{} on `{}` ({})", desc, item.project.name, time)
                    } else {
                        format!("Worked on `{}` ({})", item.project.name, time)
                    }
                })
                .collect();
            output.push_str(&items.join(", "));
            output.push('\n');
        }

        // Today
        output.push_str("*Today:* ");
        if self.today_work.is_empty() {
            output.push_str("Will continue previous work\n");
        } else {
            let items: Vec<String> = self
                .today_work
                .iter()
                .map(|item| {
                    if let Some(desc) = &item.description {
                        format!("{} on `{}`", desc, item.project.name)
                    } else {
                        format!("Working on `{}`", item.project.name)
                    }
                })
                .collect();
            output.push_str(&items.join(", "));
            output.push('\n');
        }

        // Blockers
        output.push_str("*Blockers:* ");
        if self.blockers.is_empty() {
            output.push_str("None\n");
        } else {
            output.push_str(&self.blockers.join(", "));
            output.push('\n');
        }

        output
    }

    /// Generate Discord markdown format
    #[must_use]
    pub fn format_discord(&self) -> String {
        let mut output = String::new();

        // Yesterday
        output.push_str("**Yesterday:** ");
        if self.yesterday_work.is_empty() {
            output.push_str("No tracked activity\n");
        } else {
            let items: Vec<String> = self
                .yesterday_work
                .iter()
                .map(|item| {
                    let time = Self::format_duration(item.total_seconds);
                    if let Some(desc) = &item.description {
                        format!("{} on `{}` ({})", desc, item.project.name, time)
                    } else {
                        format!("Worked on `{}` ({})", item.project.name, time)
                    }
                })
                .collect();
            output.push_str(&items.join(", "));
            output.push('\n');
        }

        // Today
        output.push_str("**Today:** ");
        if self.today_work.is_empty() {
            output.push_str("Will continue previous work\n");
        } else {
            let items: Vec<String> = self
                .today_work
                .iter()
                .map(|item| {
                    if let Some(desc) = &item.description {
                        format!("{} on `{}`", desc, item.project.name)
                    } else {
                        format!("Working on `{}`", item.project.name)
                    }
                })
                .collect();
            output.push_str(&items.join(", "));
            output.push('\n');
        }

        // Blockers
        output.push_str("**Blockers:** ");
        if self.blockers.is_empty() {
            output.push_str("None\n");
        } else {
            output.push_str(&self.blockers.join(", "));
            output.push('\n');
        }

        output
    }

    /// Generate Microsoft Teams format
    #[must_use]
    pub fn format_teams(&self) -> String {
        // Teams uses similar markdown to Discord/standard
        self.format_markdown()
    }

    /// Generate JSON output
    #[must_use]
    pub fn format_json(&self) -> String {
        let json = serde_json::json!({
            "date": self.date.to_string(),
            "yesterday": {
                "total_seconds": self.yesterday_total_seconds,
                "total_time": Self::format_duration(self.yesterday_total_seconds),
                "items": self.yesterday_work.iter().map(|item| {
                    serde_json::json!({
                        "project": item.project.name,
                        "path": item.project.path,
                        "seconds": item.total_seconds,
                        "time": Self::format_duration(item.total_seconds),
                        "sessions": item.session_count,
                        "tool_calls": item.tool_calls,
                        "prompts": item.prompt_count,
                        "description": item.description
                    })
                }).collect::<Vec<_>>()
            },
            "today": {
                "total_seconds": self.today_total_seconds,
                "total_time": Self::format_duration(self.today_total_seconds),
                "items": self.today_work.iter().map(|item| {
                    serde_json::json!({
                        "project": item.project.name,
                        "path": item.project.path,
                        "seconds": item.total_seconds,
                        "time": Self::format_duration(item.total_seconds),
                        "sessions": item.session_count,
                        "tool_calls": item.tool_calls,
                        "prompts": item.prompt_count,
                        "description": item.description
                    })
                }).collect::<Vec<_>>()
            },
            "blockers": self.blockers
        });

        serde_json::to_string_pretty(&json).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Standup report generator
pub struct StandupGenerator {
    db: Arc<Database>,
}

impl StandupGenerator {
    /// Create a new standup generator
    #[must_use]
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Generate a standup report for the given date
    ///
    /// # Errors
    ///
    /// Returns an error if database queries fail
    ///
    /// # Panics
    ///
    /// Panics if date time construction fails (should never happen with valid dates)
    pub fn generate(&self, date: Option<NaiveDate>) -> anyhow::Result<StandupReport> {
        let today = date.unwrap_or_else(|| Utc::now().date_naive());
        let yesterday = today - Duration::days(1);

        // Get yesterday's sessions
        let yesterday_start = DateTime::from_naive_utc_and_offset(
            yesterday.and_hms_opt(0, 0, 0).unwrap(),
            Utc,
        );
        let yesterday_end = DateTime::from_naive_utc_and_offset(
            yesterday.and_hms_opt(23, 59, 59).unwrap(),
            Utc,
        );
        let yesterday_sessions = self.db.get_claude_sessions(yesterday_start, yesterday_end)?;

        // Get today's sessions
        let today_start = DateTime::from_naive_utc_and_offset(
            today.and_hms_opt(0, 0, 0).unwrap(),
            Utc,
        );
        let today_end = DateTime::from_naive_utc_and_offset(
            today.and_hms_opt(23, 59, 59).unwrap(),
            Utc,
        );
        let today_sessions = self.db.get_claude_sessions(today_start, today_end)?;

        // Aggregate yesterday's work
        let (yesterday_work, yesterday_total) = self.aggregate_sessions(&yesterday_sessions);

        // Aggregate today's work
        let (today_work, today_total) = self.aggregate_sessions(&today_sessions);

        // Detect blockers
        let blockers = self.detect_blockers(&yesterday_sessions, &today_sessions);

        Ok(StandupReport {
            yesterday_work,
            yesterday_total_seconds: yesterday_total,
            today_work,
            today_total_seconds: today_total,
            blockers,
            date: today,
        })
    }

    /// Aggregate sessions by project
    fn aggregate_sessions(
        &self,
        sessions: &[ClaudeSession],
    ) -> (Vec<ProjectStandupItem>, u32) {
        let mut project_data: HashMap<String, ProjectStandupItem> = HashMap::new();
        let mut total_seconds = 0u32;

        for session in sessions {
            let duration = session.duration_seconds();
            total_seconds += duration;

            if let Some(project_id) = session.project_id {
                if let Ok(Some(project)) = self.db.get_project(project_id) {
                    let entry = project_data
                        .entry(project.path.clone())
                        .or_insert_with(|| ProjectStandupItem {
                            project: project.clone(),
                            total_seconds: 0,
                            session_count: 0,
                            tool_calls: 0,
                            prompt_count: 0,
                            description: None,
                        });
                    entry.total_seconds += duration;
                    entry.session_count += 1;
                    entry.tool_calls += session.tool_calls;
                    entry.prompt_count += session.prompt_count;
                }
            }
        }

        // Sort by time spent (descending)
        let mut items: Vec<ProjectStandupItem> = project_data.into_values().collect();
        items.sort_by(|a, b| b.total_seconds.cmp(&a.total_seconds));

        (items, total_seconds)
    }

    /// Detect potential blockers from session patterns
    fn detect_blockers(
        &self,
        yesterday_sessions: &[ClaudeSession],
        today_sessions: &[ClaudeSession],
    ) -> Vec<String> {
        let mut blockers = Vec::new();

        // Check for long debugging sessions (high prompt count, low tool calls)
        for session in yesterday_sessions.iter().chain(today_sessions.iter()) {
            if session.prompt_count > 20 && session.tool_calls < 5 {
                if let Some(project_id) = session.project_id {
                    if let Ok(Some(project)) = self.db.get_project(project_id) {
                        blockers.push(format!(
                            "Extended debugging session on {} (possible blocker)",
                            project.name
                        ));
                    }
                }
            }
        }

        // Check for frequent context switches (many short sessions)
        let short_sessions: Vec<_> = yesterday_sessions
            .iter()
            .filter(|s| s.duration_seconds() < 300 && s.duration_seconds() > 0)
            .collect();

        if short_sessions.len() > 5 {
            blockers.push(format!(
                "Frequent context switching detected ({} short sessions)",
                short_sessions.len()
            ));
        }

        // Check for abandoned sessions
        let abandoned = yesterday_sessions
            .iter()
            .filter(|s| {
                s.end_reason.as_deref() == Some("abandoned")
                    || (s.is_active() && s.duration_seconds() > 7200)
            })
            .count();

        if abandoned > 0 {
            blockers.push(format!(
                "{abandoned} session(s) may have been interrupted"
            ));
        }

        // Deduplicate blockers
        blockers.sort();
        blockers.dedup();

        blockers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use uuid::Uuid;

    // ==================== StandupFormat tests ====================

    #[test]
    fn test_standup_format_parse_slack() {
        assert_eq!(StandupFormat::parse("slack"), StandupFormat::Slack);
        assert_eq!(StandupFormat::parse("SLACK"), StandupFormat::Slack);
        assert_eq!(StandupFormat::parse("Slack"), StandupFormat::Slack);
    }

    #[test]
    fn test_standup_format_parse_discord() {
        assert_eq!(StandupFormat::parse("discord"), StandupFormat::Discord);
        assert_eq!(StandupFormat::parse("DISCORD"), StandupFormat::Discord);
    }

    #[test]
    fn test_standup_format_parse_teams() {
        assert_eq!(StandupFormat::parse("teams"), StandupFormat::Teams);
        assert_eq!(StandupFormat::parse("TEAMS"), StandupFormat::Teams);
    }

    #[test]
    fn test_standup_format_parse_markdown() {
        assert_eq!(StandupFormat::parse("markdown"), StandupFormat::Markdown);
        assert_eq!(StandupFormat::parse("md"), StandupFormat::Markdown);
        assert_eq!(StandupFormat::parse("MD"), StandupFormat::Markdown);
    }

    #[test]
    fn test_standup_format_parse_json() {
        assert_eq!(StandupFormat::parse("json"), StandupFormat::Json);
        assert_eq!(StandupFormat::parse("JSON"), StandupFormat::Json);
    }

    #[test]
    fn test_standup_format_parse_text_default() {
        assert_eq!(StandupFormat::parse("text"), StandupFormat::Text);
        assert_eq!(StandupFormat::parse("unknown"), StandupFormat::Text);
        assert_eq!(StandupFormat::parse(""), StandupFormat::Text);
        assert_eq!(StandupFormat::parse("random"), StandupFormat::Text);
    }

    #[test]
    fn test_standup_format_equality() {
        assert_eq!(StandupFormat::Slack, StandupFormat::Slack);
        assert_ne!(StandupFormat::Slack, StandupFormat::Discord);
        assert_ne!(StandupFormat::Text, StandupFormat::Markdown);
    }

    // ==================== StandupReport::format_duration tests ====================

    #[test]
    fn test_format_duration_zero() {
        assert_eq!(StandupReport::format_duration(0), "0m");
    }

    #[test]
    fn test_format_duration_seconds_only() {
        assert_eq!(StandupReport::format_duration(30), "30s");
        assert_eq!(StandupReport::format_duration(59), "59s");
    }

    #[test]
    fn test_format_duration_minutes_only() {
        assert_eq!(StandupReport::format_duration(60), "1m");
        assert_eq!(StandupReport::format_duration(120), "2m");
        assert_eq!(StandupReport::format_duration(45 * 60), "45m");
    }

    #[test]
    fn test_format_duration_hours_only() {
        assert_eq!(StandupReport::format_duration(3600), "1h");
        assert_eq!(StandupReport::format_duration(2 * 3600), "2h");
    }

    #[test]
    fn test_format_duration_hours_and_minutes() {
        assert_eq!(StandupReport::format_duration(3600 + 30 * 60), "1h 30m");
        assert_eq!(StandupReport::format_duration(2 * 3600 + 15 * 60), "2h 15m");
    }

    // ==================== Helper functions for tests ====================

    fn create_test_project(name: &str) -> Project {
        Project {
            id: Uuid::new_v4(),
            name: name.to_string(),
            path: format!("/home/user/{name}"),
            description: None,
            created_at: Utc::now(),
            last_active: Utc::now(),
            pm_system: None,
            pm_project_id: None,
            pm_workspace: None,
        }
    }

    fn create_test_standup_item(name: &str, seconds: u32, desc: Option<&str>) -> ProjectStandupItem {
        ProjectStandupItem {
            project: create_test_project(name),
            total_seconds: seconds,
            session_count: 1,
            tool_calls: 10,
            prompt_count: 5,
            description: desc.map(String::from),
        }
    }

    fn create_empty_report() -> StandupReport {
        StandupReport {
            yesterday_work: vec![],
            yesterday_total_seconds: 0,
            today_work: vec![],
            today_total_seconds: 0,
            blockers: vec![],
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        }
    }

    fn create_sample_report() -> StandupReport {
        StandupReport {
            yesterday_work: vec![
                create_test_standup_item("toki", 7200, Some("Implemented time tracking")),
                create_test_standup_item("api-server", 3600, None),
            ],
            yesterday_total_seconds: 10800,
            today_work: vec![
                create_test_standup_item("toki", 1800, Some("Adding tests")),
            ],
            today_total_seconds: 1800,
            blockers: vec!["API rate limit issue".to_string()],
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        }
    }

    // ==================== format_text tests ====================

    #[test]
    fn test_format_text_empty_report() {
        let report = create_empty_report();
        let output = report.format_text();

        assert!(output.contains("Yesterday: No tracked activity"));
        assert!(output.contains("Today: Will continue previous work"));
        assert!(output.contains("Blockers: None"));
    }

    #[test]
    fn test_format_text_with_work() {
        let report = create_sample_report();
        let output = report.format_text();

        assert!(output.contains("Yesterday:"));
        assert!(output.contains("Implemented time tracking on toki (2h)"));
        assert!(output.contains("Worked on api-server (1h)"));
        assert!(output.contains("Today:"));
        assert!(output.contains("Adding tests on toki"));
        assert!(output.contains("Blockers: API rate limit issue"));
    }

    #[test]
    fn test_format_text_no_description() {
        let report = StandupReport {
            yesterday_work: vec![create_test_standup_item("myproject", 3600, None)],
            yesterday_total_seconds: 3600,
            today_work: vec![],
            today_total_seconds: 0,
            blockers: vec![],
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        };
        let output = report.format_text();

        assert!(output.contains("Worked on myproject (1h)"));
    }

    // ==================== format_markdown tests ====================

    #[test]
    fn test_format_markdown_empty_report() {
        let report = create_empty_report();
        let output = report.format_markdown();

        assert!(output.contains("**Yesterday:** No tracked activity"));
        assert!(output.contains("**Today:** Will continue previous work"));
        assert!(output.contains("**Blockers:** None"));
    }

    #[test]
    fn test_format_markdown_with_work() {
        let report = create_sample_report();
        let output = report.format_markdown();

        assert!(output.contains("**Yesterday:**"));
        assert!(output.contains("Implemented time tracking on toki (2h)"));
        assert!(output.contains("**Today:**"));
        assert!(output.contains("**Blockers:** API rate limit issue"));
    }

    // ==================== format_slack tests ====================

    #[test]
    fn test_format_slack_empty_report() {
        let report = create_empty_report();
        let output = report.format_slack();

        assert!(output.contains("*Yesterday:* No tracked activity"));
        assert!(output.contains("*Today:* Will continue previous work"));
        assert!(output.contains("*Blockers:* None"));
    }

    #[test]
    fn test_format_slack_with_work() {
        let report = create_sample_report();
        let output = report.format_slack();

        assert!(output.contains("*Yesterday:*"));
        // Slack uses backticks for project names
        assert!(output.contains("`toki`"));
        assert!(output.contains("`api-server`"));
        assert!(output.contains("*Today:*"));
        assert!(output.contains("*Blockers:* API rate limit issue"));
    }

    // ==================== format_discord tests ====================

    #[test]
    fn test_format_discord_empty_report() {
        let report = create_empty_report();
        let output = report.format_discord();

        assert!(output.contains("**Yesterday:** No tracked activity"));
        assert!(output.contains("**Today:** Will continue previous work"));
        assert!(output.contains("**Blockers:** None"));
    }

    #[test]
    fn test_format_discord_with_work() {
        let report = create_sample_report();
        let output = report.format_discord();

        assert!(output.contains("**Yesterday:**"));
        // Discord uses backticks for project names
        assert!(output.contains("`toki`"));
        assert!(output.contains("**Blockers:** API rate limit issue"));
    }

    // ==================== format_teams tests ====================

    #[test]
    fn test_format_teams_is_markdown() {
        let report = create_sample_report();
        let teams_output = report.format_teams();
        let markdown_output = report.format_markdown();

        // Teams format should be identical to markdown
        assert_eq!(teams_output, markdown_output);
    }

    // ==================== format_json tests ====================

    #[test]
    fn test_format_json_empty_report() {
        let report = create_empty_report();
        let output = report.format_json();

        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.is_object());
        assert_eq!(parsed["date"], "2024-01-15");
        assert!(parsed["yesterday"]["items"].as_array().unwrap().is_empty());
        assert!(parsed["today"]["items"].as_array().unwrap().is_empty());
        assert!(parsed["blockers"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_format_json_with_work() {
        let report = create_sample_report();
        let output = report.format_json();

        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["yesterday"]["total_seconds"], 10800);
        assert_eq!(parsed["yesterday"]["total_time"], "3h");
        assert_eq!(parsed["yesterday"]["items"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["yesterday"]["items"][0]["project"], "toki");
        assert_eq!(parsed["yesterday"]["items"][0]["seconds"], 7200);
        assert_eq!(parsed["today"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["blockers"][0], "API rate limit issue");
    }

    #[test]
    fn test_format_json_item_fields() {
        let report = StandupReport {
            yesterday_work: vec![ProjectStandupItem {
                project: create_test_project("test-project"),
                total_seconds: 5400,
                session_count: 3,
                tool_calls: 25,
                prompt_count: 15,
                description: Some("Bug fixes".to_string()),
            }],
            yesterday_total_seconds: 5400,
            today_work: vec![],
            today_total_seconds: 0,
            blockers: vec![],
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        };
        let output = report.format_json();

        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let item = &parsed["yesterday"]["items"][0];
        assert_eq!(item["project"], "test-project");
        assert_eq!(item["path"], "/home/user/test-project");
        assert_eq!(item["seconds"], 5400);
        assert_eq!(item["time"], "1h 30m");
        assert_eq!(item["sessions"], 3);
        assert_eq!(item["tool_calls"], 25);
        assert_eq!(item["prompts"], 15);
        assert_eq!(item["description"], "Bug fixes");
    }

    // ==================== format dispatch tests ====================

    #[test]
    fn test_format_dispatch_text() {
        let report = create_empty_report();
        assert_eq!(report.format(StandupFormat::Text), report.format_text());
    }

    #[test]
    fn test_format_dispatch_markdown() {
        let report = create_empty_report();
        assert_eq!(report.format(StandupFormat::Markdown), report.format_markdown());
    }

    #[test]
    fn test_format_dispatch_slack() {
        let report = create_empty_report();
        assert_eq!(report.format(StandupFormat::Slack), report.format_slack());
    }

    #[test]
    fn test_format_dispatch_discord() {
        let report = create_empty_report();
        assert_eq!(report.format(StandupFormat::Discord), report.format_discord());
    }

    #[test]
    fn test_format_dispatch_teams() {
        let report = create_empty_report();
        assert_eq!(report.format(StandupFormat::Teams), report.format_teams());
    }

    #[test]
    fn test_format_dispatch_json() {
        let report = create_empty_report();
        assert_eq!(report.format(StandupFormat::Json), report.format_json());
    }

    // ==================== ProjectStandupItem tests ====================

    #[test]
    fn test_project_standup_item_creation() {
        let item = ProjectStandupItem {
            project: create_test_project("myproject"),
            total_seconds: 7200,
            session_count: 3,
            tool_calls: 50,
            prompt_count: 20,
            description: Some("Feature implementation".to_string()),
        };

        assert_eq!(item.project.name, "myproject");
        assert_eq!(item.total_seconds, 7200);
        assert_eq!(item.session_count, 3);
        assert_eq!(item.tool_calls, 50);
        assert_eq!(item.prompt_count, 20);
        assert_eq!(item.description, Some("Feature implementation".to_string()));
    }

    #[test]
    fn test_project_standup_item_clone() {
        let item = create_test_standup_item("project", 3600, Some("Test"));
        let cloned = item.clone();

        assert_eq!(item.project.name, cloned.project.name);
        assert_eq!(item.total_seconds, cloned.total_seconds);
        assert_eq!(item.description, cloned.description);
    }

    // ==================== Multiple blockers tests ====================

    #[test]
    fn test_format_multiple_blockers() {
        let report = StandupReport {
            yesterday_work: vec![],
            yesterday_total_seconds: 0,
            today_work: vec![],
            today_total_seconds: 0,
            blockers: vec![
                "API issue".to_string(),
                "Build failing".to_string(),
                "Waiting for review".to_string(),
            ],
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        };
        let output = report.format_text();

        assert!(output.contains("Blockers: API issue, Build failing, Waiting for review"));
    }

    // ==================== Multiple projects tests ====================

    #[test]
    fn test_format_multiple_projects_yesterday() {
        let report = StandupReport {
            yesterday_work: vec![
                create_test_standup_item("project-a", 3600, None),
                create_test_standup_item("project-b", 1800, None),
                create_test_standup_item("project-c", 900, None),
            ],
            yesterday_total_seconds: 6300,
            today_work: vec![],
            today_total_seconds: 0,
            blockers: vec![],
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        };
        let output = report.format_text();

        assert!(output.contains("Worked on project-a (1h)"));
        assert!(output.contains("Worked on project-b (30m)"));
        assert!(output.contains("Worked on project-c (15m)"));
    }
}
