use super::*;
use chrono::NaiveDate;

// ==================== Helper functions ====================

fn create_test_segment(
    project_name: Option<&str>,
    category: &str,
    start_offset_mins: i64,
    end_offset_mins: i64,
) -> ActivitySegment {
    let now = Utc::now();
    ActivitySegment {
        start_time: now - Duration::minutes(start_offset_mins),
        end_time: now - Duration::minutes(end_offset_mins),
        project_name: project_name.map(String::from),
        category: category.to_string(),
        edited_files: vec![],
        git_commits: vec![],
        git_branch: None,
        browser_urls: vec![],
    }
}

fn create_segment_with_files(files: Vec<&str>) -> ActivitySegment {
    let now = Utc::now();
    ActivitySegment {
        start_time: now - Duration::minutes(30),
        end_time: now,
        project_name: Some("test-project".to_string()),
        category: "Coding".to_string(),
        edited_files: files.into_iter().map(String::from).collect(),
        git_commits: vec![],
        git_branch: None,
        browser_urls: vec![],
    }
}

fn create_segment_with_commits(commits: Vec<&str>) -> ActivitySegment {
    let now = Utc::now();
    ActivitySegment {
        start_time: now - Duration::minutes(30),
        end_time: now,
        project_name: Some("test-project".to_string()),
        category: "Coding".to_string(),
        edited_files: vec![],
        git_commits: commits.into_iter().map(String::from).collect(),
        git_branch: None,
        browser_urls: vec![],
    }
}

fn create_segment_with_urls(urls: Vec<&str>) -> ActivitySegment {
    let now = Utc::now();
    ActivitySegment {
        start_time: now - Duration::minutes(30),
        end_time: now,
        project_name: Some("test-project".to_string()),
        category: "Browser".to_string(),
        edited_files: vec![],
        git_commits: vec![],
        git_branch: None,
        browser_urls: urls.into_iter().map(String::from).collect(),
    }
}

fn create_segment_with_branch(branch: &str) -> ActivitySegment {
    let now = Utc::now();
    ActivitySegment {
        start_time: now - Duration::minutes(30),
        end_time: now,
        project_name: Some("test-project".to_string()),
        category: "Coding".to_string(),
        edited_files: vec![],
        git_commits: vec![],
        git_branch: Some(branch.to_string()),
        browser_urls: vec![],
    }
}

// ==================== seconds_to_u32 tests ====================

#[test]
fn test_seconds_to_u32_positive() {
    assert_eq!(seconds_to_u32(100), 100);
    assert_eq!(seconds_to_u32(3600), 3600);
}

#[test]
fn test_seconds_to_u32_zero() {
    assert_eq!(seconds_to_u32(0), 0);
}

#[test]
fn test_seconds_to_u32_negative() {
    assert_eq!(seconds_to_u32(-100), 0);
    assert_eq!(seconds_to_u32(-1), 0);
}

#[test]
fn test_seconds_to_u32_max() {
    assert_eq!(seconds_to_u32(i64::from(u32::MAX)), u32::MAX);
}

#[test]
fn test_seconds_to_u32_overflow() {
    assert_eq!(seconds_to_u32(i64::from(u32::MAX) + 1), u32::MAX);
    assert_eq!(seconds_to_u32(i64::MAX), u32::MAX);
}

// ==================== duration_seconds tests ====================

#[test]
fn test_duration_seconds_positive() {
    let start = Utc::now() - Duration::hours(1);
    let end = Utc::now();
    assert_eq!(duration_seconds(start, end), 3600);
}

#[test]
fn test_duration_seconds_same_time() {
    let now = Utc::now();
    assert_eq!(duration_seconds(now, now), 0);
}

#[test]
fn test_duration_seconds_negative() {
    let start = Utc::now();
    let end = Utc::now() - Duration::hours(1);
    assert_eq!(duration_seconds(start, end), 0);
}

// ==================== format_duration tests ====================

#[test]
fn test_format_duration_minutes_only() {
    assert_eq!(format_duration(300), "5m");
    assert_eq!(format_duration(60), "1m");
    assert_eq!(format_duration(0), "0m");
}

#[test]
fn test_format_duration_hours_and_minutes() {
    assert_eq!(format_duration(3600), "1h 0m");
    assert_eq!(format_duration(3660), "1h 1m");
    assert_eq!(format_duration(7200), "2h 0m");
    assert_eq!(format_duration(5400), "1h 30m");
}

// ==================== TimeAnalyzer::new tests ====================

#[test]
fn test_time_analyzer_new() {
    let analyzer = TimeAnalyzer::new();
    assert_eq!(analyzer.min_block_duration, Duration::minutes(5));
}

#[test]
fn test_time_analyzer_default() {
    let analyzer = TimeAnalyzer::default();
    assert_eq!(analyzer.min_block_duration, Duration::minutes(5));
}

// ==================== WorkPattern tests ====================

#[test]
fn test_work_pattern_equality() {
    assert_eq!(WorkPattern::SingleFocus, WorkPattern::SingleFocus);
    assert_eq!(WorkPattern::Debugging, WorkPattern::Debugging);
    assert_ne!(WorkPattern::SingleFocus, WorkPattern::Debugging);
}

#[test]
fn test_work_pattern_clone() {
    let pattern = WorkPattern::CodeReview;
    let cloned = pattern.clone();
    assert_eq!(pattern, cloned);
}

// ==================== detect_pattern tests ====================

#[test]
fn test_detect_pattern_from_test_files() {
    let segment = create_segment_with_files(vec!["src/test_utils.rs", "tests/main.rs"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Debugging);
}

#[test]
fn test_detect_pattern_from_spec_files() {
    let segment = create_segment_with_files(vec!["spec/models/user_spec.rb"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Debugging);
}

#[test]
fn test_detect_pattern_from_readme_files() {
    let segment = create_segment_with_files(vec!["README.md"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Documentation);
}

#[test]
fn test_detect_pattern_from_doc_files() {
    let segment = create_segment_with_files(vec!["docs/api.md"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Documentation);
}

#[test]
fn test_detect_pattern_from_changelog() {
    let segment = create_segment_with_files(vec!["CHANGELOG.md"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Documentation);
}

#[test]
fn test_detect_pattern_from_fix_commit() {
    let segment = create_segment_with_commits(vec!["fix: resolve button bug"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Debugging);
}

#[test]
fn test_detect_pattern_from_bug_commit() {
    let segment = create_segment_with_commits(vec!["Bug in login flow"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Debugging);
}

#[test]
fn test_detect_pattern_from_refactor_commit() {
    let segment = create_segment_with_commits(vec!["refactor: extract helper methods"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Maintenance);
}

#[test]
fn test_detect_pattern_from_clean_commit() {
    let segment = create_segment_with_commits(vec!["clean up unused imports"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Maintenance);
}

#[test]
fn test_detect_pattern_from_docs_commit() {
    let segment = create_segment_with_commits(vec!["docs: update API documentation"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Documentation);
}

#[test]
fn test_detect_pattern_from_review_commit() {
    let segment = create_segment_with_commits(vec!["review: address PR comments"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::CodeReview);
}

#[test]
fn test_detect_pattern_from_pull_url() {
    let segment = create_segment_with_urls(vec!["https://github.com/org/repo/pull/123"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::CodeReview);
}

#[test]
fn test_detect_pattern_from_merge_url() {
    let segment = create_segment_with_urls(vec!["https://gitlab.com/group/project/-/merge_requests/42"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::CodeReview);
}

#[test]
fn test_detect_pattern_from_issue_url() {
    let segment = create_segment_with_urls(vec!["https://github.com/org/repo/issues/456"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::SingleFocus);
}

#[test]
fn test_detect_pattern_from_ticket_url() {
    let segment = create_segment_with_urls(vec!["https://jira.company.com/ticket/ABC-123"]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::SingleFocus);
}

#[test]
fn test_detect_pattern_multitasking() {
    let segment = create_segment_with_files(vec![
        "file1.rs", "file2.rs", "file3.rs", "file4.rs", "file5.rs", "file6.rs"
    ]);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::MultiTasking);
}

#[test]
fn test_detect_pattern_exploration_stackoverflow() {
    let mut segment = create_segment_with_urls(vec!["https://stackoverflow.com/questions/123"]);
    segment.category = "Browser".to_string();
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Exploration);
}

#[test]
fn test_detect_pattern_exploration_docs() {
    let mut segment = create_segment_with_urls(vec!["https://docs.rust-lang.org/book/"]);
    segment.category = "Browser".to_string();
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Exploration);
}

#[test]
fn test_detect_pattern_meeting() {
    let mut segment = create_test_segment(Some("project"), "Communication", 30, 0);
    segment.category = "Communication".to_string();
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::Meeting);
}

#[test]
fn test_detect_pattern_default_single_focus() {
    let segment = create_test_segment(Some("project"), "Coding", 30, 0);
    assert_eq!(TimeAnalyzer::detect_pattern(&segment), WorkPattern::SingleFocus);
}

// ==================== should_merge_segments tests ====================

#[test]
fn test_should_merge_segments_no_prev_end() {
    let next_start = Utc::now();
    assert!(!TimeAnalyzer::should_merge_segments(None, next_start));
}

#[test]
fn test_should_merge_segments_small_gap() {
    let now = Utc::now();
    let prev_end = now - Duration::minutes(5);
    assert!(TimeAnalyzer::should_merge_segments(Some(prev_end), now));
}

#[test]
fn test_should_merge_segments_large_gap() {
    let now = Utc::now();
    let prev_end = now - Duration::minutes(15);
    assert!(!TimeAnalyzer::should_merge_segments(Some(prev_end), now));
}

#[test]
fn test_should_merge_segments_exact_boundary() {
    let now = Utc::now();
    let prev_end = now - Duration::minutes(10);
    // Gap is exactly 10 minutes, should NOT merge (< 10, not <=)
    assert!(!TimeAnalyzer::should_merge_segments(Some(prev_end), now));
}

#[test]
fn test_should_merge_segments_just_under_boundary() {
    let now = Utc::now();
    let prev_end = now - Duration::minutes(9) - Duration::seconds(59);
    assert!(TimeAnalyzer::should_merge_segments(Some(prev_end), now));
}

// ==================== extract_issues tests ====================

#[test]
fn test_extract_issues_from_branch() {
    let segment = create_segment_with_branch("feature/TOKI-42-add-feature");
    let issues = TimeAnalyzer::extract_issues(&segment);

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].issue_id, "TOKI-42");
    assert!((issues[0].confidence - 0.9).abs() < 0.01);
    assert!(issues[0].reason.contains("Git branch"));
}

#[test]
fn test_extract_issues_from_commit() {
    let segment = create_segment_with_commits(vec!["PROJ-123: Add new feature"]);
    let issues = TimeAnalyzer::extract_issues(&segment);

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].issue_id, "PROJ-123");
    assert!((issues[0].confidence - 0.8).abs() < 0.01);
}

#[test]
fn test_extract_issues_from_url() {
    let segment = create_segment_with_urls(vec!["https://jira.com/browse/ABC-789"]);
    let issues = TimeAnalyzer::extract_issues(&segment);

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].issue_id, "ABC-789");
    assert!((issues[0].confidence - 0.7).abs() < 0.01);
}

#[test]
fn test_extract_issues_multiple_sources() {
    let now = Utc::now();
    let segment = ActivitySegment {
        start_time: now - Duration::minutes(30),
        end_time: now,
        project_name: Some("test".to_string()),
        category: "Coding".to_string(),
        edited_files: vec![],
        git_commits: vec!["PROJ-123: implement feature".to_string()],
        git_branch: Some("feature/PROJ-123-feature".to_string()),
        browser_urls: vec!["https://jira.com/PROJ-456".to_string()],
    };

    let issues = TimeAnalyzer::extract_issues(&segment);

    // Should have 2 issues (PROJ-123 deduplicated from branch/commit, PROJ-456 from URL)
    assert_eq!(issues.len(), 2);
    assert!(issues.iter().any(|i| i.issue_id == "PROJ-123"));
    assert!(issues.iter().any(|i| i.issue_id == "PROJ-456"));
}

#[test]
fn test_extract_issues_no_matches() {
    let segment = create_test_segment(Some("project"), "Coding", 30, 0);
    let issues = TimeAnalyzer::extract_issues(&segment);
    assert!(issues.is_empty());
}

#[test]
fn test_extract_issues_lowercase_converted() {
    let segment = create_segment_with_branch("feature/proj-42-lowercase");
    let issues = TimeAnalyzer::extract_issues(&segment);

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].issue_id, "PROJ-42"); // Should be uppercased
}

// ==================== calculate_confidence tests ====================

#[test]
fn test_calculate_confidence_with_issues() {
    let issues = vec![
        SuggestedIssue { issue_id: "A-1".to_string(), confidence: 0.8, reason: "test".to_string() },
        SuggestedIssue { issue_id: "B-2".to_string(), confidence: 0.9, reason: "test".to_string() },
    ];

    let confidence = TimeAnalyzer::calculate_confidence(&issues, &WorkPattern::SingleFocus);
    assert!((confidence - 0.9).abs() < 0.01); // Should take highest
}

#[test]
fn test_calculate_confidence_empty_exploration() {
    let confidence = TimeAnalyzer::calculate_confidence(&[], &WorkPattern::Exploration);
    assert!((confidence - 0.6).abs() < 0.01);
}

#[test]
fn test_calculate_confidence_empty_maintenance() {
    let confidence = TimeAnalyzer::calculate_confidence(&[], &WorkPattern::Maintenance);
    assert!((confidence - 0.6).abs() < 0.01);
}

#[test]
fn test_calculate_confidence_empty_documentation() {
    let confidence = TimeAnalyzer::calculate_confidence(&[], &WorkPattern::Documentation);
    assert!((confidence - 0.6).abs() < 0.01);
}

#[test]
fn test_calculate_confidence_empty_meeting() {
    let confidence = TimeAnalyzer::calculate_confidence(&[], &WorkPattern::Meeting);
    assert!((confidence - 0.7).abs() < 0.01);
}

#[test]
fn test_calculate_confidence_empty_code_review() {
    let confidence = TimeAnalyzer::calculate_confidence(&[], &WorkPattern::CodeReview);
    assert!((confidence - 0.7).abs() < 0.01);
}

#[test]
fn test_calculate_confidence_empty_default() {
    let confidence = TimeAnalyzer::calculate_confidence(&[], &WorkPattern::SingleFocus);
    assert!((confidence - 0.3).abs() < 0.01);
}

// ==================== generate_description tests ====================

#[test]
fn test_generate_description_single_focus_with_commit() {
    let segment = create_segment_with_commits(vec!["Add user authentication"]);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::SingleFocus);
    assert_eq!(desc, "Add user authentication");
}

#[test]
fn test_generate_description_single_focus_no_commit() {
    let segment = create_test_segment(Some("myproject"), "Coding", 30, 0);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::SingleFocus);
    assert_eq!(desc, "Development on myproject");
}

#[test]
fn test_generate_description_multitasking() {
    let segment = create_test_segment(Some("myproject"), "Coding", 30, 0);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::MultiTasking);
    assert_eq!(desc, "Multi-tasking - myproject");
}

#[test]
fn test_generate_description_exploration() {
    let segment = create_test_segment(Some("myproject"), "Browser", 30, 0);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::Exploration);
    assert_eq!(desc, "Exploration/Learning");
}

#[test]
fn test_generate_description_maintenance() {
    let segment = create_test_segment(Some("myproject"), "Coding", 30, 0);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::Maintenance);
    assert_eq!(desc, "myproject maintenance/refactoring");
}

#[test]
fn test_generate_description_code_review() {
    let segment = create_test_segment(Some("myproject"), "Coding", 30, 0);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::CodeReview);
    assert_eq!(desc, "Code Review");
}

#[test]
fn test_generate_description_debugging() {
    let segment = create_test_segment(Some("myproject"), "Coding", 30, 0);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::Debugging);
    assert_eq!(desc, "myproject debugging");
}

#[test]
fn test_generate_description_meeting() {
    let segment = create_test_segment(Some("myproject"), "Communication", 30, 0);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::Meeting);
    assert_eq!(desc, "Meeting/Communication");
}

#[test]
fn test_generate_description_documentation() {
    let segment = create_test_segment(Some("myproject"), "Coding", 30, 0);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::Documentation);
    assert_eq!(desc, "Documentation");
}

#[test]
fn test_generate_description_unknown() {
    let segment = create_test_segment(Some("myproject"), "Coding", 30, 0);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::Unknown);
    assert_eq!(desc, "Working on myproject");
}

#[test]
fn test_generate_description_no_project() {
    let segment = create_test_segment(None, "Coding", 30, 0);
    let desc = TimeAnalyzer::generate_description(&segment, &WorkPattern::SingleFocus);
    assert_eq!(desc, "Development on unknown");
}

// ==================== generate_reasoning tests ====================

#[test]
fn test_generate_reasoning_basic() {
    let segment = create_test_segment(Some("project"), "Coding", 30, 0);
    let reasons = TimeAnalyzer::generate_reasoning(&segment, &WorkPattern::SingleFocus, &[]);

    assert!(reasons.iter().any(|r| r.contains("Work pattern: SingleFocus")));
    assert!(reasons.iter().any(|r| r.contains("No related Issue ID detected")));
}

#[test]
fn test_generate_reasoning_with_files() {
    let segment = create_segment_with_files(vec!["file1.rs", "file2.rs"]);
    let reasons = TimeAnalyzer::generate_reasoning(&segment, &WorkPattern::Debugging, &[]);

    assert!(reasons.iter().any(|r| r.contains("Edited 2 files")));
}

#[test]
fn test_generate_reasoning_with_commits() {
    let segment = create_segment_with_commits(vec!["commit1", "commit2", "commit3"]);
    let reasons = TimeAnalyzer::generate_reasoning(&segment, &WorkPattern::SingleFocus, &[]);

    assert!(reasons.iter().any(|r| r.contains("Made 3 commits")));
}

#[test]
fn test_generate_reasoning_with_issues() {
    let segment = create_test_segment(Some("project"), "Coding", 30, 0);
    let issues = vec![
        SuggestedIssue { issue_id: "PROJ-123".to_string(), confidence: 0.9, reason: "From branch".to_string() },
    ];
    let reasons = TimeAnalyzer::generate_reasoning(&segment, &WorkPattern::SingleFocus, &issues);

    assert!(reasons.iter().any(|r| r.contains("PROJ-123: From branch")));
    assert!(!reasons.iter().any(|r| r.contains("No related Issue ID detected")));
}

// ==================== analyze_and_suggest tests ====================

#[test]
fn test_analyze_and_suggest_empty() {
    let analyzer = TimeAnalyzer::new();
    let suggestions = analyzer.analyze_and_suggest(&[]);
    assert!(suggestions.is_empty());
}

#[test]
fn test_analyze_and_suggest_single_segment() {
    let analyzer = TimeAnalyzer::new();
    let now = Utc::now();
    let segment = ActivitySegment {
        start_time: now - Duration::minutes(30),
        end_time: now,
        project_name: Some("test".to_string()),
        category: "Coding".to_string(),
        edited_files: vec![],
        git_commits: vec!["PROJ-123: Add feature".to_string()],
        git_branch: None,
        browser_urls: vec![],
    };

    let suggestions = analyzer.analyze_and_suggest(&[segment]);

    assert_eq!(suggestions.len(), 1);
    assert!(suggestions[0].duration_seconds >= 1800); // ~30 minutes
}

#[test]
fn test_analyze_and_suggest_filters_short_blocks() {
    let analyzer = TimeAnalyzer::new();
    let now = Utc::now();
    let segment = ActivitySegment {
        start_time: now - Duration::minutes(2), // Only 2 minutes
        end_time: now,
        project_name: Some("test".to_string()),
        category: "Coding".to_string(),
        edited_files: vec![],
        git_commits: vec![],
        git_branch: None,
        browser_urls: vec![],
    };

    let suggestions = analyzer.analyze_and_suggest(&[segment]);

    // Should be filtered out (< 5 minutes)
    assert!(suggestions.is_empty());
}

#[test]
fn test_analyze_and_suggest_merges_similar_patterns() {
    let analyzer = TimeAnalyzer::new();
    let now = Utc::now();

    // Two segments with same pattern and small gap
    let segments = vec![
        ActivitySegment {
            start_time: now - Duration::minutes(30),
            end_time: now - Duration::minutes(15),
            project_name: Some("test".to_string()),
            category: "Coding".to_string(),
            edited_files: vec!["test_file.rs".to_string()], // Debugging pattern
            git_commits: vec![],
            git_branch: None,
            browser_urls: vec![],
        },
        ActivitySegment {
            start_time: now - Duration::minutes(10), // 5 min gap (< 10 min)
            end_time: now,
            project_name: Some("test".to_string()),
            category: "Coding".to_string(),
            edited_files: vec!["tests/other.rs".to_string()], // Same Debugging pattern
            git_commits: vec![],
            git_branch: None,
            browser_urls: vec![],
        },
    ];

    let suggestions = analyzer.analyze_and_suggest(&segments);

    // Should merge into single block
    assert_eq!(suggestions.len(), 1);
}

#[test]
fn test_analyze_and_suggest_separates_different_patterns() {
    let analyzer = TimeAnalyzer::new();
    let now = Utc::now();

    let segments = vec![
        ActivitySegment {
            start_time: now - Duration::minutes(60),
            end_time: now - Duration::minutes(35),
            project_name: Some("test".to_string()),
            category: "Coding".to_string(),
            edited_files: vec!["test_file.rs".to_string()], // Debugging
            git_commits: vec![],
            git_branch: None,
            browser_urls: vec![],
        },
        ActivitySegment {
            start_time: now - Duration::minutes(30),
            end_time: now,
            project_name: Some("test".to_string()),
            category: "Coding".to_string(),
            edited_files: vec!["README.md".to_string()], // Documentation
            git_commits: vec![],
            git_branch: None,
            browser_urls: vec![],
        },
    ];

    let suggestions = analyzer.analyze_and_suggest(&segments);

    // Should have 2 separate blocks
    assert_eq!(suggestions.len(), 2);
}

// ==================== generate_daily_summary tests ====================

#[test]
fn test_generate_daily_summary_empty() {
    let analyzer = TimeAnalyzer::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    let report = analyzer.generate_daily_summary(date, &[]);

    assert_eq!(report.date, date);
    assert_eq!(report.total_active_seconds, 0);
    assert_eq!(report.classified_seconds, 0);
    assert_eq!(report.unclassified_seconds, 0);
    assert!(report.project_breakdown.is_empty());
    assert!(report.suggested_blocks.is_empty());
}

#[test]
fn test_generate_daily_summary_with_data() {
    let analyzer = TimeAnalyzer::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let now = Utc::now();

    let segments = vec![
        ActivitySegment {
            start_time: now - Duration::hours(2),
            end_time: now - Duration::hours(1),
            project_name: Some("project-a".to_string()),
            category: "Coding".to_string(),
            edited_files: vec![],
            git_commits: vec!["PROJ-1: feature".to_string()],
            git_branch: None,
            browser_urls: vec![],
        },
        ActivitySegment {
            start_time: now - Duration::hours(1),
            end_time: now,
            project_name: Some("project-b".to_string()),
            category: "Coding".to_string(),
            edited_files: vec![],
            git_commits: vec![],
            git_branch: None,
            browser_urls: vec![],
        },
    ];

    let report = analyzer.generate_daily_summary(date, &segments);

    assert_eq!(report.date, date);
    assert!(report.total_active_seconds > 0);
    assert_eq!(report.project_breakdown.len(), 2);
    assert!(report.project_breakdown.contains_key("project-a"));
    assert!(report.project_breakdown.contains_key("project-b"));
}

#[test]
fn test_generate_daily_summary_project_times() {
    let analyzer = TimeAnalyzer::new();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let now = Utc::now();

    let segments = vec![
        ActivitySegment {
            start_time: now - Duration::hours(2),
            end_time: now - Duration::hours(1),
            project_name: Some("project-a".to_string()),
            category: "Coding".to_string(),
            edited_files: vec![],
            git_commits: vec![],
            git_branch: None,
            browser_urls: vec![],
        },
        ActivitySegment {
            start_time: now - Duration::minutes(30),
            end_time: now,
            project_name: Some("project-a".to_string()),
            category: "Coding".to_string(),
            edited_files: vec![],
            git_commits: vec![],
            git_branch: None,
            browser_urls: vec![],
        },
    ];

    let report = analyzer.generate_daily_summary(date, &segments);

    // project-a should have ~1.5 hours (3600 + 1800 = 5400 seconds)
    let project_a_time = report.project_breakdown.get("project-a").unwrap();
    assert!(*project_a_time >= 5300 && *project_a_time <= 5500);
}

// ==================== DailySummaryReport::format_report tests ====================

#[test]
fn test_format_report_empty() {
    let report = DailySummaryReport {
        date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        total_active_seconds: 0,
        classified_seconds: 0,
        unclassified_seconds: 0,
        project_breakdown: HashMap::new(),
        suggested_blocks: vec![],
    };

    let output = report.format_report();

    assert!(output.contains("2024-01-15 Work Summary"));
    assert!(output.contains("Total active time: 0m"));
    assert!(output.contains("Classified time: 0m (0%)"));
}

#[test]
fn test_format_report_with_data() {
    let mut project_breakdown = HashMap::new();
    project_breakdown.insert("project-a".to_string(), 3600);
    project_breakdown.insert("project-b".to_string(), 1800);

    let report = DailySummaryReport {
        date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        total_active_seconds: 5400,
        classified_seconds: 3600,
        unclassified_seconds: 1800,
        project_breakdown,
        suggested_blocks: vec![],
    };

    let output = report.format_report();

    assert!(output.contains("Total active time: 1h 30m"));
    assert!(output.contains("Classified time: 1h 0m"));
    assert!(output.contains("Project breakdown:"));
    assert!(output.contains("project-a: 1h 0m"));
    assert!(output.contains("project-b: 30m"));
}

#[test]
fn test_format_report_with_blocks() {
    let now = Utc::now();
    let suggested_blocks = vec![
        SuggestedTimeBlock {
            start_time: now - Duration::hours(1),
            end_time: now,
            duration_seconds: 3600,
            suggested_description: "Feature development".to_string(),
            suggested_issues: vec![
                SuggestedIssue { issue_id: "PROJ-123".to_string(), confidence: 0.9, reason: "From branch".to_string() }
            ],
            confidence: 0.9,
            reasoning: vec![],
        },
    ];

    let report = DailySummaryReport {
        date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        total_active_seconds: 3600,
        classified_seconds: 3600,
        unclassified_seconds: 0,
        project_breakdown: HashMap::new(),
        suggested_blocks,
    };

    let output = report.format_report();

    assert!(output.contains("AI suggested time blocks:"));
    assert!(output.contains("Feature development"));
    assert!(output.contains("PROJ-123"));
    assert!(output.contains("Confidence: 90%"));
}

// ==================== SuggestedIssue tests ====================

#[test]
fn test_suggested_issue_clone() {
    let issue = SuggestedIssue {
        issue_id: "PROJ-123".to_string(),
        confidence: 0.85,
        reason: "From branch".to_string(),
    };

    let cloned = issue.clone();

    assert_eq!(issue.issue_id, cloned.issue_id);
    assert!((issue.confidence - cloned.confidence).abs() < 0.001);
    assert_eq!(issue.reason, cloned.reason);
}

// ==================== ActivitySegment tests ====================

#[test]
fn test_activity_segment_clone() {
    let segment = create_test_segment(Some("project"), "Coding", 30, 0);
    let cloned = segment.clone();

    assert_eq!(segment.project_name, cloned.project_name);
    assert_eq!(segment.category, cloned.category);
}

// ==================== SuggestedTimeBlock tests ====================

#[test]
fn test_suggested_time_block_clone() {
    let now = Utc::now();
    let block = SuggestedTimeBlock {
        start_time: now - Duration::hours(1),
        end_time: now,
        duration_seconds: 3600,
        suggested_description: "Test".to_string(),
        suggested_issues: vec![],
        confidence: 0.8,
        reasoning: vec!["reason1".to_string()],
    };

    let cloned = block.clone();

    assert_eq!(block.duration_seconds, cloned.duration_seconds);
    assert_eq!(block.suggested_description, cloned.suggested_description);
    assert!((block.confidence - cloned.confidence).abs() < 0.001);
}
