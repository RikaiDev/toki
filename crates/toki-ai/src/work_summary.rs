//! Work Summary Generator
//!
//! Generates natural language summaries of work activity from various data sources:
//! - Claude Code sessions
//! - Project time tracking
//! - Activity patterns
//! - Issue associations

use chrono::{DateTime, Duration, NaiveDate, Utc};
use std::collections::HashMap;
use std::sync::Arc;

use toki_storage::{ClaudeSession, Database, Project};

/// Summary time period
#[derive(Debug, Clone, Copy)]
pub enum SummaryPeriod {
    Today,
    Yesterday,
    Week,
    Month,
    Custom { start: NaiveDate, end: NaiveDate },
}

impl SummaryPeriod {
    /// Get the date range for this period
    #[must_use]
    pub fn date_range(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        let now = Utc::now();
        let today = now.date_naive();

        match self {
            Self::Today => {
                let start = today.and_hms_opt(0, 0, 0).unwrap();
                let end = today.and_hms_opt(23, 59, 59).unwrap();
                (
                    DateTime::from_naive_utc_and_offset(start, Utc),
                    DateTime::from_naive_utc_and_offset(end, Utc),
                )
            }
            Self::Yesterday => {
                let yesterday = today - Duration::days(1);
                let start = yesterday.and_hms_opt(0, 0, 0).unwrap();
                let end = yesterday.and_hms_opt(23, 59, 59).unwrap();
                (
                    DateTime::from_naive_utc_and_offset(start, Utc),
                    DateTime::from_naive_utc_and_offset(end, Utc),
                )
            }
            Self::Week => {
                let week_ago = today - Duration::days(7);
                let start = week_ago.and_hms_opt(0, 0, 0).unwrap();
                let end = today.and_hms_opt(23, 59, 59).unwrap();
                (
                    DateTime::from_naive_utc_and_offset(start, Utc),
                    DateTime::from_naive_utc_and_offset(end, Utc),
                )
            }
            Self::Month => {
                let month_ago = today - Duration::days(30);
                let start = month_ago.and_hms_opt(0, 0, 0).unwrap();
                let end = today.and_hms_opt(23, 59, 59).unwrap();
                (
                    DateTime::from_naive_utc_and_offset(start, Utc),
                    DateTime::from_naive_utc_and_offset(end, Utc),
                )
            }
            Self::Custom { start, end } => {
                let start_dt = start.and_hms_opt(0, 0, 0).unwrap();
                let end_dt = end.and_hms_opt(23, 59, 59).unwrap();
                (
                    DateTime::from_naive_utc_and_offset(start_dt, Utc),
                    DateTime::from_naive_utc_and_offset(end_dt, Utc),
                )
            }
        }
    }

    /// Get human-readable period name
    #[must_use]
    pub fn display_name(&self) -> String {
        match self {
            Self::Today => "Today".to_string(),
            Self::Yesterday => "Yesterday".to_string(),
            Self::Week => "This Week".to_string(),
            Self::Month => "This Month".to_string(),
            Self::Custom { start, end } => format!("{} to {}", start, end),
        }
    }
}

/// Project work summary
#[derive(Debug, Clone)]
pub struct ProjectWorkSummary {
    pub project: Project,
    pub total_seconds: u32,
    pub session_count: u32,
    pub tool_calls: u32,
    pub prompt_count: u32,
}

/// Work summary data
#[derive(Debug)]
pub struct WorkSummary {
    pub period: SummaryPeriod,
    pub total_seconds: u32,
    pub session_count: u32,
    pub total_tool_calls: u32,
    pub total_prompts: u32,
    pub projects: Vec<ProjectWorkSummary>,
    pub sessions: Vec<ClaudeSession>,
    pub insights: Vec<String>,
    pub suggestions: Vec<String>,
}

impl WorkSummary {
    /// Format duration in human-readable form
    fn format_duration(seconds: u32) -> String {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;

        if hours > 0 {
            if minutes > 0 {
                format!("{}h {}m", hours, minutes)
            } else {
                format!("{}h", hours)
            }
        } else if minutes > 0 {
            format!("{}m", minutes)
        } else {
            format!("{}s", seconds)
        }
    }

    /// Generate a natural language summary
    #[must_use]
    pub fn generate_text(&self) -> String {
        let mut output = String::new();

        // Header
        output.push_str(&format!("# Work Summary - {}\n\n", self.period.display_name()));

        // Overview
        output.push_str("## Overview\n\n");
        output.push_str(&format!(
            "- **Total Time**: {}\n",
            Self::format_duration(self.total_seconds)
        ));
        output.push_str(&format!("- **Sessions**: {}\n", self.session_count));
        output.push_str(&format!("- **Tool Calls**: {}\n", self.total_tool_calls));
        output.push_str(&format!("- **Prompts**: {}\n", self.total_prompts));
        output.push('\n');

        // Project breakdown
        if !self.projects.is_empty() {
            output.push_str("## Projects\n\n");
            for project in &self.projects {
                let percentage = if self.total_seconds > 0 {
                    (project.total_seconds as f32 / self.total_seconds as f32 * 100.0) as u32
                } else {
                    0
                };
                output.push_str(&format!(
                    "### {} ({}%)\n",
                    project.project.name, percentage
                ));
                output.push_str(&format!(
                    "- Time: {}\n",
                    Self::format_duration(project.total_seconds)
                ));
                output.push_str(&format!("- Sessions: {}\n", project.session_count));
                output.push_str(&format!("- Tool Calls: {}\n", project.tool_calls));
                output.push('\n');
            }
        }

        // Insights
        if !self.insights.is_empty() {
            output.push_str("## Insights\n\n");
            for insight in &self.insights {
                output.push_str(&format!("- {}\n", insight));
            }
            output.push('\n');
        }

        // Suggestions
        if !self.suggestions.is_empty() {
            output.push_str("## Suggestions\n\n");
            for suggestion in &self.suggestions {
                output.push_str(&format!("- {}\n", suggestion));
            }
            output.push('\n');
        }

        output
    }

    /// Generate a concise one-paragraph summary
    #[must_use]
    pub fn generate_brief(&self) -> String {
        let time_str = Self::format_duration(self.total_seconds);

        if self.session_count == 0 {
            return format!(
                "{}: No recorded Claude Code sessions.",
                self.period.display_name()
            );
        }

        let project_names: Vec<&str> = self
            .projects
            .iter()
            .take(3)
            .map(|p| p.project.name.as_str())
            .collect();

        let project_str = if project_names.is_empty() {
            "various projects".to_string()
        } else if project_names.len() == 1 {
            project_names[0].to_string()
        } else {
            let last = project_names.last().unwrap();
            let rest: Vec<&str> = project_names[..project_names.len() - 1].to_vec();
            format!("{} and {}", rest.join(", "), last)
        };

        format!(
            "{}: {} of AI-assisted development across {} session(s) on {}. {} tool calls and {} prompts processed.",
            self.period.display_name(),
            time_str,
            self.session_count,
            project_str,
            self.total_tool_calls,
            self.total_prompts
        )
    }

    /// Generate JSON output
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "period": self.period.display_name(),
            "total_seconds": self.total_seconds,
            "total_time_formatted": Self::format_duration(self.total_seconds),
            "session_count": self.session_count,
            "tool_calls": self.total_tool_calls,
            "prompts": self.total_prompts,
            "projects": self.projects.iter().map(|p| {
                serde_json::json!({
                    "name": p.project.name,
                    "path": p.project.path,
                    "seconds": p.total_seconds,
                    "time_formatted": Self::format_duration(p.total_seconds),
                    "sessions": p.session_count,
                    "tool_calls": p.tool_calls,
                    "prompts": p.prompt_count
                })
            }).collect::<Vec<_>>(),
            "insights": self.insights,
            "suggestions": self.suggestions
        })
    }
}

/// Work summary generator
pub struct WorkSummaryGenerator {
    db: Arc<Database>,
}

impl WorkSummaryGenerator {
    /// Create a new summary generator
    #[must_use]
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Generate a work summary for the given period
    ///
    /// # Errors
    ///
    /// Returns an error if database queries fail
    pub fn generate(&self, period: SummaryPeriod) -> anyhow::Result<WorkSummary> {
        let (start, end) = period.date_range();

        // Get Claude sessions for the period
        let sessions = self.db.get_claude_sessions(start, end)?;

        // Aggregate by project
        let mut project_data: HashMap<String, ProjectWorkSummary> = HashMap::new();
        let mut total_seconds = 0u32;
        let mut total_tool_calls = 0u32;
        let mut total_prompts = 0u32;

        for session in &sessions {
            let duration = session.duration_seconds();
            total_seconds += duration;
            total_tool_calls += session.tool_calls;
            total_prompts += session.prompt_count;

            if let Some(project_id) = session.project_id {
                if let Ok(Some(project)) = self.db.get_project(project_id) {
                    let entry = project_data
                        .entry(project.path.clone())
                        .or_insert_with(|| ProjectWorkSummary {
                            project: project.clone(),
                            total_seconds: 0,
                            session_count: 0,
                            tool_calls: 0,
                            prompt_count: 0,
                        });
                    entry.total_seconds += duration;
                    entry.session_count += 1;
                    entry.tool_calls += session.tool_calls;
                    entry.prompt_count += session.prompt_count;
                }
            }
        }

        // Sort projects by time spent (descending)
        let mut projects: Vec<ProjectWorkSummary> = project_data.into_values().collect();
        projects.sort_by(|a, b| b.total_seconds.cmp(&a.total_seconds));

        // Generate insights
        let insights = self.generate_insights(&sessions, &projects, total_seconds);

        // Generate suggestions
        let suggestions = self.generate_suggestions(&sessions, &projects);

        Ok(WorkSummary {
            period,
            total_seconds,
            session_count: sessions.len() as u32,
            total_tool_calls,
            total_prompts,
            projects,
            sessions,
            insights,
            suggestions,
        })
    }

    /// Generate a summary for a specific project
    ///
    /// # Errors
    ///
    /// Returns an error if database queries fail
    pub fn generate_for_project(
        &self,
        project_path: &str,
        period: SummaryPeriod,
    ) -> anyhow::Result<WorkSummary> {
        let (start, end) = period.date_range();

        // Get project
        let project = self
            .db
            .get_project_by_path(project_path)?
            .ok_or_else(|| anyhow::anyhow!("Project not found: {}", project_path))?;

        // Get all sessions and filter by project
        let all_sessions = self.db.get_claude_sessions(start, end)?;
        let sessions: Vec<ClaudeSession> = all_sessions
            .into_iter()
            .filter(|s| s.project_id == Some(project.id))
            .collect();

        let mut total_seconds = 0u32;
        let mut total_tool_calls = 0u32;
        let mut total_prompts = 0u32;

        for session in &sessions {
            total_seconds += session.duration_seconds();
            total_tool_calls += session.tool_calls;
            total_prompts += session.prompt_count;
        }

        let project_summary = ProjectWorkSummary {
            project: project.clone(),
            total_seconds,
            session_count: sessions.len() as u32,
            tool_calls: total_tool_calls,
            prompt_count: total_prompts,
        };

        let insights = self.generate_insights(&sessions, &[project_summary.clone()], total_seconds);
        let suggestions = self.generate_suggestions(&sessions, &[project_summary.clone()]);

        Ok(WorkSummary {
            period,
            total_seconds,
            session_count: sessions.len() as u32,
            total_tool_calls,
            total_prompts,
            projects: vec![project_summary],
            sessions,
            insights,
            suggestions,
        })
    }

    /// Generate insights from the data
    fn generate_insights(
        &self,
        sessions: &[ClaudeSession],
        projects: &[ProjectWorkSummary],
        total_seconds: u32,
    ) -> Vec<String> {
        let mut insights = Vec::new();

        if sessions.is_empty() {
            return insights;
        }

        // Average session duration
        let avg_duration = total_seconds / sessions.len().max(1) as u32;
        if avg_duration > 0 {
            insights.push(format!(
                "Average session duration: {}",
                WorkSummary::format_duration(avg_duration)
            ));
        }

        // Tool usage intensity
        let tools_per_hour = if total_seconds > 0 {
            (sessions.iter().map(|s| s.tool_calls).sum::<u32>() as f32 / total_seconds as f32)
                * 3600.0
        } else {
            0.0
        };
        if tools_per_hour > 0.0 {
            insights.push(format!(
                "Tool usage rate: {:.1} calls/hour",
                tools_per_hour
            ));
        }

        // Focus analysis
        if let Some(top_project) = projects.first() {
            let focus_percentage = if total_seconds > 0 {
                (top_project.total_seconds as f32 / total_seconds as f32 * 100.0) as u32
            } else {
                0
            };
            if focus_percentage > 70 {
                insights.push(format!(
                    "Highly focused: {}% of time on {}",
                    focus_percentage, top_project.project.name
                ));
            } else if projects.len() > 2 {
                insights.push(format!(
                    "Context switching: work spread across {} projects",
                    projects.len()
                ));
            }
        }

        // Session patterns
        let active_sessions = sessions.iter().filter(|s| s.is_active()).count();
        if active_sessions > 0 {
            insights.push(format!(
                "{} session(s) currently active",
                active_sessions
            ));
        }

        insights
    }

    /// Generate suggestions based on the data
    fn generate_suggestions(
        &self,
        sessions: &[ClaudeSession],
        projects: &[ProjectWorkSummary],
    ) -> Vec<String> {
        let mut suggestions = Vec::new();

        if sessions.is_empty() {
            suggestions.push("Start a Claude Code session to track AI-assisted development".to_string());
            return suggestions;
        }

        // Check for unfinished sessions
        let active_count = sessions.iter().filter(|s| s.is_active()).count();
        if active_count > 1 {
            suggestions.push(format!(
                "Consider closing {} inactive sessions to keep tracking accurate",
                active_count - 1
            ));
        }

        // Low tool usage sessions
        let low_tool_sessions = sessions
            .iter()
            .filter(|s| !s.is_active() && s.tool_calls == 0 && s.duration_seconds() > 60)
            .count();
        if low_tool_sessions > 0 {
            suggestions.push(
                "Some sessions had no tool calls - consider using more AI assistance".to_string(),
            );
        }

        // Project without recent activity
        for project in projects {
            if project.session_count == 1 && project.total_seconds < 300 {
                suggestions.push(format!(
                    "Brief work on {} - consider continuing or documenting progress",
                    project.project.name
                ));
            }
        }

        suggestions
    }
}
