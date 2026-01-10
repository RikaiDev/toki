use super::*;
use toki_integrations::IssueSyncReport;
use toki_ai::{SyncOutcome, SyncResult};

// Helper to create test sync results
fn create_sync_result(title: &str, outcome: SyncOutcome) -> SyncResult {
    SyncResult {
        page_id: "page-123".to_string(),
        title: title.to_string(),
        outcome,
    }
}

// Helper to create a sync report with common defaults
fn create_report(created: usize, skipped: usize, failed: usize) -> IssueSyncReport {
    IssueSyncReport {
        total: created + skipped + failed,
        created,
        updated: 0,
        skipped,
        failed,
        errors: Vec::new(),
    }
}

#[test]
fn test_format_sync_output_created() {
    let report = create_report(1, 0, 0);
    let results = vec![create_sync_result(
        "Add login feature",
        SyncOutcome::Created {
            issue_number: 42,
            issue_url: "https://github.com/org/repo/issues/42".to_string(),
        },
    )];

    let output = format_sync_output(&report, &results);

    assert!(output.contains("Sync completed:"));
    assert!(output.contains("[CREATED] #42 Add login feature"));
    assert!(output.contains("https://github.com/org/repo/issues/42"));
    assert!(output.contains("1 created, 0 skipped, 0 failed"));
}

#[test]
fn test_format_sync_output_skipped() {
    let report = create_report(0, 1, 0);
    let results = vec![create_sync_result(
        "Already synced task",
        SyncOutcome::Skipped {
            reason: "Already synced".to_string(),
        },
    )];

    let output = format_sync_output(&report, &results);

    assert!(output.contains("[SKIPPED] Already synced task - Already synced"));
    assert!(output.contains("0 created, 1 skipped, 0 failed"));
}

#[test]
fn test_format_sync_output_failed() {
    let report = create_report(0, 0, 1);
    let results = vec![create_sync_result(
        "Failed task",
        SyncOutcome::Failed {
            error: "API error".to_string(),
        },
    )];

    let output = format_sync_output(&report, &results);

    assert!(output.contains("[FAILED] Failed task - API error"));
    assert!(output.contains("0 created, 0 skipped, 1 failed"));
}

#[test]
fn test_format_sync_output_would_create() {
    let report = create_report(0, 0, 0);
    let results = vec![create_sync_result("Dry run task", SyncOutcome::WouldCreate)];

    let output = format_sync_output(&report, &results);

    assert!(output.contains("[WOULD CREATE] Dry run task"));
}

#[test]
fn test_format_sync_output_mixed_results() {
    let report = create_report(1, 1, 1);
    let results = vec![
        create_sync_result(
            "Task 1",
            SyncOutcome::Created {
                issue_number: 1,
                issue_url: "https://example.com/1".to_string(),
            },
        ),
        create_sync_result(
            "Task 2",
            SyncOutcome::Skipped {
                reason: "Already exists".to_string(),
            },
        ),
        create_sync_result(
            "Task 3",
            SyncOutcome::Failed {
                error: "Network error".to_string(),
            },
        ),
    ];

    let output = format_sync_output(&report, &results);

    assert!(output.contains("[CREATED] #1 Task 1"));
    assert!(output.contains("[SKIPPED] Task 2"));
    assert!(output.contains("[FAILED] Task 3"));
    assert!(output.contains("1 created, 1 skipped, 1 failed"));
}

#[test]
fn test_format_sync_output_empty_results() {
    let report = create_report(0, 0, 0);
    let results: Vec<SyncResult> = vec![];

    let output = format_sync_output(&report, &results);

    assert!(output.contains("Sync completed:"));
    assert!(output.contains("0 created, 0 skipped, 0 failed"));
}

// Request deserialization tests
#[test]
fn test_list_pages_request_deserialization() {
    let json = r#"{"database_id": "abc-123"}"#;
    let req: ListPagesRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.database_id, "abc-123");
}

#[test]
fn test_sync_to_github_request_deserialization() {
    let json = r#"{"database_id": "db-456", "repo": "owner/repo"}"#;
    let req: SyncToGitHubRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.database_id, "db-456");
    assert_eq!(req.repo, "owner/repo");
}

#[test]
fn test_sync_to_gitlab_request_deserialization() {
    let json = r#"{"database_id": "db-789", "project": "group/project"}"#;
    let req: SyncToGitLabRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.database_id, "db-789");
    assert_eq!(req.project, "group/project");
}

#[test]
fn test_config_get_request_deserialization() {
    let json = r#"{"key": "notion.api_key"}"#;
    let req: ConfigGetRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.key, "notion.api_key");
}

#[test]
fn test_config_set_request_deserialization() {
    let json = r#"{"key": "github.token", "value": "ghp_xxx"}"#;
    let req: ConfigSetRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.key, "github.token");
    assert_eq!(req.value, "ghp_xxx");
}

#[test]
fn test_suggest_issue_request_deserialization() {
    let json = r#"{"path": "/home/user/project", "max_suggestions": 10}"#;
    let req: SuggestIssueRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.path, "/home/user/project");
    assert_eq!(req.max_suggestions, Some(10));
}

#[test]
fn test_suggest_issue_request_defaults() {
    let json = r#"{"path": "/home/user/project"}"#;
    let req: SuggestIssueRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.path, "/home/user/project");
    assert_eq!(req.max_suggestions, None);
}

#[test]
fn test_generate_summary_request_deserialization() {
    let json = r#"{"period": "today", "format": "json", "project": "my-project"}"#;
    let req: GenerateSummaryRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.period, "today");
    assert_eq!(req.format, Some("json".to_string()));
    assert_eq!(req.project, Some("my-project".to_string()));
}

#[test]
fn test_generate_summary_request_minimal() {
    let json = r#"{"period": "week"}"#;
    let req: GenerateSummaryRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.period, "week");
    assert_eq!(req.format, None);
    assert_eq!(req.project, None);
}

#[test]
fn test_generate_standup_request_deserialization() {
    let json = r#"{"format": "slack", "date": "2024-01-15"}"#;
    let req: GenerateStandupRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.format, Some("slack".to_string()));
    assert_eq!(req.date, Some("2024-01-15".to_string()));
}

#[test]
fn test_generate_standup_request_defaults() {
    let json = r#"{}"#;
    let req: GenerateStandupRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.format, None);
    assert_eq!(req.date, None);
}

// Config key parsing tests
#[test]
fn test_config_key_parsing_valid() {
    let key = "notion.api_key";
    let parts: Vec<&str> = key.split('.').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], "notion");
    assert_eq!(parts[1], "api_key");
}

#[test]
fn test_config_key_parsing_invalid_no_dot() {
    let key = "notion_api_key";
    let parts: Vec<&str> = key.split('.').collect();
    assert_eq!(parts.len(), 1);
}

#[test]
fn test_config_key_parsing_invalid_multiple_dots() {
    let key = "notion.api.key";
    let parts: Vec<&str> = key.split('.').collect();
    assert_eq!(parts.len(), 3);
}

#[test]
fn test_config_key_valid_sections() {
    let valid_sections = ["notion", "github", "gitlab", "plane"];
    for section in valid_sections {
        let key = format!("{section}.api_key");
        let parts: Vec<&str> = key.split('.').collect();
        assert_eq!(parts.len(), 2);
        assert!(valid_sections.contains(&parts[0]));
    }
}

#[test]
fn test_config_key_valid_fields() {
    let valid_fields = ["api_key", "token", "api_url", "url"];
    for field in valid_fields {
        let key = format!("notion.{field}");
        let parts: Vec<&str> = key.split('.').collect();
        assert_eq!(parts.len(), 2);
        assert!(valid_fields.contains(&parts[1]));
    }
}
