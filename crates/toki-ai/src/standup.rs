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
