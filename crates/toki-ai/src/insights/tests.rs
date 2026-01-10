use super::*;
use chrono::Utc;
use uuid::Uuid;

// ============================================================================
// Helper functions for creating test data
// ============================================================================

fn create_activity(app_bundle_id: &str, category: &str, duration: u32, is_active: bool) -> Activity {
    Activity {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        app_bundle_id: app_bundle_id.to_string(),
        category: category.to_string(),
        duration_seconds: duration,
        is_active,
        work_item_id: None,
    }
}

fn create_span(app_bundle_id: &str, category: &str, duration: u32) -> ActivitySpan {
    ActivitySpan {
        id: Uuid::new_v4(),
        app_bundle_id: app_bundle_id.to_string(),
        category: category.to_string(),
        start_time: Utc::now(),
        end_time: None,
        duration_seconds: duration,
        project_id: None,
        work_item_id: None,
        session_id: None,
        context: None,
    }
}

// ============================================================================
// time_per_category tests
// ============================================================================

#[test]
fn test_time_per_category_empty() {
    let activities: Vec<Activity> = vec![];
    let result = InsightsGenerator::time_per_category(&activities);
    assert!(result.is_empty());
}

#[test]
fn test_time_per_category_single_activity() {
    let activities = vec![create_activity("com.app.vscode", "Development", 3600, true)];
    let result = InsightsGenerator::time_per_category(&activities);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get("Development"), Some(&3600));
}

#[test]
fn test_time_per_category_multiple_same_category() {
    let activities = vec![
        create_activity("com.app.vscode", "Development", 1800, true),
        create_activity("com.app.jetbrains", "Development", 1200, true),
    ];
    let result = InsightsGenerator::time_per_category(&activities);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get("Development"), Some(&3000)); // 1800 + 1200
}

#[test]
fn test_time_per_category_multiple_categories() {
    let activities = vec![
        create_activity("com.app.vscode", "Development", 3600, true),
        create_activity("com.app.chrome", "Browser", 1800, true),
        create_activity("com.app.slack", "Communication", 900, true),
    ];
    let result = InsightsGenerator::time_per_category(&activities);
    assert_eq!(result.len(), 3);
    assert_eq!(result.get("Development"), Some(&3600));
    assert_eq!(result.get("Browser"), Some(&1800));
    assert_eq!(result.get("Communication"), Some(&900));
}

#[test]
fn test_time_per_category_includes_inactive() {
    let activities = vec![
        create_activity("com.app.vscode", "Development", 3600, true),
        create_activity("com.app.vscode", "Development", 1800, false), // inactive
    ];
    let result = InsightsGenerator::time_per_category(&activities);
    // time_per_category includes all activities regardless of is_active
    assert_eq!(result.get("Development"), Some(&5400)); // 3600 + 1800
}

// ============================================================================
// time_per_category_from_spans tests
// ============================================================================

#[test]
fn test_time_per_category_from_spans_empty() {
    let spans: Vec<ActivitySpan> = vec![];
    let result = InsightsGenerator::time_per_category_from_spans(&spans);
    assert!(result.is_empty());
}

#[test]
fn test_time_per_category_from_spans_single() {
    let spans = vec![create_span("com.app.vscode", "Development", 3600)];
    let result = InsightsGenerator::time_per_category_from_spans(&spans);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get("Development"), Some(&3600));
}

#[test]
fn test_time_per_category_from_spans_aggregates() {
    let spans = vec![
        create_span("com.app.vscode", "Development", 1800),
        create_span("com.app.jetbrains", "Development", 1200),
        create_span("com.app.chrome", "Browser", 600),
    ];
    let result = InsightsGenerator::time_per_category_from_spans(&spans);
    assert_eq!(result.len(), 2);
    assert_eq!(result.get("Development"), Some(&3000));
    assert_eq!(result.get("Browser"), Some(&600));
}

#[test]
fn test_time_per_category_from_spans_many_categories() {
    let spans = vec![
        create_span("app1", "Cat1", 100),
        create_span("app2", "Cat2", 200),
        create_span("app3", "Cat3", 300),
        create_span("app4", "Cat4", 400),
        create_span("app5", "Cat5", 500),
    ];
    let result = InsightsGenerator::time_per_category_from_spans(&spans);
    assert_eq!(result.len(), 5);
    assert_eq!(result.get("Cat1"), Some(&100));
    assert_eq!(result.get("Cat5"), Some(&500));
}

// ============================================================================
// total_active_time tests
// ============================================================================

#[test]
fn test_total_active_time_empty() {
    let activities: Vec<Activity> = vec![];
    let result = InsightsGenerator::total_active_time(&activities);
    assert_eq!(result, 0);
}

#[test]
fn test_total_active_time_all_active() {
    let activities = vec![
        create_activity("app1", "Dev", 1000, true),
        create_activity("app2", "Dev", 2000, true),
        create_activity("app3", "Dev", 3000, true),
    ];
    let result = InsightsGenerator::total_active_time(&activities);
    assert_eq!(result, 6000);
}

#[test]
fn test_total_active_time_all_inactive() {
    let activities = vec![
        create_activity("app1", "Dev", 1000, false),
        create_activity("app2", "Dev", 2000, false),
    ];
    let result = InsightsGenerator::total_active_time(&activities);
    assert_eq!(result, 0);
}

#[test]
fn test_total_active_time_mixed() {
    let activities = vec![
        create_activity("app1", "Dev", 1000, true),
        create_activity("app2", "Dev", 2000, false), // not counted
        create_activity("app3", "Dev", 3000, true),
        create_activity("app4", "Dev", 4000, false), // not counted
    ];
    let result = InsightsGenerator::total_active_time(&activities);
    assert_eq!(result, 4000); // only 1000 + 3000
}

#[test]
fn test_total_active_time_single_active() {
    let activities = vec![create_activity("app1", "Dev", 5000, true)];
    let result = InsightsGenerator::total_active_time(&activities);
    assert_eq!(result, 5000);
}

#[test]
fn test_total_active_time_single_inactive() {
    let activities = vec![create_activity("app1", "Dev", 5000, false)];
    let result = InsightsGenerator::total_active_time(&activities);
    assert_eq!(result, 0);
}

// ============================================================================
// total_time_from_spans tests
// ============================================================================

#[test]
fn test_total_time_from_spans_empty() {
    let spans: Vec<ActivitySpan> = vec![];
    let result = InsightsGenerator::total_time_from_spans(&spans);
    assert_eq!(result, 0);
}

#[test]
fn test_total_time_from_spans_single() {
    let spans = vec![create_span("app1", "Dev", 3600)];
    let result = InsightsGenerator::total_time_from_spans(&spans);
    assert_eq!(result, 3600);
}

#[test]
fn test_total_time_from_spans_multiple() {
    let spans = vec![
        create_span("app1", "Dev", 1000),
        create_span("app2", "Dev", 2000),
        create_span("app3", "Dev", 3000),
    ];
    let result = InsightsGenerator::total_time_from_spans(&spans);
    assert_eq!(result, 6000);
}

#[test]
fn test_total_time_from_spans_with_zeros() {
    let spans = vec![
        create_span("app1", "Dev", 1000),
        create_span("app2", "Dev", 0),
        create_span("app3", "Dev", 2000),
    ];
    let result = InsightsGenerator::total_time_from_spans(&spans);
    assert_eq!(result, 3000);
}

// ============================================================================
// top_applications tests
// ============================================================================

#[test]
fn test_top_applications_empty() {
    let activities: Vec<Activity> = vec![];
    let result = InsightsGenerator::top_applications(&activities, 5);
    assert!(result.is_empty());
}

#[test]
fn test_top_applications_single() {
    let activities = vec![create_activity("com.app.vscode", "Dev", 3600, true)];
    let result = InsightsGenerator::top_applications(&activities, 5);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], ("com.app.vscode".to_string(), 3600));
}

#[test]
fn test_top_applications_sorted_descending() {
    let activities = vec![
        create_activity("app.small", "Dev", 100, true),
        create_activity("app.large", "Dev", 1000, true),
        create_activity("app.medium", "Dev", 500, true),
    ];
    let result = InsightsGenerator::top_applications(&activities, 5);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].0, "app.large");
    assert_eq!(result[0].1, 1000);
    assert_eq!(result[1].0, "app.medium");
    assert_eq!(result[1].1, 500);
    assert_eq!(result[2].0, "app.small");
    assert_eq!(result[2].1, 100);
}

#[test]
fn test_top_applications_respects_limit() {
    let activities = vec![
        create_activity("app1", "Dev", 500, true),
        create_activity("app2", "Dev", 400, true),
        create_activity("app3", "Dev", 300, true),
        create_activity("app4", "Dev", 200, true),
        create_activity("app5", "Dev", 100, true),
    ];
    let result = InsightsGenerator::top_applications(&activities, 3);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].0, "app1");
    assert_eq!(result[1].0, "app2");
    assert_eq!(result[2].0, "app3");
}

#[test]
fn test_top_applications_limit_larger_than_data() {
    let activities = vec![
        create_activity("app1", "Dev", 500, true),
        create_activity("app2", "Dev", 400, true),
    ];
    let result = InsightsGenerator::top_applications(&activities, 10);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_top_applications_aggregates_same_app() {
    let activities = vec![
        create_activity("com.app.vscode", "Dev", 1000, true),
        create_activity("com.app.vscode", "Dev", 2000, true),
        create_activity("com.app.chrome", "Browser", 500, true),
    ];
    let result = InsightsGenerator::top_applications(&activities, 5);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], ("com.app.vscode".to_string(), 3000));
    assert_eq!(result[1], ("com.app.chrome".to_string(), 500));
}

#[test]
fn test_top_applications_includes_inactive() {
    let activities = vec![
        create_activity("app.active", "Dev", 1000, true),
        create_activity("app.inactive", "Dev", 2000, false),
    ];
    let result = InsightsGenerator::top_applications(&activities, 5);
    // top_applications includes all activities regardless of is_active
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], ("app.inactive".to_string(), 2000));
    assert_eq!(result[1], ("app.active".to_string(), 1000));
}

#[test]
fn test_top_applications_limit_zero() {
    let activities = vec![
        create_activity("app1", "Dev", 1000, true),
        create_activity("app2", "Dev", 2000, true),
    ];
    let result = InsightsGenerator::top_applications(&activities, 0);
    assert!(result.is_empty());
}

// ============================================================================
// top_applications_from_spans tests
// ============================================================================

#[test]
fn test_top_applications_from_spans_empty() {
    let spans: Vec<ActivitySpan> = vec![];
    let result = InsightsGenerator::top_applications_from_spans(&spans, 5);
    assert!(result.is_empty());
}

#[test]
fn test_top_applications_from_spans_single() {
    let spans = vec![create_span("com.app.vscode", "Dev", 3600)];
    let result = InsightsGenerator::top_applications_from_spans(&spans, 5);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], ("com.app.vscode".to_string(), 3600));
}

#[test]
fn test_top_applications_from_spans_sorted() {
    let spans = vec![
        create_span("app.small", "Dev", 100),
        create_span("app.large", "Dev", 1000),
        create_span("app.medium", "Dev", 500),
    ];
    let result = InsightsGenerator::top_applications_from_spans(&spans, 5);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].0, "app.large");
    assert_eq!(result[1].0, "app.medium");
    assert_eq!(result[2].0, "app.small");
}

#[test]
fn test_top_applications_from_spans_respects_limit() {
    let spans = vec![
        create_span("app1", "Dev", 500),
        create_span("app2", "Dev", 400),
        create_span("app3", "Dev", 300),
        create_span("app4", "Dev", 200),
    ];
    let result = InsightsGenerator::top_applications_from_spans(&spans, 2);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].0, "app1");
    assert_eq!(result[1].0, "app2");
}

#[test]
fn test_top_applications_from_spans_aggregates() {
    let spans = vec![
        create_span("com.app.vscode", "Dev", 1000),
        create_span("com.app.vscode", "Dev", 1500),
        create_span("com.app.chrome", "Browser", 800),
    ];
    let result = InsightsGenerator::top_applications_from_spans(&spans, 5);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], ("com.app.vscode".to_string(), 2500));
    assert_eq!(result[1], ("com.app.chrome".to_string(), 800));
}

#[test]
fn test_top_applications_from_spans_limit_zero() {
    let spans = vec![create_span("app1", "Dev", 1000)];
    let result = InsightsGenerator::top_applications_from_spans(&spans, 0);
    assert!(result.is_empty());
}

#[test]
fn test_top_applications_from_spans_limit_one() {
    let spans = vec![
        create_span("app1", "Dev", 500),
        create_span("app2", "Dev", 1000),
        create_span("app3", "Dev", 750),
    ];
    let result = InsightsGenerator::top_applications_from_spans(&spans, 1);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, "app2");
    assert_eq!(result[0].1, 1000);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_large_duration_values() {
    let activities = vec![
        create_activity("app1", "Dev", u32::MAX - 100, true),
        create_activity("app1", "Dev", 50, true),
    ];
    let result = InsightsGenerator::time_per_category(&activities);
    // Should handle large values without overflow panic (Rust wraps in release)
    assert!(result.get("Dev").is_some());
}

#[test]
fn test_empty_category_name() {
    let activities = vec![create_activity("app1", "", 1000, true)];
    let result = InsightsGenerator::time_per_category(&activities);
    assert_eq!(result.get(""), Some(&1000));
}

#[test]
fn test_empty_app_bundle_id() {
    let activities = vec![create_activity("", "Dev", 1000, true)];
    let result = InsightsGenerator::top_applications(&activities, 5);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, "");
}

#[test]
fn test_unicode_category_names() {
    let activities = vec![
        create_activity("app1", "開発", 1000, true),
        create_activity("app2", "開発", 500, true),
        create_activity("app3", "ブラウザ", 300, true),
    ];
    let result = InsightsGenerator::time_per_category(&activities);
    assert_eq!(result.get("開発"), Some(&1500));
    assert_eq!(result.get("ブラウザ"), Some(&300));
}

#[test]
fn test_unicode_app_bundle_ids() {
    let spans = vec![
        create_span("com.アプリ.one", "Dev", 1000),
        create_span("com.アプリ.two", "Dev", 500),
    ];
    let result = InsightsGenerator::top_applications_from_spans(&spans, 5);
    assert_eq!(result.len(), 2);
}
