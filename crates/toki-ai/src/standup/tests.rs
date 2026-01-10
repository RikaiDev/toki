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
