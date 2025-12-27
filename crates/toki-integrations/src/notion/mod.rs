//! Notion API client for Toki integration
//!
//! This module provides a client for interacting with the Notion API to:
//! - Fetch database schemas and pages as issue candidates
//! - Update page properties (e.g., for time tracking)
//!
//! # Rate Limiting
//!
//! Notion API has a rate limit of 3 requests per second. This client implements
//! automatic rate limiting to avoid hitting the limit.

mod client;
mod schema;
mod types;

use anyhow::Result;
use async_trait::async_trait;

use crate::traits::{ProjectManagementSystem, SyncReport, TimeEntry, WorkItemDetails};

// Re-export public types
pub use client::NotionClient;
pub use schema::{
    PropertyMapping, PropertyMappingConfig, ASSIGNEE_CONVENTIONS, DESCRIPTION_CONVENTIONS,
    DUE_DATE_CONVENTIONS, NOTION_API_VERSION, NOTION_BASE_URL, PRIORITY_CONVENTIONS,
    RATE_LIMIT_INTERVAL_MS, STATUS_CONVENTIONS, TIME_CONVENTIONS, TITLE_CONVENTIONS,
};
pub use types::{
    NotionBlock, NotionBlockContent, NotionCodeContent, NotionDatabase, NotionDateValue,
    NotionIssueCandidateData, NotionMultiSelectConfig, NotionPage, NotionPaginatedResponse,
    NotionPropertySchema, NotionPropertyUpdate, NotionPropertyValue, NotionRichText,
    NotionSelectConfig, NotionSelectOption, NotionSelectValue, NotionStatusConfig,
    NotionStatusGroup, NotionToDoContent,
};

// ============================================================================
// ProjectManagementSystem Trait Implementation
// ============================================================================

#[async_trait]
impl ProjectManagementSystem for NotionClient {
    async fn fetch_work_item(&self, work_item_id: &str) -> Result<WorkItemDetails> {
        // Try to get page ID from cache
        let page_id = self
            .get_cached_page_id(work_item_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("Page ID not found in cache for: {work_item_id}"))?;

        let page = self.get_page(&page_id).await?;

        // Extract title from properties (look for title type property)
        let title = page
            .properties
            .values()
            .find(|p| p.value_type == "title")
            .and_then(NotionPropertyValue::as_plain_text)
            .unwrap_or_else(|| "Untitled".to_string());

        // Extract status from properties (look for status or select type)
        let status = page
            .properties
            .values()
            .find(|p| p.value_type == "status" || p.value_type == "select")
            .and_then(NotionPropertyValue::as_select_name)
            .unwrap_or_else(|| "Unknown".to_string());

        // Get description from page blocks
        let blocks = self.get_page_blocks_all(&page_id, 2).await.unwrap_or_default();
        let description = if blocks.is_empty() {
            None
        } else {
            Some(NotionClient::blocks_to_text(&blocks))
        };

        Ok(WorkItemDetails {
            id: work_item_id.to_string(),
            title,
            description,
            status,
            project: None,
            workspace: None,
        })
    }

    async fn add_time_entry(&self, entry: &TimeEntry) -> Result<()> {
        self.add_time_entry_internal(&entry.work_item_id, entry.duration_seconds)
            .await
    }

    async fn batch_sync(&self, entries: Vec<TimeEntry>) -> Result<SyncReport> {
        let mut report = SyncReport::new(entries.len());

        for entry in entries {
            match self.add_time_entry(&entry).await {
                Ok(()) => report.record_success(),
                Err(e) => report.record_failure(format!("{}: {e}", entry.work_item_id)),
            }
        }

        Ok(report)
    }

    async fn validate_credentials(&self) -> Result<bool> {
        // Try to list databases to validate the token
        match self.list_databases().await {
            Ok(_) => Ok(true),
            Err(e) => {
                log::warn!("Notion credential validation failed: {e}");
                Ok(false)
            }
        }
    }

    fn system_name(&self) -> &'static str {
        "notion"
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_generate_external_id() {
        let external_id =
            NotionClient::generate_external_id("abcd1234-5678-9abc-def0-123456789abc", "f7e3c2a1-1234-5678-9abc-def012345678");
        assert_eq!(external_id, "abcd-f7e3c2a1");
    }

    #[test]
    fn test_rich_text_plain_text() {
        let rt = NotionRichText::text("Hello, World!");
        assert_eq!(rt.plain_text, "Hello, World!");
    }

    #[test]
    fn test_property_value_as_plain_text() {
        let prop = NotionPropertyValue {
            id: "test".to_string(),
            value_type: "title".to_string(),
            title: Some(vec![NotionRichText::text("Test Title")]),
            rich_text: None,
            number: None,
            select: None,
            multi_select: None,
            status: None,
            checkbox: None,
            url: None,
            email: None,
            phone_number: None,
            date: None,
        };

        assert_eq!(prop.as_plain_text(), Some("Test Title".to_string()));
    }

    // Helper to create a mock database for testing
    fn create_test_database(properties: Vec<(&str, &str)>) -> NotionDatabase {
        let mut props = HashMap::new();
        for (name, prop_type) in properties {
            props.insert(
                name.to_string(),
                NotionPropertySchema {
                    id: format!("id_{name}"),
                    name: name.to_string(),
                    property_type: prop_type.to_string(),
                    select: None,
                    multi_select: None,
                    status: None,
                },
            );
        }
        NotionDatabase {
            id: "test-db-id".to_string(),
            title: vec![NotionRichText::text("Test Database")],
            properties: props,
            url: None,
        }
    }

    #[test]
    fn test_detect_property_mapping_english_conventions() {
        let db = create_test_database(vec![
            ("Name", "title"),
            ("Status", "status"),
            ("Description", "rich_text"),
            ("Hours", "number"),
            ("Priority", "select"),
            ("Assignee", "people"),
            ("Due Date", "date"),
        ]);

        let mapping = db.detect_property_mapping(None);

        assert_eq!(mapping.title, Some("Name".to_string()));
        assert_eq!(mapping.status, Some("Status".to_string()));
        assert_eq!(mapping.description, Some("Description".to_string()));
        assert_eq!(mapping.time, Some("Hours".to_string()));
        assert_eq!(mapping.priority, Some("Priority".to_string()));
        assert_eq!(mapping.assignee, Some("Assignee".to_string()));
        assert_eq!(mapping.due_date, Some("Due Date".to_string()));
    }

    #[test]
    fn test_detect_property_mapping_chinese_conventions() {
        let db = create_test_database(vec![
            ("Task Name", "title"),  // Not a convention, will use type fallback
            ("Status", "status"),    // English convention
            ("Description", "rich_text"),
            ("Time", "number"),      // Convention match
        ]);

        let mapping = db.detect_property_mapping(None);

        // "Task Name" is not in conventions, but it's the only title type
        assert_eq!(mapping.title, Some("Task Name".to_string()));
        assert_eq!(mapping.status, Some("Status".to_string()));
        assert_eq!(mapping.time, Some("Time".to_string()));
    }

    #[test]
    fn test_detect_property_mapping_type_fallback() {
        let db = create_test_database(vec![
            ("My Custom Title", "title"),     // No convention match, type fallback
            ("My Custom Status", "status"),   // No convention match, type fallback
            ("Notes Field", "rich_text"),     // No convention match, type fallback
            ("Custom Number", "number"),      // No convention match, no time fallback (number needs convention)
            ("Team Members", "people"),       // No convention match, type fallback
            ("Target Date", "date"),          // No convention match, type fallback
        ]);

        let mapping = db.detect_property_mapping(None);

        assert_eq!(mapping.title, Some("My Custom Title".to_string()));
        assert_eq!(mapping.status, Some("My Custom Status".to_string()));
        assert_eq!(mapping.description, Some("Notes Field".to_string()));
        assert_eq!(mapping.time, None); // Number without convention = no time tracking
        assert_eq!(mapping.assignee, Some("Team Members".to_string()));
        assert_eq!(mapping.due_date, Some("Target Date".to_string()));
    }

    #[test]
    fn test_detect_property_mapping_user_config_override() {
        let db = create_test_database(vec![
            ("Name", "title"),
            ("Status", "status"),
            ("Custom Time Field", "number"),
        ]);

        let config = PropertyMappingConfig {
            title_property: Some("Name".to_string()),
            status_property: None,
            description_property: None,
            time_property: Some("Custom Time Field".to_string()),
            priority_property: None,
            assignee_property: None,
            due_date_property: None,
        };

        let mapping = db.detect_property_mapping(Some(&config));

        assert_eq!(mapping.title, Some("Name".to_string()));
        assert_eq!(mapping.status, Some("Status".to_string())); // Convention match
        assert_eq!(mapping.time, Some("Custom Time Field".to_string())); // Config override
    }

    #[test]
    fn test_detect_property_mapping_case_insensitive() {
        let db = create_test_database(vec![
            ("name", "title"),     // lowercase
            ("STATUS", "status"),  // uppercase
            ("HoUrS", "number"),   // mixed case
        ]);

        let mapping = db.detect_property_mapping(None);

        assert_eq!(mapping.title, Some("name".to_string()));
        assert_eq!(mapping.status, Some("STATUS".to_string()));
        assert_eq!(mapping.time, Some("HoUrS".to_string()));
    }

    #[test]
    fn test_property_mapping_helpers() {
        let mut mapping = PropertyMapping::new();
        assert!(!mapping.has_title());
        assert!(!mapping.has_time_tracking());

        mapping.title = Some("Name".to_string());
        assert!(mapping.has_title());
        assert!(!mapping.has_time_tracking());

        mapping.time = Some("Hours".to_string());
        assert!(mapping.has_time_tracking());
    }

    #[test]
    fn test_property_value_as_select_name() {
        // Test with select value
        let prop = NotionPropertyValue {
            id: "test".to_string(),
            value_type: "select".to_string(),
            title: None,
            rich_text: None,
            number: None,
            select: Some(NotionSelectValue {
                id: "sel-1".to_string(),
                name: "In Progress".to_string(),
                color: Some("blue".to_string()),
            }),
            multi_select: None,
            status: None,
            checkbox: None,
            url: None,
            email: None,
            phone_number: None,
            date: None,
        };

        assert_eq!(prop.as_select_name(), Some("In Progress".to_string()));

        // Test with status value
        let prop_status = NotionPropertyValue {
            id: "test".to_string(),
            value_type: "status".to_string(),
            title: None,
            rich_text: None,
            number: None,
            select: None,
            multi_select: None,
            status: Some(NotionSelectValue {
                id: "sta-1".to_string(),
                name: "Done".to_string(),
                color: Some("green".to_string()),
            }),
            checkbox: None,
            url: None,
            email: None,
            phone_number: None,
            date: None,
        };

        assert_eq!(prop_status.as_select_name(), Some("Done".to_string()));
    }

    #[test]
    fn test_property_value_as_number() {
        let prop = NotionPropertyValue {
            id: "test".to_string(),
            value_type: "number".to_string(),
            title: None,
            rich_text: None,
            number: Some(42.5),
            select: None,
            multi_select: None,
            status: None,
            checkbox: None,
            url: None,
            email: None,
            phone_number: None,
            date: None,
        };

        assert_eq!(prop.as_number(), Some(42.5));

        // Test with no number
        let prop_empty = NotionPropertyValue {
            id: "test".to_string(),
            value_type: "number".to_string(),
            title: None,
            rich_text: None,
            number: None,
            select: None,
            multi_select: None,
            status: None,
            checkbox: None,
            url: None,
            email: None,
            phone_number: None,
            date: None,
        };

        assert_eq!(prop_empty.as_number(), None);
    }

    #[test]
    fn test_property_value_rich_text() {
        let prop = NotionPropertyValue {
            id: "test".to_string(),
            value_type: "rich_text".to_string(),
            title: None,
            rich_text: Some(vec![
                NotionRichText::text("Hello "),
                NotionRichText::text("World"),
            ]),
            number: None,
            select: None,
            multi_select: None,
            status: None,
            checkbox: None,
            url: None,
            email: None,
            phone_number: None,
            date: None,
        };

        assert_eq!(prop.as_plain_text(), Some("Hello World".to_string()));
    }

    #[test]
    fn test_notion_issue_candidate_data() {
        let candidate = NotionIssueCandidateData {
            external_id: "abcd-12345678".to_string(),
            external_system: "notion".to_string(),
            title: "Fix login bug".to_string(),
            description: Some("Users cannot login with SSO".to_string()),
            status: "In Progress".to_string(),
            database_id: "db-123".to_string(),
            page_id: "page-456".to_string(),
            labels: vec!["bug".to_string(), "urgent".to_string()],
        };

        assert_eq!(candidate.external_id, "abcd-12345678");
        assert_eq!(candidate.external_system, "notion");
        assert_eq!(candidate.title, "Fix login bug");
        assert!(candidate.description.is_some());
        assert_eq!(candidate.status, "In Progress");
        assert_eq!(candidate.labels.len(), 2);
    }

    #[test]
    fn test_property_mapping_config_serialization() {
        let config = PropertyMappingConfig {
            title_property: Some("Task Name".to_string()),
            status_property: None,
            description_property: Some("Notes".to_string()),
            time_property: Some("Hours Spent".to_string()),
            priority_property: None,
            assignee_property: None,
            due_date_property: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("Task Name"));
        assert!(json.contains("Hours Spent"));
        // None values should be skipped
        assert!(!json.contains("status_property"));

        // Deserialize back
        let parsed: PropertyMappingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.title_property, Some("Task Name".to_string()));
        assert_eq!(parsed.status_property, None);
    }

    #[test]
    fn test_detect_property_mapping_japanese_conventions() {
        let db = create_test_database(vec![
            ("\u{30bf}\u{30a4}\u{30c8}\u{30eb}", "title"),
            ("\u{30b9}\u{30c6}\u{30fc}\u{30bf}\u{30b9}", "status"),
            ("\u{8aac}\u{660e}", "rich_text"),
            ("\u{4f5c}\u{696d}\u{6642}\u{9593}", "number"),
        ]);

        let mapping = db.detect_property_mapping(None);

        assert_eq!(mapping.title, Some("\u{30bf}\u{30a4}\u{30c8}\u{30eb}".to_string()));
        assert_eq!(mapping.status, Some("\u{30b9}\u{30c6}\u{30fc}\u{30bf}\u{30b9}".to_string()));
        assert_eq!(mapping.description, Some("\u{8aac}\u{660e}".to_string()));
        assert_eq!(mapping.time, Some("\u{4f5c}\u{696d}\u{6642}\u{9593}".to_string()));
    }

    #[test]
    fn test_detect_property_mapping_mixed_languages() {
        // Database with a mix of English and Chinese property names
        let db = create_test_database(vec![
            ("Name", "title"),
            ("\u{72c0}\u{614b}", "status"),      // Chinese for Status
            ("Description", "rich_text"),
            ("\u{5de5}\u{6642}", "number"),      // Chinese for Hours
        ]);

        let mapping = db.detect_property_mapping(None);

        assert_eq!(mapping.title, Some("Name".to_string()));
        assert_eq!(mapping.status, Some("\u{72c0}\u{614b}".to_string()));
        assert_eq!(mapping.description, Some("Description".to_string()));
        assert_eq!(mapping.time, Some("\u{5de5}\u{6642}".to_string()));
    }

    #[test]
    fn test_detect_property_mapping_empty_database() {
        let db = create_test_database(vec![]);
        let mapping = db.detect_property_mapping(None);

        assert_eq!(mapping.title, None);
        assert_eq!(mapping.status, None);
        assert_eq!(mapping.time, None);
    }

    #[test]
    fn test_generate_external_id_short_ids() {
        // Test with short IDs
        let external_id = NotionClient::generate_external_id("abc", "12345");
        assert_eq!(external_id, "abc-12345");

        // Test with exact length
        let external_id = NotionClient::generate_external_id("abcd", "12345678");
        assert_eq!(external_id, "abcd-12345678");
    }

    #[test]
    fn test_notion_property_update_variants() {
        // Test that enum variants can be created
        let num = NotionPropertyUpdate::Number(10.5);
        let text = NotionPropertyUpdate::Text("Hello".to_string());
        let title = NotionPropertyUpdate::Title("Task Name".to_string());
        let check = NotionPropertyUpdate::Checkbox(true);
        let select = NotionPropertyUpdate::Select("Option A".to_string());
        let status = NotionPropertyUpdate::Status("Done".to_string());

        // Verify variants exist (compile-time check mostly)
        match num {
            NotionPropertyUpdate::Number(n) => assert!((n - 10.5).abs() < f64::EPSILON),
            _ => panic!("Wrong variant"),
        }
        match text {
            NotionPropertyUpdate::Text(s) => assert_eq!(s, "Hello"),
            _ => panic!("Wrong variant"),
        }
        match title {
            NotionPropertyUpdate::Title(s) => assert_eq!(s, "Task Name"),
            _ => panic!("Wrong variant"),
        }
        match check {
            NotionPropertyUpdate::Checkbox(b) => assert!(b),
            _ => panic!("Wrong variant"),
        }
        match select {
            NotionPropertyUpdate::Select(s) => assert_eq!(s, "Option A"),
            _ => panic!("Wrong variant"),
        }
        match status {
            NotionPropertyUpdate::Status(s) => assert_eq!(s, "Done"),
            _ => panic!("Wrong variant"),
        }
    }

    // Helper to create a mock page for testing
    fn create_test_page(properties: Vec<(&str, NotionPropertyValue)>) -> NotionPage {
        let mut props = HashMap::new();
        for (name, value) in properties {
            props.insert(name.to_string(), value);
        }
        NotionPage {
            id: "page-123".to_string(),
            properties: props,
            url: Some("https://notion.so/page-123".to_string()),
            created_time: Some("2024-01-01T00:00:00Z".to_string()),
            last_edited_time: Some("2024-01-02T00:00:00Z".to_string()),
            archived: false,
        }
    }

    #[test]
    fn test_page_property_extraction() {
        let page = create_test_page(vec![
            ("Name", NotionPropertyValue {
                id: "name-id".to_string(),
                value_type: "title".to_string(),
                title: Some(vec![NotionRichText::text("Test Task")]),
                rich_text: None,
                number: None,
                select: None,
                multi_select: None,
                status: None,
                checkbox: None,
                url: None,
                email: None,
                phone_number: None,
                date: None,
            }),
            ("Status", NotionPropertyValue {
                id: "status-id".to_string(),
                value_type: "status".to_string(),
                title: None,
                rich_text: None,
                number: None,
                select: None,
                multi_select: None,
                status: Some(NotionSelectValue {
                    id: "s1".to_string(),
                    name: "In Progress".to_string(),
                    color: None,
                }),
                checkbox: None,
                url: None,
                email: None,
                phone_number: None,
                date: None,
            }),
        ]);

        // Extract values
        let title = page.properties.get("Name").and_then(NotionPropertyValue::as_plain_text);
        let status = page.properties.get("Status").and_then(NotionPropertyValue::as_select_name);

        assert_eq!(title, Some("Test Task".to_string()));
        assert_eq!(status, Some("In Progress".to_string()));
    }

    #[test]
    fn test_multi_select_extraction() {
        let prop = NotionPropertyValue {
            id: "test".to_string(),
            value_type: "multi_select".to_string(),
            title: None,
            rich_text: None,
            number: None,
            select: None,
            multi_select: Some(vec![
                NotionSelectValue {
                    id: "1".to_string(),
                    name: "bug".to_string(),
                    color: Some("red".to_string()),
                },
                NotionSelectValue {
                    id: "2".to_string(),
                    name: "urgent".to_string(),
                    color: Some("orange".to_string()),
                },
            ]),
            status: None,
            checkbox: None,
            url: None,
            email: None,
            phone_number: None,
            date: None,
        };

        let labels: Vec<String> = prop
            .multi_select
            .as_ref()
            .map(|ms| ms.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default();

        assert_eq!(labels, vec!["bug".to_string(), "urgent".to_string()]);
    }
}
