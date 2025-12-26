use regex::Regex;
use std::path::Path;

/// Represents a parsed issue ID with its system
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueId {
    pub raw: String,             // Original matched text (e.g., "PROJ-123")
    pub id: String,              // Cleaned ID (e.g., "PROJ-123" or "123")
    pub project: Option<String>, // Project prefix if exists (e.g., "PROJ")
}

impl IssueId {
    /// Get the full issue ID string
    #[must_use]
    pub fn full_id(&self) -> String {
        self.id.clone()
    }
}

/// Parser for extracting issue IDs from text
pub struct IssueIdParser {
    patterns: Vec<Regex>,
}

impl IssueIdParser {
    /// Create a new parser with default patterns
    ///
    /// Supports:
    /// - PROJ-123 (Jira/Plane style with project prefix)
    /// - #123 (GitHub style)
    /// - `TASK_123` (underscore style)
    ///
    /// # Panics
    ///
    /// May panic if regex patterns are invalid (should never happen with hardcoded patterns)
    #[must_use]
    pub fn new() -> Self {
        let patterns = vec![
            // Jira/Plane style: PROJECT-123
            Regex::new(r"\b([A-Z][A-Z0-9]{1,10})-(\d+)\b").unwrap(),
            // GitHub style: #123
            Regex::new(r"#(\d+)\b").unwrap(),
            // Underscore style: TASK_123
            Regex::new(r"\b([A-Z][A-Z0-9_]{1,10})_(\d+)\b").unwrap(),
        ];

        Self { patterns }
    }

    /// Parse issue IDs from text
    ///
    /// Returns all matched issue IDs in order of appearance
    ///
    /// # Panics
    ///
    /// May panic if regex capture groups are accessed incorrectly (should not happen with valid patterns)
    #[must_use]
    pub fn parse(&self, text: &str) -> Vec<IssueId> {
        let mut results = Vec::new();

        for pattern in &self.patterns {
            for cap in pattern.captures_iter(text) {
                let raw = cap.get(0).unwrap().as_str().to_string();

                let (id, project) = if pattern.as_str().contains('#') {
                    // GitHub style #123
                    (cap.get(1).unwrap().as_str().to_string(), None)
                } else if cap.len() == 3 {
                    // PROJ-123 or TASK_123 style
                    let proj = cap.get(1).unwrap().as_str().to_string();
                    let num = cap.get(2).unwrap().as_str();
                    (format!("{proj}-{num}"), Some(proj))
                } else {
                    continue;
                };

                results.push(IssueId { raw, id, project });
            }
        }

        results
    }

    /// Extract issue ID from file path
    ///
    /// Looks for patterns in directory and file names
    #[must_use]
    pub fn extract_from_path(&self, path: &Path) -> Option<IssueId> {
        let path_str = path.to_string_lossy();

        // Try parsing the path as text
        let mut ids = self.parse(&path_str);

        // Return the first match
        ids.pop()
    }
}

impl Default for IssueIdParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_jira_style() {
        let parser = IssueIdParser::new();
        let ids = parser.parse("Working on PROJ-123 and TASK-456");

        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0].id, "PROJ-123");
        assert_eq!(ids[0].project, Some("PROJ".to_string()));
        assert_eq!(ids[1].id, "TASK-456");
    }

    #[test]
    fn test_parse_github_style() {
        let parser = IssueIdParser::new();
        let ids = parser.parse("Fix #123 and close #456");

        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0].id, "123");
        assert_eq!(ids[0].project, None);
    }

    #[test]
    fn test_parse_underscore_style() {
        let parser = IssueIdParser::new();
        let ids = parser.parse("Implementing TASK_123");

        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].id, "TASK-123");
        assert_eq!(ids[0].project, Some("TASK".to_string()));
    }

    #[test]
    fn test_path_extraction() {
        let parser = IssueIdParser::new();
        let path = Path::new("/projects/PROJ-123-feature");

        let id = parser.extract_from_path(path);
        assert!(id.is_some());
        assert_eq!(id.unwrap().id, "PROJ-123");
    }
}
