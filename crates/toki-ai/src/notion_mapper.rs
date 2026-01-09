//! Notion page to GitHub/GitLab issue mapper
//!
//! Converts Notion database pages to issue creation requests for
//! GitHub or GitLab issue trackers.

use toki_integrations::notion::{NotionIssueCandidateData, PropertyMapping, NotionPage, NotionClient};
use toki_integrations::traits::CreateIssueRequest;

/// Configuration for issue mapping behavior
#[derive(Debug, Clone, Default)]
pub struct IssueMappingConfig {
    /// Labels to add to all synced issues
    pub default_labels: Vec<String>,
    /// Prefix for issue titles (e.g., "[Notion]")
    pub title_prefix: Option<String>,
    /// Whether to include Notion page link in issue body
    pub include_notion_link: bool,
    /// Status values that should be synced (empty = all)
    pub sync_statuses: Vec<String>,
    /// Status values that should NOT be synced
    pub exclude_statuses: Vec<String>,
    /// Whether to map Notion labels to issue labels
    pub map_labels: bool,
}

impl IssueMappingConfig {
    /// Create a new config with sensible defaults
    #[must_use]
    pub fn new() -> Self {
        Self {
            default_labels: vec!["notion-sync".to_string()],
            title_prefix: None,
            include_notion_link: true,
            sync_statuses: Vec::new(),
            exclude_statuses: vec!["Done".to_string(), "Completed".to_string()],
            map_labels: true,
        }
    }

    /// Builder: set default labels
    #[must_use]
    pub fn with_default_labels(mut self, labels: Vec<String>) -> Self {
        self.default_labels = labels;
        self
    }

    /// Builder: set title prefix
    #[must_use]
    pub fn with_title_prefix(mut self, prefix: String) -> Self {
        self.title_prefix = Some(prefix);
        self
    }

    /// Builder: set statuses to sync
    #[must_use]
    pub fn with_sync_statuses(mut self, statuses: Vec<String>) -> Self {
        self.sync_statuses = statuses;
        self
    }

    /// Builder: set statuses to exclude
    #[must_use]
    pub fn with_exclude_statuses(mut self, statuses: Vec<String>) -> Self {
        self.exclude_statuses = statuses;
        self
    }
}

/// Mapper for converting Notion pages to GitHub/GitLab issues
pub struct NotionIssueMapper {
    config: IssueMappingConfig,
}

impl Default for NotionIssueMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl NotionIssueMapper {
    /// Create a new mapper with default config
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: IssueMappingConfig::new(),
        }
    }

    /// Create a mapper with custom config
    #[must_use]
    pub fn with_config(config: IssueMappingConfig) -> Self {
        Self { config }
    }

    /// Check if a Notion page should be synced based on its status
    #[must_use]
    pub fn should_sync(&self, status: &str) -> bool {
        // If sync_statuses is specified, only sync those
        if !self.config.sync_statuses.is_empty() {
            return self.config.sync_statuses.iter().any(|s| s.eq_ignore_ascii_case(status));
        }

        // Otherwise, sync everything except excluded statuses
        !self.config.exclude_statuses.iter().any(|s| s.eq_ignore_ascii_case(status))
    }

    /// Map a Notion issue candidate to a create issue request
    #[must_use]
    pub fn map_to_issue_request(&self, candidate: &NotionIssueCandidateData) -> CreateIssueRequest {
        // Build title with optional prefix
        let title = if let Some(ref prefix) = self.config.title_prefix {
            format!("{} {}", prefix, candidate.title)
        } else {
            candidate.title.clone()
        };

        // Build body with description and metadata
        let body = self.build_issue_body(candidate);

        // Collect labels
        let mut labels = self.config.default_labels.clone();
        if self.config.map_labels {
            labels.extend(candidate.labels.clone());
        }

        CreateIssueRequest::new(title)
            .with_body(body)
            .with_labels(labels)
            .with_source(candidate.page_id.clone(), "notion".to_string())
    }

    /// Build the issue body with description and metadata
    fn build_issue_body(&self, candidate: &NotionIssueCandidateData) -> String {
        let mut parts = Vec::new();

        // Add description if present
        if let Some(ref desc) = candidate.description {
            if !desc.is_empty() {
                parts.push(desc.clone());
            }
        }

        // Add metadata section
        let mut metadata = Vec::new();
        metadata.push(format!("**Status:** {}", candidate.status));

        if self.config.include_notion_link {
            // Notion page URL format
            let page_id_clean = candidate.page_id.replace('-', "");
            let notion_url = format!("https://notion.so/{page_id_clean}");
            metadata.push(format!("**Notion:** [Open in Notion]({notion_url})"));
        }

        if !metadata.is_empty() {
            parts.push(String::new()); // Empty line separator
            parts.push("---".to_string());
            parts.extend(metadata);
        }

        parts.join("\n")
    }

    /// Convert a Notion page directly to issue request using property mapping
    #[must_use]
    pub fn map_page_to_issue_request(
        &self,
        page: &NotionPage,
        database_id: &str,
        mapping: &PropertyMapping,
    ) -> CreateIssueRequest {
        // Use NotionClient's helper to convert to candidate first
        let candidate = NotionClient::page_to_issue_candidate(page, database_id, mapping, None);
        self.map_to_issue_request(&candidate)
    }

    /// Filter and map multiple candidates
    #[must_use] pub fn map_candidates(&self, candidates: &[NotionIssueCandidateData]) -> Vec<(NotionIssueCandidateData, CreateIssueRequest)> {
        candidates
            .iter()
            .filter(|c| self.should_sync(&c.status))
            .map(|c| (c.clone(), self.map_to_issue_request(c)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_candidate() -> NotionIssueCandidateData {
        NotionIssueCandidateData {
            external_id: "abcd-12345678".to_string(),
            external_system: "notion".to_string(),
            title: "Fix login bug".to_string(),
            description: Some("Users cannot login with SSO".to_string()),
            status: "In Progress".to_string(),
            database_id: "db-123".to_string(),
            page_id: "page-456".to_string(),
            labels: vec!["bug".to_string(), "urgent".to_string()],
        }
    }

    #[test]
    fn test_map_to_issue_request_basic() {
        let mapper = NotionIssueMapper::new();
        let candidate = create_test_candidate();

        let request = mapper.map_to_issue_request(&candidate);

        assert_eq!(request.title, "Fix login bug");
        assert!(request.body.is_some());
        assert!(request.labels.contains(&"notion-sync".to_string()));
        assert!(request.labels.contains(&"bug".to_string()));
        assert!(request.labels.contains(&"urgent".to_string()));
        assert_eq!(request.source_id, Some("page-456".to_string()));
        assert_eq!(request.source_system, Some("notion".to_string()));
    }

    #[test]
    fn test_map_with_title_prefix() {
        let config = IssueMappingConfig::new().with_title_prefix("[Notion]".to_string());
        let mapper = NotionIssueMapper::with_config(config);
        let candidate = create_test_candidate();

        let request = mapper.map_to_issue_request(&candidate);

        assert_eq!(request.title, "[Notion] Fix login bug");
    }

    #[test]
    fn test_should_sync_default_excludes_done() {
        let mapper = NotionIssueMapper::new();

        assert!(mapper.should_sync("In Progress"));
        assert!(mapper.should_sync("Backlog"));
        assert!(mapper.should_sync("To Do"));
        assert!(!mapper.should_sync("Done"));
        assert!(!mapper.should_sync("Completed"));
        assert!(!mapper.should_sync("done")); // Case insensitive
    }

    #[test]
    fn test_should_sync_with_specific_statuses() {
        let config = IssueMappingConfig::new()
            .with_sync_statuses(vec!["In Progress".to_string(), "Backlog".to_string()]);
        let mapper = NotionIssueMapper::with_config(config);

        assert!(mapper.should_sync("In Progress"));
        assert!(mapper.should_sync("Backlog"));
        assert!(mapper.should_sync("in progress")); // Case insensitive
        assert!(!mapper.should_sync("To Do"));
        assert!(!mapper.should_sync("Done"));
    }

    #[test]
    fn test_issue_body_contains_metadata() {
        let mapper = NotionIssueMapper::new();
        let candidate = create_test_candidate();

        let request = mapper.map_to_issue_request(&candidate);
        let body = request.body.unwrap();

        assert!(body.contains("Users cannot login with SSO"));
        assert!(body.contains("**Status:** In Progress"));
        assert!(body.contains("notion.so"));
    }

    #[test]
    fn test_map_candidates_filters_by_status() {
        let mapper = NotionIssueMapper::new();

        let candidates = vec![
            NotionIssueCandidateData {
                external_id: "1".to_string(),
                external_system: "notion".to_string(),
                title: "Task 1".to_string(),
                description: None,
                status: "In Progress".to_string(),
                database_id: "db".to_string(),
                page_id: "p1".to_string(),
                labels: vec![],
            },
            NotionIssueCandidateData {
                external_id: "2".to_string(),
                external_system: "notion".to_string(),
                title: "Task 2".to_string(),
                description: None,
                status: "Done".to_string(),
                database_id: "db".to_string(),
                page_id: "p2".to_string(),
                labels: vec![],
            },
            NotionIssueCandidateData {
                external_id: "3".to_string(),
                external_system: "notion".to_string(),
                title: "Task 3".to_string(),
                description: None,
                status: "Backlog".to_string(),
                database_id: "db".to_string(),
                page_id: "p3".to_string(),
                labels: vec![],
            },
        ];

        let results = mapper.map_candidates(&candidates);

        assert_eq!(results.len(), 2); // "Done" should be filtered out
        assert_eq!(results[0].0.title, "Task 1");
        assert_eq!(results[1].0.title, "Task 3");
    }

    #[test]
    fn test_config_builder() {
        let config = IssueMappingConfig::new()
            .with_default_labels(vec!["from-notion".to_string()])
            .with_title_prefix("[N]".to_string())
            .with_exclude_statuses(vec!["Archived".to_string()]);

        assert_eq!(config.default_labels, vec!["from-notion".to_string()]);
        assert_eq!(config.title_prefix, Some("[N]".to_string()));
        assert_eq!(config.exclude_statuses, vec!["Archived".to_string()]);
    }
}
