use super::*;

// ============================================================================
// IssueMatch tests
// ============================================================================

#[test]
fn test_issue_match_clone() {
    let match_result = IssueMatch {
        issue_id: "PROJ-123".to_string(),
        confidence: 0.85,
        match_reasons: vec![MatchReason::BranchName],
    };
    let cloned = match_result.clone();
    assert_eq!(cloned.issue_id, "PROJ-123");
    assert!((cloned.confidence - 0.85).abs() < f32::EPSILON);
}

#[test]
fn test_issue_match_debug() {
    let match_result = IssueMatch {
        issue_id: "TEST-1".to_string(),
        confidence: 0.5,
        match_reasons: vec![],
    };
    let debug_str = format!("{:?}", match_result);
    assert!(debug_str.contains("TEST-1"));
}

// ============================================================================
// MatchReason tests
// ============================================================================

#[test]
fn test_match_reason_commit_message() {
    let reason = MatchReason::CommitMessage("fix: resolve bug".to_string());
    let cloned = reason.clone();
    if let MatchReason::CommitMessage(msg) = cloned {
        assert_eq!(msg, "fix: resolve bug");
    } else {
        panic!("Expected CommitMessage variant");
    }
}

#[test]
fn test_match_reason_branch_name() {
    let reason = MatchReason::BranchName;
    let cloned = reason.clone();
    assert!(matches!(cloned, MatchReason::BranchName));
}

#[test]
fn test_match_reason_browser_url() {
    let reason = MatchReason::BrowserUrl("https://example.com/issue/123".to_string());
    let cloned = reason.clone();
    if let MatchReason::BrowserUrl(url) = cloned {
        assert!(url.contains("example.com"));
    } else {
        panic!("Expected BrowserUrl variant");
    }
}

#[test]
fn test_match_reason_file_path_pattern() {
    let reason = MatchReason::FilePathPattern("src/PROJ-123/main.rs".to_string());
    let cloned = reason.clone();
    if let MatchReason::FilePathPattern(path) = cloned {
        assert!(path.contains("PROJ-123"));
    } else {
        panic!("Expected FilePathPattern variant");
    }
}

#[test]
fn test_match_reason_semantic_similarity() {
    let reason = MatchReason::SemanticSimilarity(0.75);
    let cloned = reason.clone();
    if let MatchReason::SemanticSimilarity(score) = cloned {
        assert!((score - 0.75).abs() < f32::EPSILON);
    } else {
        panic!("Expected SemanticSimilarity variant");
    }
}

#[test]
fn test_match_reason_recently_viewed() {
    let reason = MatchReason::RecentlyViewed;
    assert!(matches!(reason.clone(), MatchReason::RecentlyViewed));
}

#[test]
fn test_match_reason_assigned() {
    let reason = MatchReason::Assigned;
    assert!(matches!(reason.clone(), MatchReason::Assigned));
}

#[test]
fn test_match_reason_debug() {
    let reason = MatchReason::BranchName;
    let debug_str = format!("{:?}", reason);
    assert!(debug_str.contains("BranchName"));
}

// ============================================================================
// ActivitySignals tests
// ============================================================================

#[test]
fn test_activity_signals_default() {
    let signals = ActivitySignals::default();
    assert!(signals.recent_commits.is_empty());
    assert!(signals.edited_files.is_empty());
    assert!(signals.browser_urls.is_empty());
    assert!(signals.window_titles.is_empty());
    assert!(signals.git_branch.is_none());
}

#[test]
fn test_activity_signals_clone() {
    let signals = ActivitySignals {
        recent_commits: vec!["commit 1".to_string()],
        edited_files: vec!["file.rs".to_string()],
        browser_urls: vec!["https://example.com".to_string()],
        window_titles: vec!["VS Code".to_string()],
        git_branch: Some("feature/test".to_string()),
    };
    let cloned = signals.clone();
    assert_eq!(cloned.recent_commits.len(), 1);
    assert_eq!(cloned.git_branch, Some("feature/test".to_string()));
}

#[test]
fn test_activity_signals_debug() {
    let signals = ActivitySignals::default();
    let debug_str = format!("{:?}", signals);
    assert!(debug_str.contains("ActivitySignals"));
}

// ============================================================================
// CandidateIssue tests
// ============================================================================

#[test]
fn test_candidate_issue_clone() {
    let issue = CandidateIssue {
        external_id: "PROJ-42".to_string(),
        title: "Fix authentication".to_string(),
        description: Some("User login fails".to_string()),
        status: "open".to_string(),
        labels: vec!["bug".to_string(), "priority".to_string()],
        is_assigned_to_user: true,
    };
    let cloned = issue.clone();
    assert_eq!(cloned.external_id, "PROJ-42");
    assert_eq!(cloned.title, "Fix authentication");
    assert!(cloned.is_assigned_to_user);
    assert_eq!(cloned.labels.len(), 2);
}

#[test]
fn test_candidate_issue_no_description() {
    let issue = CandidateIssue {
        external_id: "TEST-1".to_string(),
        title: "Simple task".to_string(),
        description: None,
        status: "todo".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    };
    assert!(issue.description.is_none());
}

#[test]
fn test_candidate_issue_debug() {
    let issue = CandidateIssue {
        external_id: "DEBUG-1".to_string(),
        title: "Test".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    };
    let debug_str = format!("{:?}", issue);
    assert!(debug_str.contains("DEBUG-1"));
}

// ============================================================================
// IssueMatcher tests
// ============================================================================

#[test]
fn test_issue_matcher_new() {
    let matcher = IssueMatcher::new();
    // Verify it can extract issue IDs (pattern compiled correctly)
    let ids = matcher.extract_issue_ids("TEST-123");
    assert_eq!(ids, vec!["TEST-123"]);
}

#[test]
fn test_issue_matcher_default() {
    let matcher = IssueMatcher::default();
    let ids = matcher.extract_issue_ids("ABC-1");
    assert_eq!(ids, vec!["ABC-1"]);
}

#[test]
fn test_extract_issue_ids_single() {
    let matcher = IssueMatcher::new();
    assert_eq!(
        matcher.extract_issue_ids("feature/TOKI-9-add-tracking"),
        vec!["TOKI-9"]
    );
}

#[test]
fn test_extract_issue_ids_multiple() {
    let matcher = IssueMatcher::new();
    assert_eq!(
        matcher.extract_issue_ids("Fix ABC-123 and DEF-456"),
        vec!["ABC-123", "DEF-456"]
    );
}

#[test]
fn test_extract_issue_ids_lowercase() {
    let matcher = IssueMatcher::new();
    // Pattern is case-insensitive, output is uppercase
    assert_eq!(
        matcher.extract_issue_ids("fix proj-99 issue"),
        vec!["PROJ-99"]
    );
}

#[test]
fn test_extract_issue_ids_no_match() {
    let matcher = IssueMatcher::new();
    assert!(matcher.extract_issue_ids("no issue ids here").is_empty());
}

#[test]
fn test_extract_issue_ids_short_prefix() {
    let matcher = IssueMatcher::new();
    // Minimum 2-letter prefix
    assert_eq!(matcher.extract_issue_ids("AB-1"), vec!["AB-1"]);
}

#[test]
fn test_extract_issue_ids_long_prefix() {
    let matcher = IssueMatcher::new();
    // Up to 10-letter prefix
    assert_eq!(
        matcher.extract_issue_ids("PROJECTABC-999"),
        vec!["PROJECTABC-999"]
    );
}

#[test]
fn test_extract_issue_ids_long_prefix_partial_match() {
    let matcher = IssueMatcher::new();
    // With 11 letters "PROJECTABCDE", regex matches a 10-letter substring
    // The regex `[A-Z]{2,10}` matches "ROJECTABCD" or similar substrings
    let ids = matcher.extract_issue_ids("PROJECTABCDE-123");
    // Will match "JECTABCDE-123" or similar (regex finds valid 2-10 letter substring)
    assert!(!ids.is_empty());
}

#[test]
fn test_extract_issue_ids_single_letter_prefix() {
    let matcher = IssueMatcher::new();
    // Single letter prefix won't match (minimum 2)
    assert!(matcher.extract_issue_ids("A-123").is_empty());
}

// ============================================================================
// calculate_semantic_similarity tests
// ============================================================================

#[test]
fn test_semantic_similarity_exact_match() {
    let signals = ActivitySignals {
        edited_files: vec!["authentication.rs".to_string()],
        recent_commits: vec!["fix authentication bug".to_string()],
        ..Default::default()
    };
    let candidate = CandidateIssue {
        external_id: "AUTH-1".to_string(),
        title: "Authentication bug".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    };
    let similarity = IssueMatcher::calculate_semantic_similarity(&signals, &candidate);
    assert!(similarity > 0.0);
}

#[test]
fn test_semantic_similarity_no_match() {
    let signals = ActivitySignals {
        edited_files: vec!["database.rs".to_string()],
        recent_commits: vec!["update schema".to_string()],
        ..Default::default()
    };
    let candidate = CandidateIssue {
        external_id: "UI-1".to_string(),
        title: "Button color".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    };
    let similarity = IssueMatcher::calculate_semantic_similarity(&signals, &candidate);
    // Low similarity expected when no keywords match
    assert!(similarity < 0.5);
}

#[test]
fn test_semantic_similarity_with_labels() {
    let signals = ActivitySignals {
        edited_files: vec!["urgent_fix.rs".to_string()],
        recent_commits: vec!["fix critical bug".to_string()],
        window_titles: vec!["urgent".to_string()],
        ..Default::default()
    };
    let candidate = CandidateIssue {
        external_id: "BUG-1".to_string(),
        title: "Critical issue".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec!["urgent".to_string(), "critical".to_string()],
        is_assigned_to_user: false,
    };
    let similarity = IssueMatcher::calculate_semantic_similarity(&signals, &candidate);
    // Labels should boost similarity
    assert!(similarity > 0.0);
}

#[test]
fn test_semantic_similarity_with_description() {
    let signals = ActivitySignals {
        recent_commits: vec!["implement feature".to_string()],
        ..Default::default()
    };
    let candidate = CandidateIssue {
        external_id: "FEAT-1".to_string(),
        title: "New feature".to_string(),
        description: Some("Implement the feature request".to_string()),
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    };
    let similarity = IssueMatcher::calculate_semantic_similarity(&signals, &candidate);
    assert!(similarity > 0.0);
}

#[test]
fn test_semantic_similarity_empty_signals() {
    let signals = ActivitySignals::default();
    let candidate = CandidateIssue {
        external_id: "TEST-1".to_string(),
        title: "Test issue".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    };
    let similarity = IssueMatcher::calculate_semantic_similarity(&signals, &candidate);
    assert!(similarity >= 0.0);
}

// ============================================================================
// find_best_match tests
// ============================================================================

#[test]
fn test_find_best_match_empty_candidates() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals::default();
    let result = matcher.find_best_match(&signals, &[]);
    assert!(result.is_none());
}

#[test]
fn test_find_best_match_branch_name() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals {
        git_branch: Some("feature/TOKI-9-new-feature".to_string()),
        ..Default::default()
    };
    let candidates = vec![
        CandidateIssue {
            external_id: "TOKI-9".to_string(),
            title: "New feature".to_string(),
            description: None,
            status: "in_progress".to_string(),
            labels: vec![],
            is_assigned_to_user: false,
        },
        CandidateIssue {
            external_id: "TOKI-10".to_string(),
            title: "Another issue".to_string(),
            description: None,
            status: "open".to_string(),
            labels: vec![],
            is_assigned_to_user: true,
        },
    ];
    let result = matcher.find_best_match(&signals, &candidates);
    assert!(result.is_some());
    assert_eq!(result.unwrap().issue_id, "TOKI-9");
}

#[test]
fn test_find_best_match_browser_url() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals {
        browser_urls: vec!["https://plane.so/workspace/project/issues/PROJ-42".to_string()],
        ..Default::default()
    };
    let candidates = vec![CandidateIssue {
        external_id: "PROJ-42".to_string(),
        title: "Fix bug".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    }];
    let result = matcher.find_best_match(&signals, &candidates);
    assert!(result.is_some());
    assert_eq!(result.unwrap().issue_id, "PROJ-42");
}

#[test]
fn test_find_best_match_commit_message() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals {
        recent_commits: vec!["fix: TASK-100 resolve login issue".to_string()],
        ..Default::default()
    };
    let candidates = vec![CandidateIssue {
        external_id: "TASK-100".to_string(),
        title: "Login issue".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    }];
    let result = matcher.find_best_match(&signals, &candidates);
    assert!(result.is_some());
    assert_eq!(result.unwrap().issue_id, "TASK-100");
}

#[test]
fn test_find_best_match_file_path() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals {
        edited_files: vec!["src/features/FEAT-55/component.tsx".to_string()],
        ..Default::default()
    };
    let candidates = vec![CandidateIssue {
        external_id: "FEAT-55".to_string(),
        title: "New component".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    }];
    let result = matcher.find_best_match(&signals, &candidates);
    assert!(result.is_some());
    assert_eq!(result.unwrap().issue_id, "FEAT-55");
}

#[test]
fn test_find_best_match_window_title() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals {
        window_titles: vec!["JIRA-999 - Bug Report".to_string()],
        ..Default::default()
    };
    let candidates = vec![CandidateIssue {
        external_id: "JIRA-999".to_string(),
        title: "Bug Report".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    }];
    let result = matcher.find_best_match(&signals, &candidates);
    assert!(result.is_some());
    assert_eq!(result.unwrap().issue_id, "JIRA-999");
}

#[test]
fn test_find_best_match_assigned_boost() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals::default();
    let candidates = vec![
        CandidateIssue {
            external_id: "TASK-1".to_string(),
            title: "Unassigned".to_string(),
            description: None,
            status: "open".to_string(),
            labels: vec![],
            is_assigned_to_user: false,
        },
        CandidateIssue {
            external_id: "TASK-2".to_string(),
            title: "Assigned".to_string(),
            description: None,
            status: "open".to_string(),
            labels: vec![],
            is_assigned_to_user: true,
        },
    ];
    let result = matcher.find_best_match(&signals, &candidates);
    // Assigned issue should win due to boost
    assert!(result.is_some());
    assert_eq!(result.unwrap().issue_id, "TASK-2");
}

#[test]
fn test_find_best_match_combined_signals() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals {
        git_branch: Some("feature/PROJ-1".to_string()),
        recent_commits: vec!["PROJ-1: add feature".to_string()],
        browser_urls: vec!["https://example.com/PROJ-1".to_string()],
        ..Default::default()
    };
    let candidates = vec![CandidateIssue {
        external_id: "PROJ-1".to_string(),
        title: "Feature".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    }];
    let result = matcher.find_best_match(&signals, &candidates);
    assert!(result.is_some());
    let match_result = result.unwrap();
    assert_eq!(match_result.issue_id, "PROJ-1");
    // Should have high confidence due to multiple signals
    assert!(match_result.confidence > 0.9);
}

#[test]
fn test_find_best_match_confidence_capped() {
    let matcher = IssueMatcher::new();
    // Create signals that would sum to > 1.0
    let signals = ActivitySignals {
        git_branch: Some("feature/TEST-1".to_string()),
        recent_commits: vec!["TEST-1: commit".to_string()],
        browser_urls: vec!["https://example.com/TEST-1".to_string()],
        edited_files: vec!["TEST-1/file.rs".to_string()],
        window_titles: vec!["TEST-1 - Window".to_string()],
    };
    let candidates = vec![CandidateIssue {
        external_id: "TEST-1".to_string(),
        title: "Test".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: true,
    }];
    let result = matcher.find_best_match(&signals, &candidates);
    assert!(result.is_some());
    // Confidence should be capped at 1.0
    assert!(result.unwrap().confidence <= 1.0);
}

#[test]
fn test_find_best_match_no_matching_signals() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals {
        git_branch: Some("main".to_string()),
        recent_commits: vec!["update readme".to_string()],
        ..Default::default()
    };
    let candidates = vec![CandidateIssue {
        external_id: "PROJ-99".to_string(),
        title: "Unrelated".to_string(),
        description: None,
        status: "open".to_string(),
        labels: vec![],
        is_assigned_to_user: false,
    }];
    let result = matcher.find_best_match(&signals, &candidates);
    // May return None or low confidence match depending on semantic similarity
    if let Some(m) = result {
        assert!(m.confidence < 0.5);
    }
}

// ============================================================================
// suggest_issues tests
// ============================================================================

#[test]
fn test_suggest_issues_empty_candidates() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals::default();
    let suggestions = matcher.suggest_issues(&signals, &[], 5);
    assert!(suggestions.is_empty());
}

#[test]
fn test_suggest_issues_top_n() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals {
        git_branch: Some("feature/PROJ-1".to_string()),
        browser_urls: vec!["https://example.com/PROJ-2".to_string()],
        recent_commits: vec!["fix PROJ-3".to_string()],
        ..Default::default()
    };
    let candidates = vec![
        CandidateIssue {
            external_id: "PROJ-1".to_string(),
            title: "Issue 1".to_string(),
            description: None,
            status: "open".to_string(),
            labels: vec![],
            is_assigned_to_user: false,
        },
        CandidateIssue {
            external_id: "PROJ-2".to_string(),
            title: "Issue 2".to_string(),
            description: None,
            status: "open".to_string(),
            labels: vec![],
            is_assigned_to_user: false,
        },
        CandidateIssue {
            external_id: "PROJ-3".to_string(),
            title: "Issue 3".to_string(),
            description: None,
            status: "open".to_string(),
            labels: vec![],
            is_assigned_to_user: false,
        },
    ];
    let suggestions = matcher.suggest_issues(&signals, &candidates, 2);
    assert_eq!(suggestions.len(), 2);
    // First should be highest score (branch name = 0.9)
    assert_eq!(suggestions[0].issue_id, "PROJ-1");
}

#[test]
fn test_suggest_issues_assigned_not_done() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals::default();
    let candidates = vec![
        CandidateIssue {
            external_id: "TASK-1".to_string(),
            title: "Done task".to_string(),
            description: None,
            status: "done".to_string(),
            labels: vec![],
            is_assigned_to_user: true,
        },
        CandidateIssue {
            external_id: "TASK-2".to_string(),
            title: "Active task".to_string(),
            description: None,
            status: "in_progress".to_string(),
            labels: vec![],
            is_assigned_to_user: true,
        },
    ];
    let suggestions = matcher.suggest_issues(&signals, &candidates, 5);
    // TASK-2 should be suggested (assigned + not done)
    // TASK-1 should not get assigned boost (status = done)
    assert!(!suggestions.is_empty());
    if suggestions.len() >= 2 {
        assert_eq!(suggestions[0].issue_id, "TASK-2");
    }
}

#[test]
fn test_suggest_issues_sorted_by_score() {
    let matcher = IssueMatcher::new();
    let signals = ActivitySignals {
        git_branch: Some("feature/HIGH-1".to_string()),
        recent_commits: vec!["fix MED-2".to_string()],
        ..Default::default()
    };
    let candidates = vec![
        CandidateIssue {
            external_id: "HIGH-1".to_string(),
            title: "High priority".to_string(),
            description: None,
            status: "open".to_string(),
            labels: vec![],
            is_assigned_to_user: false,
        },
        CandidateIssue {
            external_id: "MED-2".to_string(),
            title: "Medium priority".to_string(),
            description: None,
            status: "open".to_string(),
            labels: vec![],
            is_assigned_to_user: false,
        },
    ];
    let suggestions = matcher.suggest_issues(&signals, &candidates, 5);
    assert!(suggestions.len() >= 2);
    // Branch match (0.9) > commit match (0.7)
    assert_eq!(suggestions[0].issue_id, "HIGH-1");
    assert_eq!(suggestions[1].issue_id, "MED-2");
}

// ============================================================================
// SmartIssueMatcher tests
// ============================================================================

#[test]
fn test_generate_context_text_with_branch() {
    let signals = ActivitySignals {
        git_branch: Some("feature/PROJ-123".to_string()),
        ..Default::default()
    };
    let text = SmartIssueMatcher::generate_context_text(&signals);
    assert!(text.contains("Branch: feature/PROJ-123"));
}

#[test]
fn test_generate_context_text_with_commits() {
    let signals = ActivitySignals {
        recent_commits: vec![
            "commit 1".to_string(),
            "commit 2".to_string(),
            "commit 3".to_string(),
            "commit 4".to_string(), // Should be truncated
        ],
        ..Default::default()
    };
    let text = SmartIssueMatcher::generate_context_text(&signals);
    assert!(text.contains("Commit: commit 1"));
    assert!(text.contains("Commit: commit 2"));
    assert!(text.contains("Commit: commit 3"));
    assert!(!text.contains("commit 4")); // Only first 3
}

#[test]
fn test_generate_context_text_with_files() {
    let signals = ActivitySignals {
        edited_files: vec![
            "/path/to/file1.rs".to_string(),
            "/path/to/file2.rs".to_string(),
        ],
        ..Default::default()
    };
    let text = SmartIssueMatcher::generate_context_text(&signals);
    assert!(text.contains("File: file1.rs"));
    assert!(text.contains("File: file2.rs"));
}

#[test]
fn test_generate_context_text_with_urls() {
    let signals = ActivitySignals {
        browser_urls: vec![
            // Path segment must be > 3 chars to be included
            "https://github.com/org/repo/issues/PROJ-1234".to_string(),
        ],
        ..Default::default()
    };
    let text = SmartIssueMatcher::generate_context_text(&signals);
    // Should extract last path segment (PROJ-1234 has 9 chars > 3)
    assert!(text.contains("URL: PROJ-1234"));
}

#[test]
fn test_generate_context_text_with_window_titles() {
    let signals = ActivitySignals {
        window_titles: vec![
            "VS Code - project".to_string(),
            "Chrome - GitHub".to_string(),
        ],
        ..Default::default()
    };
    let text = SmartIssueMatcher::generate_context_text(&signals);
    assert!(text.contains("Activity: VS Code - project"));
    assert!(text.contains("Activity: Chrome - GitHub"));
}

#[test]
fn test_generate_context_text_filters_untitled() {
    let signals = ActivitySignals {
        window_titles: vec![
            "Untitled".to_string(),
            "Untitled-1".to_string(),
            "Real Title".to_string(),
        ],
        ..Default::default()
    };
    let text = SmartIssueMatcher::generate_context_text(&signals);
    assert!(!text.contains("Untitled"));
    assert!(text.contains("Real Title"));
}

#[test]
fn test_generate_context_text_empty_signals() {
    let signals = ActivitySignals::default();
    let text = SmartIssueMatcher::generate_context_text(&signals);
    // Should have fallback text
    assert!(text.contains("Software development work"));
}

#[test]
fn test_generate_context_text_combined() {
    let signals = ActivitySignals {
        git_branch: Some("main".to_string()),
        recent_commits: vec!["initial commit".to_string()],
        edited_files: vec!["/src/main.rs".to_string()],
        browser_urls: vec!["https://docs.rs/something".to_string()],
        window_titles: vec!["Terminal".to_string()],
    };
    let text = SmartIssueMatcher::generate_context_text(&signals);
    assert!(text.contains("Branch: main"));
    assert!(text.contains("Commit: initial commit"));
    assert!(text.contains("File: main.rs"));
    assert!(text.contains("Activity: Terminal"));
}

// ============================================================================
// format_reasons tests
// ============================================================================

#[test]
fn test_format_reasons_empty() {
    let reasons: Vec<MatchReason> = vec![];
    let formatted = SmartIssueMatcher::format_reasons(&reasons);
    assert!(formatted.is_empty());
}

#[test]
fn test_format_reasons_branch_name() {
    let reasons = vec![MatchReason::BranchName];
    let formatted = SmartIssueMatcher::format_reasons(&reasons);
    assert_eq!(formatted, "Git branch");
}

#[test]
fn test_format_reasons_commit_message() {
    let reasons = vec![MatchReason::CommitMessage("fix bug".to_string())];
    let formatted = SmartIssueMatcher::format_reasons(&reasons);
    assert_eq!(formatted, "Commit message");
}

#[test]
fn test_format_reasons_browser_url() {
    let reasons = vec![MatchReason::BrowserUrl("https://example.com".to_string())];
    let formatted = SmartIssueMatcher::format_reasons(&reasons);
    assert_eq!(formatted, "Browser URL");
}

#[test]
fn test_format_reasons_file_path() {
    let reasons = vec![MatchReason::FilePathPattern("src/main.rs".to_string())];
    let formatted = SmartIssueMatcher::format_reasons(&reasons);
    assert_eq!(formatted, "File path");
}

#[test]
fn test_format_reasons_semantic_similarity() {
    let reasons = vec![MatchReason::SemanticSimilarity(0.75)];
    let formatted = SmartIssueMatcher::format_reasons(&reasons);
    assert_eq!(formatted, "Semantic (75%)");
}

#[test]
fn test_format_reasons_recently_viewed() {
    let reasons = vec![MatchReason::RecentlyViewed];
    let formatted = SmartIssueMatcher::format_reasons(&reasons);
    assert_eq!(formatted, "Recently viewed");
}

#[test]
fn test_format_reasons_assigned() {
    let reasons = vec![MatchReason::Assigned];
    let formatted = SmartIssueMatcher::format_reasons(&reasons);
    assert_eq!(formatted, "Assigned");
}

#[test]
fn test_format_reasons_multiple() {
    let reasons = vec![
        MatchReason::BranchName,
        MatchReason::CommitMessage("msg".to_string()),
        MatchReason::Assigned,
    ];
    let formatted = SmartIssueMatcher::format_reasons(&reasons);
    assert_eq!(formatted, "Git branch, Commit message, Assigned");
}

#[test]
fn test_format_reasons_all_types() {
    let reasons = vec![
        MatchReason::BranchName,
        MatchReason::CommitMessage("msg".to_string()),
        MatchReason::BrowserUrl("url".to_string()),
        MatchReason::FilePathPattern("path".to_string()),
        MatchReason::SemanticSimilarity(0.5),
        MatchReason::RecentlyViewed,
        MatchReason::Assigned,
    ];
    let formatted = SmartIssueMatcher::format_reasons(&reasons);
    assert!(formatted.contains("Git branch"));
    assert!(formatted.contains("Commit message"));
    assert!(formatted.contains("Browser URL"));
    assert!(formatted.contains("File path"));
    assert!(formatted.contains("Semantic (50%)"));
    assert!(formatted.contains("Recently viewed"));
    assert!(formatted.contains("Assigned"));
}
