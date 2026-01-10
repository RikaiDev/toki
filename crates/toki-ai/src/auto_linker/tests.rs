use super::*;

// ============================================================================
// truncate function tests
// ============================================================================

#[test]
fn test_truncate_shorter_than_max() {
    let result = truncate("hello", 10);
    assert_eq!(result, "hello");
}

#[test]
fn test_truncate_exact_length() {
    let result = truncate("hello", 5);
    assert_eq!(result, "hello");
}

#[test]
fn test_truncate_longer_than_max() {
    let result = truncate("hello world", 5);
    assert_eq!(result, "hello...");
}

#[test]
fn test_truncate_empty_string() {
    let result = truncate("", 10);
    assert_eq!(result, "");
}

#[test]
fn test_truncate_zero_max() {
    let result = truncate("hello", 0);
    assert_eq!(result, "...");
}

#[test]
fn test_truncate_unicode() {
    // Note: truncate works on byte length, not char count
    let result = truncate("你好世界", 6);
    // UTF-8: each Chinese char is 3 bytes, so 6 bytes = 2 chars
    assert!(result.ends_with("..."));
}

#[test]
fn test_truncate_one_char_max() {
    let result = truncate("hello", 1);
    assert_eq!(result, "h...");
}

// ============================================================================
// LinkReason Display tests
// ============================================================================

#[test]
fn test_link_reason_display_browser_url_short() {
    let reason = LinkReason::BrowserUrl("https://example.com".to_string());
    let display = format!("{}", reason);
    assert_eq!(display, "Browser URL: https://example.com");
}

#[test]
fn test_link_reason_display_browser_url_long() {
    let long_url = "https://plane.so/workspace/projects/very-long-project-name-that-exceeds-forty-characters";
    let reason = LinkReason::BrowserUrl(long_url.to_string());
    let display = format!("{}", reason);
    assert!(display.starts_with("Browser URL: "));
    assert!(display.ends_with("..."));
}

#[test]
fn test_link_reason_display_exact_match() {
    let reason = LinkReason::ExactNameMatch;
    let display = format!("{}", reason);
    assert_eq!(display, "Exact name match");
}

#[test]
fn test_link_reason_display_fuzzy_match() {
    let reason = LinkReason::FuzzyNameMatch(0.85);
    let display = format!("{}", reason);
    assert_eq!(display, "Name similarity: 85%");
}

#[test]
fn test_link_reason_display_fuzzy_match_low() {
    let reason = LinkReason::FuzzyNameMatch(0.123);
    let display = format!("{}", reason);
    assert_eq!(display, "Name similarity: 12%");
}

#[test]
fn test_link_reason_display_git_remote_short() {
    let reason = LinkReason::GitRemote("git@github.com:org/repo.git".to_string());
    let display = format!("{}", reason);
    assert_eq!(display, "Git remote: git@github.com:org/repo.git");
}

#[test]
fn test_link_reason_display_git_remote_long() {
    let long_remote = "git@github.com:organization-name/very-long-repository-name-here.git";
    let reason = LinkReason::GitRemote(long_remote.to_string());
    let display = format!("{}", reason);
    assert!(display.starts_with("Git remote: "));
    assert!(display.ends_with("..."));
}

#[test]
fn test_link_reason_display_issue_page_visit() {
    let reason = LinkReason::IssuePageVisit("PROJ-123".to_string());
    let display = format!("{}", reason);
    assert_eq!(display, "Visited issue: PROJ-123");
}

// ============================================================================
// LinkReason Clone and Debug tests
// ============================================================================

#[test]
fn test_link_reason_clone_browser_url() {
    let reason = LinkReason::BrowserUrl("https://example.com".to_string());
    let cloned = reason.clone();
    if let LinkReason::BrowserUrl(url) = cloned {
        assert_eq!(url, "https://example.com");
    } else {
        panic!("Expected BrowserUrl variant");
    }
}

#[test]
fn test_link_reason_clone_exact_match() {
    let reason = LinkReason::ExactNameMatch;
    let cloned = reason.clone();
    assert!(matches!(cloned, LinkReason::ExactNameMatch));
}

#[test]
fn test_link_reason_clone_fuzzy_match() {
    let reason = LinkReason::FuzzyNameMatch(0.75);
    let cloned = reason.clone();
    if let LinkReason::FuzzyNameMatch(score) = cloned {
        assert!((score - 0.75).abs() < f32::EPSILON);
    } else {
        panic!("Expected FuzzyNameMatch variant");
    }
}

#[test]
fn test_link_reason_debug() {
    let reason = LinkReason::ExactNameMatch;
    let debug = format!("{:?}", reason);
    assert!(debug.contains("ExactNameMatch"));
}

// ============================================================================
// LinkSuggestion tests
// ============================================================================

#[test]
fn test_link_suggestion_clone() {
    let suggestion = LinkSuggestion {
        local_project_id: Uuid::new_v4(),
        local_project_name: "my-project".to_string(),
        pm_project_id: "pm-123".to_string(),
        pm_project_identifier: "PROJ".to_string(),
        pm_project_name: "Project Name".to_string(),
        confidence: 0.95,
        reason: LinkReason::ExactNameMatch,
    };
    let cloned = suggestion.clone();
    assert_eq!(cloned.local_project_name, "my-project");
    assert_eq!(cloned.pm_project_id, "pm-123");
    assert!((cloned.confidence - 0.95).abs() < f32::EPSILON);
}

#[test]
fn test_link_suggestion_debug() {
    let suggestion = LinkSuggestion {
        local_project_id: Uuid::new_v4(),
        local_project_name: "test".to_string(),
        pm_project_id: "id".to_string(),
        pm_project_identifier: "TEST".to_string(),
        pm_project_name: "Test".to_string(),
        confidence: 0.5,
        reason: LinkReason::ExactNameMatch,
    };
    let debug = format!("{:?}", suggestion);
    assert!(debug.contains("LinkSuggestion"));
    assert!(debug.contains("test"));
}

// ============================================================================
// calculate_name_similarity tests
// ============================================================================

#[test]
fn test_calculate_name_similarity_identical() {
    let result = AutoLinker::calculate_name_similarity("hygieia", "hygieia");
    assert!((result - 1.0).abs() < 0.01);
}

#[test]
fn test_calculate_name_similarity_one_char_diff() {
    let result = AutoLinker::calculate_name_similarity("hygieia", "hygeia");
    assert!(result > 0.8);
}

#[test]
fn test_calculate_name_similarity_similar_projects() {
    let result = AutoLinker::calculate_name_similarity("project-a", "project-b");
    assert!(result > 0.7);
}

#[test]
fn test_calculate_name_similarity_completely_different() {
    let result = AutoLinker::calculate_name_similarity("abc", "xyz");
    assert!(result < 0.3);
}

#[test]
fn test_calculate_name_similarity_empty_strings() {
    let result = AutoLinker::calculate_name_similarity("", "");
    // Empty strings are equal, so returns 1.0 from the early return
    assert!((result - 1.0).abs() < f32::EPSILON);
}

#[test]
fn test_calculate_name_similarity_one_empty() {
    let result = AutoLinker::calculate_name_similarity("hello", "");
    assert!(result < 0.1);
}

#[test]
fn test_calculate_name_similarity_subset() {
    // "toki" is a subset of "toki-app"
    let result = AutoLinker::calculate_name_similarity("toki", "toki-app");
    assert!(result > 0.4);
}

#[test]
fn test_calculate_name_similarity_case_sensitive() {
    // Function is case-sensitive (caller should lowercase if needed)
    let result = AutoLinker::calculate_name_similarity("Hello", "hello");
    // H and h are different chars
    assert!(result < 1.0);
}

#[test]
fn test_calculate_name_similarity_with_numbers() {
    let result = AutoLinker::calculate_name_similarity("project1", "project2");
    // "project1" and "project2" share 7 chars (p,r,o,j,e,c,t), union is 9
    // similarity = 7/9 ≈ 0.78
    assert!(result > 0.7);
}

#[test]
fn test_calculate_name_similarity_with_special_chars() {
    let result = AutoLinker::calculate_name_similarity("my-project", "my_project");
    assert!(result > 0.8);
}

// ============================================================================
// extract_remote_url tests
// ============================================================================

#[test]
fn test_extract_remote_url_simple() {
    let config = r#"
[remote "origin"]
    url = git@github.com:org/project.git
    fetch = +refs/heads/*:refs/remotes/origin/*
"#;
    let result = AutoLinker::extract_remote_url(config);
    assert_eq!(result, Some("git@github.com:org/project.git".to_string()));
}

#[test]
fn test_extract_remote_url_https() {
    let config = r#"
[remote "origin"]
    url = https://github.com/org/project.git
"#;
    let result = AutoLinker::extract_remote_url(config);
    assert_eq!(result, Some("https://github.com/org/project.git".to_string()));
}

#[test]
fn test_extract_remote_url_with_spaces() {
    let config = r#"
[remote "origin"]
    url   =   git@github.com:org/project.git
"#;
    let result = AutoLinker::extract_remote_url(config);
    assert_eq!(result, Some("git@github.com:org/project.git".to_string()));
}

#[test]
fn test_extract_remote_url_multiple_remotes() {
    let config = r#"
[remote "origin"]
    url = git@github.com:org/project.git
[remote "upstream"]
    url = git@github.com:upstream/project.git
"#;
    let result = AutoLinker::extract_remote_url(config);
    // Should return the first one
    assert_eq!(result, Some("git@github.com:org/project.git".to_string()));
}

#[test]
fn test_extract_remote_url_no_url() {
    let config = r#"
[core]
    repositoryformatversion = 0
[branch "main"]
    remote = origin
"#;
    let result = AutoLinker::extract_remote_url(config);
    assert!(result.is_none());
}

#[test]
fn test_extract_remote_url_empty_config() {
    let result = AutoLinker::extract_remote_url("");
    assert!(result.is_none());
}

#[test]
fn test_extract_remote_url_gitlab() {
    let config = r#"
[remote "origin"]
    url = git@gitlab.com:group/project.git
"#;
    let result = AutoLinker::extract_remote_url(config);
    assert_eq!(result, Some("git@gitlab.com:group/project.git".to_string()));
}

// ============================================================================
// extract_project_from_git_url tests
// ============================================================================

#[test]
fn test_extract_project_from_git_url_ssh() {
    let result = AutoLinker::extract_project_from_git_url("git@github.com:org/my-project.git");
    assert_eq!(result, Some("my-project".to_string()));
}

#[test]
fn test_extract_project_from_git_url_https_with_git() {
    let result = AutoLinker::extract_project_from_git_url("https://github.com/org/my-project.git");
    assert_eq!(result, Some("my-project".to_string()));
}

#[test]
fn test_extract_project_from_git_url_https_without_git() {
    let result = AutoLinker::extract_project_from_git_url("https://github.com/org/my-project");
    assert_eq!(result, Some("my-project".to_string()));
}

#[test]
fn test_extract_project_from_git_url_gitlab_ssh() {
    let result = AutoLinker::extract_project_from_git_url("git@gitlab.com:group/subgroup/project.git");
    assert_eq!(result, Some("project".to_string()));
}

#[test]
fn test_extract_project_from_git_url_nested_path() {
    let result = AutoLinker::extract_project_from_git_url("https://github.com/org/sub/project.git");
    assert_eq!(result, Some("project".to_string()));
}

#[test]
fn test_extract_project_from_git_url_simple_name() {
    let result = AutoLinker::extract_project_from_git_url("https://github.com/owner/repo");
    assert_eq!(result, Some("repo".to_string()));
}

#[test]
fn test_extract_project_from_git_url_with_dashes() {
    let result = AutoLinker::extract_project_from_git_url("git@github.com:org/my-awesome-project.git");
    assert_eq!(result, Some("my-awesome-project".to_string()));
}

#[test]
fn test_extract_project_from_git_url_with_underscores() {
    let result = AutoLinker::extract_project_from_git_url("git@github.com:org/my_project.git");
    assert_eq!(result, Some("my_project".to_string()));
}

#[test]
fn test_extract_project_from_git_url_bitbucket() {
    let result = AutoLinker::extract_project_from_git_url("git@bitbucket.org:team/project.git");
    assert_eq!(result, Some("project".to_string()));
}

#[test]
fn test_extract_project_from_git_url_self_hosted() {
    let result = AutoLinker::extract_project_from_git_url("git@git.company.com:team/internal-tool.git");
    assert_eq!(result, Some("internal-tool".to_string()));
}

#[test]
fn test_extract_project_from_git_url_empty() {
    let result = AutoLinker::extract_project_from_git_url("");
    assert_eq!(result, Some("".to_string()));
}

#[test]
fn test_extract_project_from_git_url_just_project() {
    let result = AutoLinker::extract_project_from_git_url("project.git");
    assert_eq!(result, Some("project".to_string()));
}

// ============================================================================
// Integration-style tests for pure functions
// ============================================================================

#[test]
fn test_similarity_and_truncate_together() {
    // Test that similar names with long URLs work correctly
    let name1 = "my-awesome-long-project-name";
    let name2 = "my-awesome-long-project";
    let similarity = AutoLinker::calculate_name_similarity(name1, name2);
    assert!(similarity > 0.8);

    let url = format!("https://github.com/org/{}.git", name1);
    let truncated = truncate(&url, 40);
    assert!(truncated.ends_with("..."));
}

#[test]
fn test_extract_and_similarity() {
    // Extract project name and check similarity
    let url = "git@github.com:org/toki-tracker.git";
    let extracted = AutoLinker::extract_project_from_git_url(url).unwrap();
    let similarity = AutoLinker::calculate_name_similarity(&extracted, "toki-tracker");
    assert!((similarity - 1.0).abs() < 0.01);
}

#[test]
fn test_config_to_project_name() {
    // Full flow: config -> remote url -> project name
    let config = r#"
[remote "origin"]
    url = git@github.com:myorg/awesome-app.git
"#;
    let remote_url = AutoLinker::extract_remote_url(config).unwrap();
    let project_name = AutoLinker::extract_project_from_git_url(&remote_url).unwrap();
    assert_eq!(project_name, "awesome-app");
}
