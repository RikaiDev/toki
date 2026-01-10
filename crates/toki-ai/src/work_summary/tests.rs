use super::*;
use chrono::{NaiveDate, Timelike};
use uuid::Uuid;

// ==================== Helper functions ====================

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

fn create_test_session(
    project_id: Option<Uuid>,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    tool_calls: u32,
    prompt_count: u32,
) -> ClaudeSession {
    ClaudeSession {
        id: Uuid::new_v4(),
        session_id: format!("session-{}", Uuid::new_v4()),
        project_id,
        started_at,
        ended_at,
        end_reason: ended_at.map(|_| "clear".to_string()),
        tool_calls,
        prompt_count,
        created_at: Utc::now(),
    }
}

fn create_test_project_summary(name: &str, seconds: u32, sessions: u32, tools: u32, prompts: u32) -> ProjectWorkSummary {
    ProjectWorkSummary {
        project: create_test_project(name),
        total_seconds: seconds,
        session_count: sessions,
        tool_calls: tools,
        prompt_count: prompts,
    }
}

fn create_empty_work_summary() -> WorkSummary {
    WorkSummary {
        period: SummaryPeriod::Today,
        total_seconds: 0,
        session_count: 0,
        total_tool_calls: 0,
        total_prompts: 0,
        projects: vec![],
        sessions: vec![],
        insights: vec![],
        suggestions: vec![],
    }
}

fn create_sample_work_summary() -> WorkSummary {
    WorkSummary {
        period: SummaryPeriod::Today,
        total_seconds: 7200, // 2 hours
        session_count: 3,
        total_tool_calls: 150,
        total_prompts: 50,
        projects: vec![
            create_test_project_summary("project-a", 5400, 2, 100, 35),
            create_test_project_summary("project-b", 1800, 1, 50, 15),
        ],
        sessions: vec![],
        insights: vec!["Test insight".to_string()],
        suggestions: vec!["Test suggestion".to_string()],
    }
}

// ==================== SummaryPeriod::display_name tests ====================

#[test]
fn test_summary_period_display_name_today() {
    assert_eq!(SummaryPeriod::Today.display_name(), "Today");
}

#[test]
fn test_summary_period_display_name_yesterday() {
    assert_eq!(SummaryPeriod::Yesterday.display_name(), "Yesterday");
}

#[test]
fn test_summary_period_display_name_week() {
    assert_eq!(SummaryPeriod::Week.display_name(), "This Week");
}

#[test]
fn test_summary_period_display_name_month() {
    assert_eq!(SummaryPeriod::Month.display_name(), "This Month");
}

#[test]
fn test_summary_period_display_name_custom() {
    let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let period = SummaryPeriod::Custom { start, end };
    assert_eq!(period.display_name(), "2024-01-01 to 2024-01-15");
}

// ==================== SummaryPeriod::date_range tests ====================

#[test]
fn test_summary_period_date_range_today() {
    let (start, end) = SummaryPeriod::Today.date_range();

    // Start should be at 00:00:00
    assert_eq!(start.hour(), 0);
    assert_eq!(start.minute(), 0);
    assert_eq!(start.second(), 0);

    // End should be at 23:59:59
    assert_eq!(end.hour(), 23);
    assert_eq!(end.minute(), 59);
    assert_eq!(end.second(), 59);

    // Both should be same day
    assert_eq!(start.date_naive(), end.date_naive());
}

#[test]
fn test_summary_period_date_range_yesterday() {
    let (start, end) = SummaryPeriod::Yesterday.date_range();
    let today = Utc::now().date_naive();
    let yesterday = today - Duration::days(1);

    assert_eq!(start.date_naive(), yesterday);
    assert_eq!(end.date_naive(), yesterday);
}

#[test]
fn test_summary_period_date_range_week() {
    let (start, end) = SummaryPeriod::Week.date_range();
    let today = Utc::now().date_naive();
    let week_ago = today - Duration::days(7);

    assert_eq!(start.date_naive(), week_ago);
    assert_eq!(end.date_naive(), today);
}

#[test]
fn test_summary_period_date_range_month() {
    let (start, end) = SummaryPeriod::Month.date_range();
    let today = Utc::now().date_naive();
    let month_ago = today - Duration::days(30);

    assert_eq!(start.date_naive(), month_ago);
    assert_eq!(end.date_naive(), today);
}

#[test]
fn test_summary_period_date_range_custom() {
    let start_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    let end_date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let period = SummaryPeriod::Custom { start: start_date, end: end_date };

    let (start, end) = period.date_range();

    assert_eq!(start.date_naive(), start_date);
    assert_eq!(end.date_naive(), end_date);
}

// ==================== WorkSummary::format_duration tests ====================

#[test]
fn test_format_duration_seconds_only() {
    assert_eq!(WorkSummary::format_duration(30), "30s");
    assert_eq!(WorkSummary::format_duration(59), "59s");
}

#[test]
fn test_format_duration_zero() {
    assert_eq!(WorkSummary::format_duration(0), "0s");
}

#[test]
fn test_format_duration_minutes_only() {
    assert_eq!(WorkSummary::format_duration(60), "1m");
    assert_eq!(WorkSummary::format_duration(120), "2m");
    assert_eq!(WorkSummary::format_duration(45 * 60), "45m");
}

#[test]
fn test_format_duration_hours_only() {
    assert_eq!(WorkSummary::format_duration(3600), "1h");
    assert_eq!(WorkSummary::format_duration(2 * 3600), "2h");
}

#[test]
fn test_format_duration_hours_and_minutes() {
    assert_eq!(WorkSummary::format_duration(3600 + 30 * 60), "1h 30m");
    assert_eq!(WorkSummary::format_duration(2 * 3600 + 15 * 60), "2h 15m");
}

#[test]
fn test_format_duration_ignores_leftover_seconds() {
    // Hours and minutes present, seconds ignored
    assert_eq!(WorkSummary::format_duration(3600 + 30 * 60 + 45), "1h 30m");
    // Minutes present, seconds ignored
    assert_eq!(WorkSummary::format_duration(5 * 60 + 30), "5m");
}

// ==================== WorkSummary::generate_text tests ====================

#[test]
fn test_generate_text_empty_summary() {
    let summary = create_empty_work_summary();
    let output = summary.generate_text();

    assert!(output.contains("# Work Summary - Today"));
    assert!(output.contains("## Overview"));
    assert!(output.contains("**Total Time**: 0s"));
    assert!(output.contains("**Sessions**: 0"));
    assert!(output.contains("**Tool Calls**: 0"));
    assert!(output.contains("**Prompts**: 0"));
}

#[test]
fn test_generate_text_with_projects() {
    let summary = create_sample_work_summary();
    let output = summary.generate_text();

    assert!(output.contains("## Projects"));
    assert!(output.contains("### project-a"));
    assert!(output.contains("### project-b"));
    assert!(output.contains("Time: 1h 30m")); // 5400 seconds
    assert!(output.contains("Sessions: 2"));
    assert!(output.contains("Tool Calls: 100"));
}

#[test]
fn test_generate_text_with_insights() {
    let summary = create_sample_work_summary();
    let output = summary.generate_text();

    assert!(output.contains("## Insights"));
    assert!(output.contains("- Test insight"));
}

#[test]
fn test_generate_text_with_suggestions() {
    let summary = create_sample_work_summary();
    let output = summary.generate_text();

    assert!(output.contains("## Suggestions"));
    assert!(output.contains("- Test suggestion"));
}

#[test]
fn test_generate_text_percentage_calculation() {
    let summary = create_sample_work_summary();
    let output = summary.generate_text();

    // project-a is 5400/7200 = 75%
    assert!(output.contains("project-a (75%)"));
    // project-b is 1800/7200 = 25%
    assert!(output.contains("project-b (25%)"));
}

// ==================== WorkSummary::generate_brief tests ====================

#[test]
fn test_generate_brief_no_sessions() {
    let summary = create_empty_work_summary();
    let output = summary.generate_brief();

    assert_eq!(output, "Today: No recorded Claude Code sessions.");
}

#[test]
fn test_generate_brief_with_single_project() {
    let summary = WorkSummary {
        period: SummaryPeriod::Today,
        total_seconds: 3600,
        session_count: 2,
        total_tool_calls: 50,
        total_prompts: 20,
        projects: vec![create_test_project_summary("myproject", 3600, 2, 50, 20)],
        sessions: vec![],
        insights: vec![],
        suggestions: vec![],
    };
    let output = summary.generate_brief();

    assert!(output.contains("Today:"));
    assert!(output.contains("1h of AI-assisted development"));
    assert!(output.contains("2 session(s)"));
    assert!(output.contains("on myproject"));
    assert!(output.contains("50 tool calls"));
    assert!(output.contains("20 prompts"));
}

#[test]
fn test_generate_brief_with_multiple_projects() {
    let summary = WorkSummary {
        period: SummaryPeriod::Week,
        total_seconds: 7200,
        session_count: 3,
        total_tool_calls: 100,
        total_prompts: 40,
        projects: vec![
            create_test_project_summary("project-a", 4000, 2, 60, 25),
            create_test_project_summary("project-b", 3200, 1, 40, 15),
        ],
        sessions: vec![],
        insights: vec![],
        suggestions: vec![],
    };
    let output = summary.generate_brief();

    assert!(output.contains("This Week:"));
    assert!(output.contains("on project-a and project-b"));
}

#[test]
fn test_generate_brief_with_three_projects() {
    let summary = WorkSummary {
        period: SummaryPeriod::Month,
        total_seconds: 10800,
        session_count: 5,
        total_tool_calls: 200,
        total_prompts: 80,
        projects: vec![
            create_test_project_summary("alpha", 5000, 3, 100, 40),
            create_test_project_summary("beta", 3000, 1, 60, 25),
            create_test_project_summary("gamma", 2800, 1, 40, 15),
        ],
        sessions: vec![],
        insights: vec![],
        suggestions: vec![],
    };
    let output = summary.generate_brief();

    assert!(output.contains("This Month:"));
    assert!(output.contains("on alpha, beta and gamma"));
}

#[test]
fn test_generate_brief_no_projects() {
    let summary = WorkSummary {
        period: SummaryPeriod::Today,
        total_seconds: 3600,
        session_count: 1,
        total_tool_calls: 20,
        total_prompts: 10,
        projects: vec![],
        sessions: vec![],
        insights: vec![],
        suggestions: vec![],
    };
    let output = summary.generate_brief();

    assert!(output.contains("on various projects"));
}

// ==================== WorkSummary::to_json tests ====================

#[test]
fn test_to_json_empty_summary() {
    let summary = create_empty_work_summary();
    let json = summary.to_json();

    assert_eq!(json["period"], "Today");
    assert_eq!(json["total_seconds"], 0);
    assert_eq!(json["total_time_formatted"], "0s");
    assert_eq!(json["session_count"], 0);
    assert_eq!(json["tool_calls"], 0);
    assert_eq!(json["prompts"], 0);
    assert!(json["projects"].as_array().unwrap().is_empty());
    assert!(json["insights"].as_array().unwrap().is_empty());
    assert!(json["suggestions"].as_array().unwrap().is_empty());
}

#[test]
fn test_to_json_with_data() {
    let summary = create_sample_work_summary();
    let json = summary.to_json();

    assert_eq!(json["period"], "Today");
    assert_eq!(json["total_seconds"], 7200);
    assert_eq!(json["total_time_formatted"], "2h");
    assert_eq!(json["session_count"], 3);
    assert_eq!(json["tool_calls"], 150);
    assert_eq!(json["prompts"], 50);

    let projects = json["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0]["name"], "project-a");
    assert_eq!(projects[0]["seconds"], 5400);
    assert_eq!(projects[0]["time_formatted"], "1h 30m");
    assert_eq!(projects[0]["sessions"], 2);
    assert_eq!(projects[0]["tool_calls"], 100);
    assert_eq!(projects[0]["prompts"], 35);
}

#[test]
fn test_to_json_serializable() {
    let summary = create_sample_work_summary();
    let json = summary.to_json();

    // Should be able to serialize to string
    let json_str = serde_json::to_string_pretty(&json);
    assert!(json_str.is_ok());

    // Should contain expected fields
    let str = json_str.unwrap();
    assert!(str.contains("\"period\""));
    assert!(str.contains("\"total_seconds\""));
    assert!(str.contains("\"projects\""));
}

// ==================== WorkSummaryGenerator::generate_insights tests ====================

#[test]
fn test_generate_insights_empty_sessions() {
    let sessions: Vec<ClaudeSession> = vec![];
    let projects: Vec<ProjectWorkSummary> = vec![];

    let insights = WorkSummaryGenerator::generate_insights(&sessions, &projects, 0);

    assert!(insights.is_empty());
}

#[test]
fn test_generate_insights_avg_duration() {
    let now = Utc::now();
    let two_hours_ago = now - Duration::hours(2);
    let one_hour_ago = now - Duration::hours(1);

    let sessions = vec![
        create_test_session(None, two_hours_ago, Some(one_hour_ago), 50, 20), // 1 hour
        create_test_session(None, one_hour_ago, Some(now), 30, 10),           // 1 hour
    ];
    let projects: Vec<ProjectWorkSummary> = vec![];

    let insights = WorkSummaryGenerator::generate_insights(&sessions, &projects, 7200);

    // Average duration: 7200/2 = 3600s = 1h
    assert!(insights.iter().any(|i| i.contains("Average session duration: 1h")));
}

#[test]
fn test_generate_insights_tool_usage_rate() {
    let now = Utc::now();
    let one_hour_ago = now - Duration::hours(1);

    let sessions = vec![
        create_test_session(None, one_hour_ago, Some(now), 120, 40), // 120 tools in 1 hour
    ];
    let projects: Vec<ProjectWorkSummary> = vec![];

    let insights = WorkSummaryGenerator::generate_insights(&sessions, &projects, 3600);

    // 120 calls / 1 hour = 120 calls/hour
    assert!(insights.iter().any(|i| i.contains("Tool usage rate: 120.0 calls/hour")));
}

#[test]
fn test_generate_insights_highly_focused() {
    let now = Utc::now();
    let sessions = vec![
        create_test_session(None, now - Duration::hours(1), Some(now), 50, 20),
    ];
    let projects = vec![
        create_test_project_summary("focused-project", 3600, 1, 50, 20),
    ];

    // 100% of time on one project
    let insights = WorkSummaryGenerator::generate_insights(&sessions, &projects, 3600);

    assert!(insights.iter().any(|i| i.contains("Highly focused: 100% of time on focused-project")));
}

#[test]
fn test_generate_insights_context_switching() {
    let now = Utc::now();
    let sessions = vec![
        create_test_session(None, now - Duration::hours(1), Some(now), 50, 20),
    ];
    let projects = vec![
        create_test_project_summary("project-a", 1200, 1, 20, 10),
        create_test_project_summary("project-b", 1200, 1, 15, 5),
        create_test_project_summary("project-c", 1200, 1, 15, 5),
    ];

    // 33% each, more than 2 projects
    let insights = WorkSummaryGenerator::generate_insights(&sessions, &projects, 3600);

    assert!(insights.iter().any(|i| i.contains("Context switching: work spread across 3 projects")));
}

#[test]
fn test_generate_insights_active_sessions() {
    let now = Utc::now();
    let sessions = vec![
        create_test_session(None, now - Duration::hours(1), None, 50, 20), // Active (no end time)
    ];
    let projects: Vec<ProjectWorkSummary> = vec![];

    let insights = WorkSummaryGenerator::generate_insights(&sessions, &projects, 3600);

    assert!(insights.iter().any(|i| i.contains("1 session(s) currently active")));
}

// ==================== WorkSummaryGenerator::generate_suggestions tests ====================

#[test]
fn test_generate_suggestions_empty_sessions() {
    let sessions: Vec<ClaudeSession> = vec![];
    let projects: Vec<ProjectWorkSummary> = vec![];

    let suggestions = WorkSummaryGenerator::generate_suggestions(&sessions, &projects);

    assert_eq!(suggestions.len(), 1);
    assert!(suggestions[0].contains("Start a Claude Code session"));
}

#[test]
fn test_generate_suggestions_multiple_active_sessions() {
    let now = Utc::now();
    let sessions = vec![
        create_test_session(None, now - Duration::hours(2), None, 50, 20), // Active
        create_test_session(None, now - Duration::hours(1), None, 30, 10), // Active
        create_test_session(None, now - Duration::minutes(30), None, 20, 5), // Active
    ];
    let projects: Vec<ProjectWorkSummary> = vec![];

    let suggestions = WorkSummaryGenerator::generate_suggestions(&sessions, &projects);

    // 3 active sessions, suggest closing 2
    assert!(suggestions.iter().any(|s| s.contains("Consider closing 2 inactive sessions")));
}

#[test]
fn test_generate_suggestions_low_tool_usage() {
    let now = Utc::now();
    let sessions = vec![
        // Ended session with 0 tool calls but > 60 seconds
        create_test_session(None, now - Duration::minutes(10), Some(now - Duration::minutes(5)), 0, 5),
    ];
    let projects: Vec<ProjectWorkSummary> = vec![];

    let suggestions = WorkSummaryGenerator::generate_suggestions(&sessions, &projects);

    assert!(suggestions.iter().any(|s| s.contains("no tool calls")));
}

#[test]
fn test_generate_suggestions_brief_project_work() {
    let now = Utc::now();
    let sessions = vec![
        create_test_session(None, now - Duration::hours(1), Some(now), 50, 20),
    ];
    // Project with only 1 session and < 300 seconds (5 minutes)
    let projects = vec![
        create_test_project_summary("quick-project", 180, 1, 10, 5),
    ];

    let suggestions = WorkSummaryGenerator::generate_suggestions(&sessions, &projects);

    assert!(suggestions.iter().any(|s| s.contains("Brief work on quick-project")));
}

#[test]
fn test_generate_suggestions_no_issues() {
    let now = Utc::now();
    // Normal session with tool calls
    let sessions = vec![
        create_test_session(None, now - Duration::hours(1), Some(now), 50, 20),
    ];
    // Project with sufficient time
    let projects = vec![
        create_test_project_summary("good-project", 3600, 2, 50, 20),
    ];

    let suggestions = WorkSummaryGenerator::generate_suggestions(&sessions, &projects);

    // Should have no specific suggestions
    assert!(suggestions.is_empty());
}

// ==================== ProjectWorkSummary tests ====================

#[test]
fn test_project_work_summary_creation() {
    let summary = create_test_project_summary("test-project", 7200, 3, 100, 40);

    assert_eq!(summary.project.name, "test-project");
    assert_eq!(summary.total_seconds, 7200);
    assert_eq!(summary.session_count, 3);
    assert_eq!(summary.tool_calls, 100);
    assert_eq!(summary.prompt_count, 40);
}

#[test]
fn test_project_work_summary_clone() {
    let summary = create_test_project_summary("cloneable", 3600, 2, 50, 20);
    let cloned = summary.clone();

    assert_eq!(summary.project.name, cloned.project.name);
    assert_eq!(summary.total_seconds, cloned.total_seconds);
    assert_eq!(summary.session_count, cloned.session_count);
}

// ==================== Edge cases ====================

#[test]
fn test_generate_text_zero_total_seconds() {
    // When total_seconds is 0, percentage calculation should handle division
    let summary = WorkSummary {
        period: SummaryPeriod::Today,
        total_seconds: 0,
        session_count: 1,
        total_tool_calls: 10,
        total_prompts: 5,
        projects: vec![create_test_project_summary("empty-time", 0, 1, 10, 5)],
        sessions: vec![],
        insights: vec![],
        suggestions: vec![],
    };
    let output = summary.generate_text();

    // Should show 0% without crashing
    assert!(output.contains("empty-time (0%)"));
}

#[test]
fn test_summary_period_custom_same_day() {
    let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
    let period = SummaryPeriod::Custom { start: date, end: date };

    let (start, end) = period.date_range();
    assert_eq!(start.date_naive(), end.date_naive());
    assert_eq!(period.display_name(), "2024-06-15 to 2024-06-15");
}

#[test]
fn test_generate_brief_yesterday_period() {
    let summary = WorkSummary {
        period: SummaryPeriod::Yesterday,
        total_seconds: 1800,
        session_count: 1,
        total_tool_calls: 25,
        total_prompts: 10,
        projects: vec![create_test_project_summary("yesterday-work", 1800, 1, 25, 10)],
        sessions: vec![],
        insights: vec![],
        suggestions: vec![],
    };
    let output = summary.generate_brief();

    assert!(output.starts_with("Yesterday:"));
    assert!(output.contains("30m of AI-assisted development"));
}
