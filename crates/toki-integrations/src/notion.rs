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

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::traits::{ProjectManagementSystem, SyncReport, TimeEntry, WorkItemDetails};

// ============================================================================
// Constants
// ============================================================================

/// Notion API version (use stable version)
const NOTION_API_VERSION: &str = "2022-06-28";

/// Notion API base URL
const NOTION_BASE_URL: &str = "https://api.notion.com/v1";

/// Rate limit: minimum interval between requests (333ms for ~3 req/sec)
const RATE_LIMIT_INTERVAL_MS: u64 = 350;

// ============================================================================
// Schema Detection - Convention Lists
// ============================================================================

// These convention lists use multi-language property names (English, Chinese, Japanese)
// to support Notion databases with localized property names.

/// Common property names for title/name fields
#[allow(clippy::non_ascii_literal)]
const TITLE_CONVENTIONS: &[&str] = &[
    // English
    "Name", "Title", "Task", "Issue", "Item", "Subject",
    // Chinese (Traditional & Simplified)
    "名稱", "標題", "任務", "名称", "标题", "任务",
    // Japanese
    "名前", "タイトル", "タスク", "件名",
];

/// Common property names for status fields
#[allow(clippy::non_ascii_literal)]
const STATUS_CONVENTIONS: &[&str] = &[
    // English
    "Status", "State", "Progress", "Stage",
    // Chinese
    "狀態", "進度", "階段", "状态", "进度", "阶段",
    // Japanese
    "ステータス", "状態", "進捗",
];

/// Common property names for description fields
#[allow(clippy::non_ascii_literal)]
const DESCRIPTION_CONVENTIONS: &[&str] = &[
    // English
    "Description", "Notes", "Details", "Summary", "Content",
    // Chinese
    "描述", "備註", "說明", "內容", "备注", "说明", "内容",
    // Japanese
    "説明", "メモ", "詳細", "内容",
];

/// Common property names for time tracking fields
#[allow(clippy::non_ascii_literal)]
const TIME_CONVENTIONS: &[&str] = &[
    // English
    "Time", "Hours", "Duration", "Spent", "Logged", "Tracked",
    // Chinese
    "時間", "工時", "耗時", "時數", "时间", "工时", "耗时",
    // Japanese
    "時間", "工数", "作業時間",
];

/// Common property names for priority fields
#[allow(clippy::non_ascii_literal)]
const PRIORITY_CONVENTIONS: &[&str] = &[
    // English
    "Priority", "Importance", "Urgency",
    // Chinese
    "優先級", "重要性", "优先级", "重要程度",
    // Japanese
    "優先度", "重要度",
];

/// Common property names for assignee fields
#[allow(clippy::non_ascii_literal)]
const ASSIGNEE_CONVENTIONS: &[&str] = &[
    // English
    "Assignee", "Assigned", "Owner", "Responsible",
    // Chinese
    "負責人", "指派", "负责人",
    // Japanese
    "担当", "担当者",
];

/// Common property names for due date fields
#[allow(clippy::non_ascii_literal)]
const DUE_DATE_CONVENTIONS: &[&str] = &[
    // English
    "Due", "Due Date", "Deadline", "End Date",
    // Chinese
    "截止日期", "到期日", "期限", "截止日",
    // Japanese
    "期限", "締切", "期日",
];

// ============================================================================
// Schema Detection - Property Mapping
// ============================================================================

/// Mapping of database properties to semantic roles
#[derive(Debug, Clone, Default)]
pub struct PropertyMapping {
    /// Property name for the title/name field (usually the primary identifier)
    pub title: Option<String>,
    /// Property name for status/state
    pub status: Option<String>,
    /// Property name for description/notes
    pub description: Option<String>,
    /// Property name for time tracking (must be number type)
    pub time: Option<String>,
    /// Property name for priority
    pub priority: Option<String>,
    /// Property name for assignee
    pub assignee: Option<String>,
    /// Property name for due date
    pub due_date: Option<String>,
}

impl PropertyMapping {
    /// Create a new empty mapping
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the mapping has at least a title property
    #[must_use]
    pub fn has_title(&self) -> bool {
        self.title.is_some()
    }

    /// Check if the mapping has time tracking configured
    #[must_use]
    pub fn has_time_tracking(&self) -> bool {
        self.time.is_some()
    }
}

/// User configuration for property mapping override
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PropertyMappingConfig {
    /// Override for title property name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_property: Option<String>,
    /// Override for status property name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_property: Option<String>,
    /// Override for description property name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_property: Option<String>,
    /// Override for time property name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_property: Option<String>,
    /// Override for priority property name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_property: Option<String>,
    /// Override for assignee property name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_property: Option<String>,
    /// Override for due date property name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date_property: Option<String>,
}

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
    fn as_text(&self) -> String {
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
type SearchResponse = NotionPaginatedResponse<NotionDatabase>;

// ============================================================================
// Property Update Payloads
// ============================================================================

/// Payload for updating page properties
#[derive(Debug, Serialize)]
struct UpdatePageRequest {
    properties: HashMap<String, PropertyUpdateValue>,
}

/// Property update value
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum PropertyUpdateValue {
    Number { number: f64 },
    RichText { rich_text: Vec<RichTextInput> },
    Title { title: Vec<RichTextInput> },
    Checkbox { checkbox: bool },
    Select { select: SelectInput },
    Status { status: SelectInput },
}

/// Rich text input for updates
#[derive(Debug, Serialize)]
struct RichTextInput {
    text: TextContent,
}

/// Text content for rich text
#[derive(Debug, Serialize)]
struct TextContent {
    content: String,
}

/// Select input for updates
#[derive(Debug, Serialize)]
struct SelectInput {
    name: String,
}

// ============================================================================
// Rate Limiter
// ============================================================================

/// Simple rate limiter for Notion API
struct RateLimiter {
    last_request: Mutex<Instant>,
    interval: Duration,
}

impl RateLimiter {
    fn new(interval_ms: u64) -> Self {
        Self {
            last_request: Mutex::new(
                Instant::now()
                    .checked_sub(Duration::from_millis(interval_ms))
                    .unwrap_or_else(Instant::now),
            ),
            interval: Duration::from_millis(interval_ms),
        }
    }

    async fn wait(&self) {
        let mut last = self.last_request.lock().await;
        let elapsed = last.elapsed();
        if elapsed < self.interval {
            tokio::time::sleep(self.interval - elapsed).await;
        }
        *last = Instant::now();
    }
}

// ============================================================================
// Notion Client
// ============================================================================

/// Notion API client
pub struct NotionClient {
    api_key: String,
    client: reqwest::Client,
    rate_limiter: Arc<RateLimiter>,
    /// Mapping from `external_id` (short ID) to full page ID
    page_id_cache: Arc<Mutex<HashMap<String, String>>>,
    /// Configured time property name (overrides auto-detection)
    time_property_override: Arc<Mutex<Option<String>>>,
}

impl NotionClient {
    /// Create a new Notion client
    ///
    /// # Arguments
    /// * `api_key` - Notion integration token (starts with `secret_`)
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created
    pub fn new(api_key: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            api_key,
            client,
            rate_limiter: Arc::new(RateLimiter::new(RATE_LIMIT_INTERVAL_MS)),
            page_id_cache: Arc::new(Mutex::new(HashMap::new())),
            time_property_override: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a new Notion client with a configured time property
    ///
    /// # Arguments
    /// * `api_key` - Notion integration token
    /// * `time_property` - Name of the property to use for time tracking
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created
    pub fn with_time_property(api_key: String, time_property: String) -> Result<Self> {
        let mut client = Self::new(api_key)?;
        client.time_property_override = Arc::new(Mutex::new(Some(time_property)));
        Ok(client)
    }

    /// Set the time property name for time tracking
    pub async fn set_time_property(&self, property_name: Option<String>) {
        let mut guard = self.time_property_override.lock().await;
        *guard = property_name;
    }

    /// Get the configured time property name
    pub async fn get_time_property(&self) -> Option<String> {
        let guard = self.time_property_override.lock().await;
        guard.clone()
    }

    /// Make an authenticated GET request with rate limiting
    async fn get<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        self.rate_limiter.wait().await;

        log::debug!("GET {url}");

        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .send()
            .await
            .context("Failed to send request to Notion API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Notion API error ({status}): {body}");
        }

        response
            .json()
            .await
            .context("Failed to parse Notion API response")
    }

    /// Make an authenticated POST request with rate limiting
    async fn post<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        self.rate_limiter.wait().await;

        log::debug!("POST {url}");

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("Failed to send request to Notion API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Notion API error ({status}): {body}");
        }

        response
            .json()
            .await
            .context("Failed to parse Notion API response")
    }

    /// Make an authenticated PATCH request with rate limiting
    async fn patch<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        self.rate_limiter.wait().await;

        log::debug!("PATCH {url}");

        let response = self
            .client
            .patch(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("Failed to send request to Notion API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Notion API error ({status}): {body}");
        }

        response
            .json()
            .await
            .context("Failed to parse Notion API response")
    }

    // ========================================================================
    // Database APIs
    // ========================================================================

    /// Get a database by ID
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or database not found
    pub async fn get_database(&self, database_id: &str) -> Result<NotionDatabase> {
        let clean_id = Self::clean_id(database_id);
        let url = format!("{NOTION_BASE_URL}/databases/{clean_id}");
        self.get(&url).await
    }

    /// Query pages in a database
    ///
    /// # Arguments
    /// * `database_id` - Database UUID
    /// * `cursor` - Optional pagination cursor
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn query_database(
        &self,
        database_id: &str,
        cursor: Option<&str>,
    ) -> Result<NotionPaginatedResponse<NotionPage>> {
        let clean_id = Self::clean_id(database_id);
        let url = format!("{NOTION_BASE_URL}/databases/{clean_id}/query");

        let mut body = serde_json::json!({});
        if let Some(c) = cursor {
            body["start_cursor"] = serde_json::Value::String(c.to_string());
        }

        self.post(&url, &body).await
    }

    /// Query all pages in a database (handles pagination automatically)
    ///
    /// # Errors
    ///
    /// Returns an error if any API request fails
    pub async fn query_database_all(&self, database_id: &str) -> Result<Vec<NotionPage>> {
        let mut all_pages = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let response = self
                .query_database(database_id, cursor.as_deref())
                .await?;
            all_pages.extend(response.results);

            if response.has_more {
                cursor = response.next_cursor;
            } else {
                break;
            }
        }

        Ok(all_pages)
    }

    /// List databases the integration has access to (handles pagination)
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn list_databases(&self) -> Result<Vec<NotionDatabase>> {
        let url = format!("{NOTION_BASE_URL}/search");
        let mut all_databases = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut body = serde_json::json!({
                "filter": {
                    "value": "database",
                    "property": "object"
                }
            });

            if let Some(ref c) = cursor {
                body["start_cursor"] = serde_json::Value::String(c.clone());
            }

            let response: SearchResponse = self.post(&url, &body).await?;
            all_databases.extend(response.results);

            if response.has_more {
                cursor = response.next_cursor;
            } else {
                break;
            }
        }

        Ok(all_databases)
    }

    // ========================================================================
    // Page APIs
    // ========================================================================

    /// Get a page by ID
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or page not found
    pub async fn get_page(&self, page_id: &str) -> Result<NotionPage> {
        let clean_id = Self::clean_id(page_id);
        let url = format!("{NOTION_BASE_URL}/pages/{clean_id}");
        self.get(&url).await
    }

    /// Get blocks (content) of a page
    ///
    /// # Arguments
    /// * `page_id` - Page UUID
    /// * `cursor` - Optional pagination cursor
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn get_page_blocks(
        &self,
        page_id: &str,
        cursor: Option<&str>,
    ) -> Result<NotionPaginatedResponse<NotionBlock>> {
        let clean_id = Self::clean_id(page_id);
        let mut url = format!("{NOTION_BASE_URL}/blocks/{clean_id}/children");
        if let Some(c) = cursor {
            url = format!("{url}?start_cursor={c}");
        }
        self.get(&url).await
    }

    /// Get all blocks from a page (handles pagination)
    ///
    /// # Arguments
    /// * `page_id` - Page UUID
    /// * `max_depth` - Maximum recursion depth for nested blocks (default: 2)
    ///
    /// # Errors
    ///
    /// Returns an error if any API request fails
    pub async fn get_page_blocks_all(
        &self,
        page_id: &str,
        max_depth: usize,
    ) -> Result<Vec<NotionBlock>> {
        self.get_blocks_recursive(page_id, 0, max_depth).await
    }

    /// Recursively get blocks with depth limit
    #[async_recursion::async_recursion]
    async fn get_blocks_recursive(
        &self,
        block_id: &str,
        current_depth: usize,
        max_depth: usize,
    ) -> Result<Vec<NotionBlock>> {
        if current_depth >= max_depth {
            return Ok(Vec::new());
        }

        let mut all_blocks = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let response = self.get_page_blocks(block_id, cursor.as_deref()).await?;

            for block in response.results {
                let has_children = block.has_children;
                let block_id_clone = block.id.clone();
                all_blocks.push(block);

                // Recursively fetch children if needed
                if has_children && current_depth + 1 < max_depth {
                    let children = self
                        .get_blocks_recursive(&block_id_clone, current_depth + 1, max_depth)
                        .await?;
                    all_blocks.extend(children);
                }
            }

            if response.has_more {
                cursor = response.next_cursor;
            } else {
                break;
            }
        }

        Ok(all_blocks)
    }

    /// Update a page property
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn update_page_property(
        &self,
        page_id: &str,
        property_name: &str,
        value: NotionPropertyUpdate,
    ) -> Result<NotionPage> {
        let clean_id = Self::clean_id(page_id);
        let url = format!("{NOTION_BASE_URL}/pages/{clean_id}");

        let update_value = match value {
            NotionPropertyUpdate::Number(n) => PropertyUpdateValue::Number { number: n },
            NotionPropertyUpdate::Text(s) => PropertyUpdateValue::RichText {
                rich_text: vec![RichTextInput {
                    text: TextContent { content: s },
                }],
            },
            NotionPropertyUpdate::Title(s) => PropertyUpdateValue::Title {
                title: vec![RichTextInput {
                    text: TextContent { content: s },
                }],
            },
            NotionPropertyUpdate::Checkbox(b) => PropertyUpdateValue::Checkbox { checkbox: b },
            NotionPropertyUpdate::Select(s) => PropertyUpdateValue::Select {
                select: SelectInput { name: s },
            },
            NotionPropertyUpdate::Status(s) => PropertyUpdateValue::Status {
                status: SelectInput { name: s },
            },
        };

        let mut properties = HashMap::new();
        properties.insert(property_name.to_string(), update_value);

        let request = UpdatePageRequest { properties };
        self.patch(&url, &request).await
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Clean a Notion ID (remove dashes if present, ensure proper format)
    fn clean_id(id: &str) -> String {
        // Notion IDs can be with or without dashes
        // The API accepts both formats, but we normalize for consistency
        id.replace('-', "")
    }

    /// Generate external ID from database and page IDs
    ///
    /// Format: `{db_prefix_4}-{page_prefix_8}`
    /// Example: `ab12-f7e3c2a1`
    #[must_use]
    pub fn generate_external_id(database_id: &str, page_id: &str) -> String {
        let db_clean = Self::clean_id(database_id);
        let page_clean = Self::clean_id(page_id);

        let db_prefix = if db_clean.len() >= 4 {
            &db_clean[..4]
        } else {
            &db_clean
        };

        let page_prefix = if page_clean.len() >= 8 {
            &page_clean[..8]
        } else {
            &page_clean
        };

        format!("{db_prefix}-{page_prefix}")
    }

    /// Cache a page ID mapping for later lookup
    pub async fn cache_page_id(&self, external_id: &str, full_page_id: &str) {
        let mut cache = self.page_id_cache.lock().await;
        cache.insert(external_id.to_string(), full_page_id.to_string());
    }

    /// Look up a full page ID from external ID
    pub async fn get_cached_page_id(&self, external_id: &str) -> Option<String> {
        let cache = self.page_id_cache.lock().await;
        cache.get(external_id).cloned()
    }

    /// Detect time property from database schema
    ///
    /// Returns the configured time property if set, otherwise auto-detects
    /// using `TIME_CONVENTIONS`.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be fetched
    pub async fn detect_time_property(&self, database_id: &str) -> Result<Option<String>> {
        // Check configured override first
        if let Some(prop) = self.get_time_property().await {
            return Ok(Some(prop));
        }

        // Auto-detect from database schema
        let database = self.get_database(database_id).await?;
        let mapping = database.detect_property_mapping(None);
        Ok(mapping.time)
    }

    /// Extract text content from blocks
    #[must_use]
    pub fn blocks_to_text(blocks: &[NotionBlock]) -> String {
        blocks
            .iter()
            .map(NotionBlock::as_plain_text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Convert a Notion page to issue candidate data
    ///
    /// # Arguments
    /// * `page` - The Notion page to convert
    /// * `database_id` - The database ID (for `external_id` generation)
    /// * `mapping` - Property mapping for extracting fields
    /// * `description_text` - Optional pre-fetched description from blocks
    #[must_use]
    pub fn page_to_issue_candidate(
        page: &NotionPage,
        database_id: &str,
        mapping: &PropertyMapping,
        description_text: Option<String>,
    ) -> NotionIssueCandidateData {
        // Generate external ID
        let external_id = Self::generate_external_id(database_id, &page.id);

        // Extract title from mapped property
        let title = mapping
            .title
            .as_ref()
            .and_then(|prop_name| page.properties.get(prop_name))
            .and_then(NotionPropertyValue::as_plain_text)
            .unwrap_or_else(|| "Untitled".to_string());

        // Extract status from mapped property
        let status = mapping
            .status
            .as_ref()
            .and_then(|prop_name| page.properties.get(prop_name))
            .and_then(NotionPropertyValue::as_select_name)
            .unwrap_or_else(|| "Unknown".to_string());

        // Extract description from mapped property or use provided block text
        let description = mapping
            .description
            .as_ref()
            .and_then(|prop_name| page.properties.get(prop_name))
            .and_then(NotionPropertyValue::as_plain_text)
            .or(description_text);

        // Extract labels from multi-select if available
        let labels = page
            .properties
            .values()
            .find(|p| p.value_type == "multi_select")
            .and_then(|p| p.multi_select.as_ref())
            .map(|opts| opts.iter().map(|o| o.name.clone()).collect())
            .unwrap_or_default();

        NotionIssueCandidateData {
            external_id,
            external_system: "notion".to_string(),
            title,
            description,
            status,
            database_id: database_id.to_string(),
            page_id: page.id.clone(),
            labels,
        }
    }

    /// Sync pages from a Notion database to issue candidates
    ///
    /// This is a helper method that:
    /// 1. Queries all pages in the database
    /// 2. Detects property mapping
    /// 3. Converts pages to issue candidate data
    /// 4. Optionally fetches block content for descriptions
    ///
    /// # Arguments
    /// * `database_id` - The Notion database ID
    /// * `config` - Optional property mapping configuration
    /// * `fetch_blocks` - Whether to fetch page blocks for descriptions (slower but more complete)
    ///
    /// # Errors
    ///
    /// Returns an error if API calls fail
    pub async fn fetch_database_as_issues(
        &self,
        database_id: &str,
        config: Option<&PropertyMappingConfig>,
        fetch_blocks: bool,
    ) -> Result<Vec<NotionIssueCandidateData>> {
        // Get database schema and detect property mapping
        let database = self.get_database(database_id).await?;
        let mapping = database.detect_property_mapping(config);

        log::debug!(
            "Database '{}' schema: {}",
            database.title_plain_text(),
            database.describe_schema()
        );
        log::debug!(
            "Detected mapping - title: {:?}, status: {:?}, time: {:?}",
            mapping.title,
            mapping.status,
            mapping.time
        );

        // Query all pages
        let pages = self.query_database_all(database_id).await?;
        log::info!("Fetched {} pages from Notion database", pages.len());

        let mut candidates = Vec::with_capacity(pages.len());

        for page in &pages {
            // Skip archived pages
            if page.archived {
                continue;
            }

            // Fetch block content if requested
            let description_text = if fetch_blocks {
                match self.get_page_blocks_all(&page.id, 2).await {
                    Ok(blocks) => {
                        let text = Self::blocks_to_text(&blocks);
                        if text.is_empty() {
                            None
                        } else {
                            Some(text)
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to fetch blocks for page {}: {e}", page.id);
                        None
                    }
                }
            } else {
                None
            };

            // Convert to issue candidate
            let candidate = Self::page_to_issue_candidate(page, database_id, &mapping, description_text);

            // Cache the page ID for later lookups
            self.cache_page_id(&candidate.external_id, &page.id).await;

            candidates.push(candidate);
        }

        Ok(candidates)
    }
}

/// Property update value types
#[derive(Debug, Clone)]
pub enum NotionPropertyUpdate {
    Number(f64),
    Text(String),
    Title(String),
    Checkbox(bool),
    Select(String),
    Status(String),
}

// ============================================================================
// Data Conversion Types
// ============================================================================

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
            Some(Self::blocks_to_text(&blocks))
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
        // Get page ID from cache
        let page_id = self
            .get_cached_page_id(&entry.work_item_id)
            .await
            .ok_or_else(|| {
                anyhow::anyhow!("Page ID not found in cache for: {}", entry.work_item_id)
            })?;

        let page = self.get_page(&page_id).await?;

        // Determine the time property to use:
        // 1. Check configured override
        // 2. Use TIME_CONVENTIONS for auto-detection
        // 3. Look for any number property with time-related name

        let configured_prop = self.get_time_property().await;

        let property_name = if let Some(ref prop_name) = configured_prop {
            // Verify the configured property exists and is a number type
            if let Some(prop) = page.properties.get(prop_name) {
                if prop.value_type == "number" {
                    prop_name.clone()
                } else {
                    anyhow::bail!(
                        "Configured time property '{}' is not a number type (found: {})",
                        prop_name,
                        prop.value_type
                    );
                }
            } else {
                anyhow::bail!(
                    "Configured time property '{}' not found in page. Available properties: {}",
                    prop_name,
                    page.properties.keys().cloned().collect::<Vec<_>>().join(", ")
                );
            }
        } else {
            // Auto-detect using TIME_CONVENTIONS
            let detected = TIME_CONVENTIONS.iter().find_map(|conv| {
                page.properties.iter().find_map(|(name, prop)| {
                    if prop.value_type == "number" && name.eq_ignore_ascii_case(conv) {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
            });

            detected.ok_or_else(|| {
                let available_numbers: Vec<_> = page
                    .properties
                    .iter()
                    .filter(|(_, p)| p.value_type == "number")
                    .map(|(n, _)| n.as_str())
                    .collect();

                if available_numbers.is_empty() {
                    anyhow::anyhow!(
                        "No number properties found in page. Add a number property for time tracking."
                    )
                } else {
                    anyhow::anyhow!(
                        "No time tracking property found. Available number properties: {}. \
                         Use `toki config set notion.time_property <name>` to configure.",
                        available_numbers.join(", ")
                    )
                }
            })?
        };

        // Get current value and add new time
        let current_value = page
            .properties
            .get(&property_name)
            .and_then(NotionPropertyValue::as_number)
            .unwrap_or(0.0);

        let hours_to_add = f64::from(entry.duration_seconds) / 3600.0;
        let new_value = current_value + hours_to_add;

        log::debug!(
            "Updating time on Notion page {} (property '{}'): {:.2} + {:.2} = {:.2} hours",
            entry.work_item_id,
            property_name,
            current_value,
            hours_to_add,
            new_value
        );

        self.update_page_property(&page_id, &property_name, NotionPropertyUpdate::Number(new_value))
            .await?;

        log::info!(
            "Time entry added to Notion: {} (+{:.2}h, total: {:.2}h)",
            entry.work_item_id,
            hours_to_add,
            new_value
        );
        Ok(())
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

    #[test]
    fn test_generate_external_id() {
        let external_id =
            NotionClient::generate_external_id("abcd1234-5678-9abc-def0-123456789abc", "f7e3c2a1-1234-5678-9abc-def012345678");
        assert_eq!(external_id, "abcd-f7e3c2a1");
    }

    #[test]
    fn test_clean_id() {
        assert_eq!(
            NotionClient::clean_id("abcd1234-5678-9abc-def0-123456789abc"),
            "abcd123456789abcdef0123456789abc"
        );
        assert_eq!(
            NotionClient::clean_id("abcd123456789abcdef0123456789abc"),
            "abcd123456789abcdef0123456789abc"
        );
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
            ("タイトル", "title"),
            ("ステータス", "status"),
            ("説明", "rich_text"),
            ("作業時間", "number"),
        ]);

        let mapping = db.detect_property_mapping(None);

        assert_eq!(mapping.title, Some("タイトル".to_string()));
        assert_eq!(mapping.status, Some("ステータス".to_string()));
        assert_eq!(mapping.description, Some("説明".to_string()));
        assert_eq!(mapping.time, Some("作業時間".to_string()));
    }

    #[test]
    fn test_detect_property_mapping_mixed_languages() {
        // Database with a mix of English and Chinese property names
        let db = create_test_database(vec![
            ("Name", "title"),
            ("狀態", "status"),      // Chinese for Status
            ("Description", "rich_text"),
            ("工時", "number"),      // Chinese for Hours
        ]);

        let mapping = db.detect_property_mapping(None);

        assert_eq!(mapping.title, Some("Name".to_string()));
        assert_eq!(mapping.status, Some("狀態".to_string()));
        assert_eq!(mapping.description, Some("Description".to_string()));
        assert_eq!(mapping.time, Some("工時".to_string()));
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
        let title = page.properties.get("Name").and_then(|p| p.as_plain_text());
        let status = page.properties.get("Status").and_then(|p| p.as_select_name());

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
