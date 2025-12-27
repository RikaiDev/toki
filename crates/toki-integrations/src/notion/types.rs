//! Notion API data types
//!
//! This module contains the data structures for interacting with the Notion API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::schema::{PropertyMapping, PropertyMappingConfig};
use super::{
    ASSIGNEE_CONVENTIONS, DESCRIPTION_CONVENTIONS, DUE_DATE_CONVENTIONS, PRIORITY_CONVENTIONS,
    STATUS_CONVENTIONS, TIME_CONVENTIONS, TITLE_CONVENTIONS,
};

// ============================================================================
// Notion API Types
// ============================================================================

/// Notion Database schema
#[derive(Debug, Clone, Deserialize)]
pub struct NotionDatabase {
    pub id: String,
    pub title: Vec<NotionRichText>,
    pub properties: HashMap<String, NotionPropertySchema>,
    #[serde(default)]
    pub url: Option<String>,
}

impl NotionDatabase {
    /// Get the database title as plain text
    #[must_use]
    pub fn title_plain_text(&self) -> String {
        self.title
            .iter()
            .map(|rt| rt.plain_text.as_str())
            .collect::<Vec<_>>()
            .join("")
    }

    /// Detect property mapping using convention-based matching
    ///
    /// Detection priority:
    /// 1. User configuration (explicit override)
    /// 2. Convention matching (case-insensitive)
    /// 3. Type fallback (first property of expected type)
    #[must_use]
    pub fn detect_property_mapping(&self, config: Option<&PropertyMappingConfig>) -> PropertyMapping {
        let mut mapping = PropertyMapping::new();
        let props = &self.properties;

        // Helper to find property by conventions (case-insensitive)
        let find_by_convention = |conventions: &[&str]| -> Option<String> {
            for conv in conventions {
                for name in props.keys() {
                    if name.eq_ignore_ascii_case(conv) {
                        return Some(name.clone());
                    }
                }
            }
            None
        };

        // Helper to find property by type
        let find_by_type = |prop_type: &str| -> Option<String> {
            props
                .iter()
                .find(|(_, schema)| schema.property_type == prop_type)
                .map(|(name, _)| name.clone())
        };

        // Helper to find number property by conventions (for time tracking)
        let find_number_by_convention = |conventions: &[&str]| -> Option<String> {
            for conv in conventions {
                for (name, schema) in props {
                    if name.eq_ignore_ascii_case(conv) && schema.property_type == "number" {
                        return Some(name.clone());
                    }
                }
            }
            None
        };

        // Title: config > convention > type fallback
        mapping.title = config
            .and_then(|c| c.title_property.clone())
            .or_else(|| find_by_convention(TITLE_CONVENTIONS))
            .or_else(|| find_by_type("title"));

        // Status: config > convention > type fallback (status or select)
        mapping.status = config
            .and_then(|c| c.status_property.clone())
            .or_else(|| find_by_convention(STATUS_CONVENTIONS))
            .or_else(|| find_by_type("status"))
            .or_else(|| find_by_type("select"));

        // Description: config > convention > type fallback (rich_text)
        mapping.description = config
            .and_then(|c| c.description_property.clone())
            .or_else(|| find_by_convention(DESCRIPTION_CONVENTIONS))
            .or_else(|| {
                // Find rich_text that's not the title
                props
                    .iter()
                    .find(|(name, schema)| {
                        schema.property_type == "rich_text"
                            && mapping.title.as_ref() != Some(name)
                    })
                    .map(|(name, _)| name.clone())
            });

        // Time: config > convention (must be number type)
        mapping.time = config
            .and_then(|c| c.time_property.clone())
            .or_else(|| find_number_by_convention(TIME_CONVENTIONS));

        // Priority: config > convention
        mapping.priority = config
            .and_then(|c| c.priority_property.clone())
            .or_else(|| find_by_convention(PRIORITY_CONVENTIONS));

        // Assignee: config > convention > type fallback (people)
        mapping.assignee = config
            .and_then(|c| c.assignee_property.clone())
            .or_else(|| find_by_convention(ASSIGNEE_CONVENTIONS))
            .or_else(|| find_by_type("people"));

        // Due date: config > convention > type fallback (date)
        mapping.due_date = config
            .and_then(|c| c.due_date_property.clone())
            .or_else(|| find_by_convention(DUE_DATE_CONVENTIONS))
            .or_else(|| find_by_type("date"));

        mapping
    }

    /// Get a summary of detected properties for logging/debugging
    #[must_use]
    pub fn describe_schema(&self) -> String {
        let mut parts = Vec::new();
        for (name, schema) in &self.properties {
            parts.push(format!("{}: {}", name, schema.property_type));
        }
        parts.sort();
        parts.join(", ")
    }
}

/// Notion Property Schema (defines property type in database)
#[derive(Debug, Clone, Deserialize)]
pub struct NotionPropertySchema {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub property_type: String,
    // Type-specific configuration (select options, etc.)
    #[serde(default)]
    pub select: Option<NotionSelectConfig>,
    #[serde(default)]
    pub multi_select: Option<NotionMultiSelectConfig>,
    #[serde(default)]
    pub status: Option<NotionStatusConfig>,
}

/// Select property configuration
#[derive(Debug, Clone, Deserialize)]
pub struct NotionSelectConfig {
    pub options: Vec<NotionSelectOption>,
}

/// Multi-select property configuration
#[derive(Debug, Clone, Deserialize)]
pub struct NotionMultiSelectConfig {
    pub options: Vec<NotionSelectOption>,
}

/// Status property configuration
#[derive(Debug, Clone, Deserialize)]
pub struct NotionStatusConfig {
    pub options: Vec<NotionSelectOption>,
    #[serde(default)]
    pub groups: Vec<NotionStatusGroup>,
}

/// Status group (e.g., To-do, In Progress, Complete)
#[derive(Debug, Clone, Deserialize)]
pub struct NotionStatusGroup {
    pub id: String,
    pub name: String,
    pub color: String,
    pub option_ids: Vec<String>,
}

/// Select option
#[derive(Debug, Clone, Deserialize)]
pub struct NotionSelectOption {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

/// Notion Page (database item)
#[derive(Debug, Clone, Deserialize)]
pub struct NotionPage {
    pub id: String,
    pub properties: HashMap<String, NotionPropertyValue>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub created_time: Option<String>,
    #[serde(default)]
    pub last_edited_time: Option<String>,
    #[serde(default)]
    pub archived: bool,
}

/// Notion Property Value (actual value in a page)
#[derive(Debug, Clone, Deserialize)]
pub struct NotionPropertyValue {
    pub id: String,
    #[serde(rename = "type")]
    pub value_type: String,
    // Type-specific values
    #[serde(default)]
    pub title: Option<Vec<NotionRichText>>,
    #[serde(default)]
    pub rich_text: Option<Vec<NotionRichText>>,
    #[serde(default)]
    pub number: Option<f64>,
    #[serde(default)]
    pub select: Option<NotionSelectValue>,
    #[serde(default)]
    pub multi_select: Option<Vec<NotionSelectValue>>,
    #[serde(default)]
    pub status: Option<NotionSelectValue>,
    #[serde(default)]
    pub checkbox: Option<bool>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone_number: Option<String>,
    #[serde(default)]
    pub date: Option<NotionDateValue>,
}

impl NotionPropertyValue {
    /// Extract plain text from title or `rich_text` properties
    #[must_use]
    pub fn as_plain_text(&self) -> Option<String> {
        if let Some(ref texts) = self.title {
            if !texts.is_empty() {
                return Some(
                    texts
                        .iter()
                        .map(|t| t.plain_text.as_str())
                        .collect::<Vec<_>>()
                        .join(""),
                );
            }
        }
        if let Some(ref texts) = self.rich_text {
            if !texts.is_empty() {
                return Some(
                    texts
                        .iter()
                        .map(|t| t.plain_text.as_str())
                        .collect::<Vec<_>>()
                        .join(""),
                );
            }
        }
        None
    }

    /// Extract select/status value name
    #[must_use]
    pub fn as_select_name(&self) -> Option<String> {
        self.select
            .as_ref()
            .map(|s| s.name.clone())
            .or_else(|| self.status.as_ref().map(|s| s.name.clone()))
    }

    /// Extract number value
    #[must_use]
    pub fn as_number(&self) -> Option<f64> {
        self.number
    }
}

/// Select/Status value
#[derive(Debug, Clone, Deserialize)]
pub struct NotionSelectValue {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

/// Date value
#[derive(Debug, Clone, Deserialize)]
pub struct NotionDateValue {
    pub start: String,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub time_zone: Option<String>,
}

/// Rich text object
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionRichText {
    pub plain_text: String,
    #[serde(default)]
    pub href: Option<String>,
    #[serde(rename = "type", default)]
    pub text_type: String,
}

impl NotionRichText {
    /// Create a simple text object
    #[must_use]
    pub fn text(content: &str) -> Self {
        Self {
            plain_text: content.to_string(),
            href: None,
            text_type: "text".to_string(),
        }
    }
}

/// Notion Block (page content)
#[derive(Debug, Clone, Deserialize)]
pub struct NotionBlock {
    pub id: String,
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(default)]
    pub has_children: bool,
    // Type-specific content
    #[serde(default)]
    pub paragraph: Option<NotionBlockContent>,
    #[serde(default)]
    pub heading_1: Option<NotionBlockContent>,
    #[serde(default)]
    pub heading_2: Option<NotionBlockContent>,
    #[serde(default)]
    pub heading_3: Option<NotionBlockContent>,
    #[serde(default)]
    pub bulleted_list_item: Option<NotionBlockContent>,
    #[serde(default)]
    pub numbered_list_item: Option<NotionBlockContent>,
    #[serde(default)]
    pub to_do: Option<NotionToDoContent>,
    #[serde(default)]
    pub toggle: Option<NotionBlockContent>,
    #[serde(default)]
    pub code: Option<NotionCodeContent>,
    #[serde(default)]
    pub quote: Option<NotionBlockContent>,
    #[serde(default)]
    pub callout: Option<NotionBlockContent>,
}

impl NotionBlock {
    /// Extract plain text from block content
    #[must_use]
    pub fn as_plain_text(&self) -> String {
        let content = match self.block_type.as_str() {
            "paragraph" => self.paragraph.as_ref().map(NotionBlockContent::as_text),
            "heading_1" => self.heading_1.as_ref().map(NotionBlockContent::as_text),
            "heading_2" => self.heading_2.as_ref().map(NotionBlockContent::as_text),
            "heading_3" => self.heading_3.as_ref().map(NotionBlockContent::as_text),
            "bulleted_list_item" => self
                .bulleted_list_item
                .as_ref()
                .map(NotionBlockContent::as_text),
            "numbered_list_item" => self
                .numbered_list_item
                .as_ref()
                .map(NotionBlockContent::as_text),
            "to_do" => self.to_do.as_ref().map(|t| {
                t.rich_text
                    .iter()
                    .map(|rt| rt.plain_text.as_str())
                    .collect::<Vec<_>>()
                    .join("")
            }),
            "toggle" => self.toggle.as_ref().map(NotionBlockContent::as_text),
            "code" => self.code.as_ref().map(|c| {
                c.rich_text
                    .iter()
                    .map(|rt| rt.plain_text.as_str())
                    .collect::<Vec<_>>()
                    .join("")
            }),
            "quote" => self.quote.as_ref().map(NotionBlockContent::as_text),
            "callout" => self.callout.as_ref().map(NotionBlockContent::as_text),
            _ => None,
        };
        content.unwrap_or_default()
    }
}

/// Block content with rich text
#[derive(Debug, Clone, Deserialize)]
pub struct NotionBlockContent {
    pub rich_text: Vec<NotionRichText>,
}

impl NotionBlockContent {
    pub(crate) fn as_text(&self) -> String {
        self.rich_text
            .iter()
            .map(|rt| rt.plain_text.as_str())
            .collect::<Vec<_>>()
            .join("")
    }
}

/// To-do block content
#[derive(Debug, Clone, Deserialize)]
pub struct NotionToDoContent {
    pub rich_text: Vec<NotionRichText>,
    #[serde(default)]
    pub checked: bool,
}

/// Code block content
#[derive(Debug, Clone, Deserialize)]
pub struct NotionCodeContent {
    pub rich_text: Vec<NotionRichText>,
    #[serde(default)]
    pub language: Option<String>,
}

// ============================================================================
// API Response Types
// ============================================================================

/// Paginated response for database queries
#[derive(Debug, Deserialize)]
pub struct NotionPaginatedResponse<T> {
    pub results: Vec<T>,
    #[serde(default)]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// Response for listing databases (uses same pagination structure)
pub type SearchResponse = NotionPaginatedResponse<NotionDatabase>;

// ============================================================================
// Property Update Payloads
// ============================================================================

/// Payload for updating page properties
#[derive(Debug, Serialize)]
pub(crate) struct UpdatePageRequest {
    pub properties: HashMap<String, PropertyUpdateValue>,
}

/// Property update value
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum PropertyUpdateValue {
    Number { number: f64 },
    RichText { rich_text: Vec<RichTextInput> },
    Title { title: Vec<RichTextInput> },
    Checkbox { checkbox: bool },
    Select { select: SelectInput },
    Status { status: SelectInput },
}

/// Rich text input for updates
#[derive(Debug, Serialize)]
pub(crate) struct RichTextInput {
    pub text: TextContent,
}

/// Text content for rich text
#[derive(Debug, Serialize)]
pub(crate) struct TextContent {
    pub content: String,
}

/// Select input for updates
#[derive(Debug, Serialize)]
pub(crate) struct SelectInput {
    pub name: String,
}

// ============================================================================
// Data Conversion Types
// ============================================================================

/// Property update value types (public API)
#[derive(Debug, Clone)]
pub enum NotionPropertyUpdate {
    Number(f64),
    Text(String),
    Title(String),
    Checkbox(bool),
    Select(String),
    Status(String),
}

/// Data for creating a local `IssueCandidate` entry from Notion
#[derive(Debug, Clone, Serialize)]
pub struct NotionIssueCandidateData {
    pub external_id: String,
    pub external_system: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub database_id: String,
    pub page_id: String,
    pub labels: Vec<String>,
}
