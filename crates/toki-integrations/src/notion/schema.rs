//! Notion schema detection and property mapping
//!
//! This module provides convention-based detection of Notion database properties
//! with multi-language support (English, Chinese, Japanese).

use serde::{Deserialize, Serialize};

// ============================================================================
// Constants
// ============================================================================

/// Notion API version (use stable version)
pub const NOTION_API_VERSION: &str = "2022-06-28";

/// Notion API base URL
pub const NOTION_BASE_URL: &str = "https://api.notion.com/v1";

/// Rate limit: minimum interval between requests (333ms for ~3 req/sec)
pub const RATE_LIMIT_INTERVAL_MS: u64 = 350;

// ============================================================================
// Schema Detection - Convention Lists
// ============================================================================

// These convention lists use multi-language property names (English, Chinese, Japanese)
// to support Notion databases with localized property names.

/// Common property names for title/name fields
#[allow(clippy::non_ascii_literal)]
pub const TITLE_CONVENTIONS: &[&str] = &[
    // English
    "Name", "Title", "Task", "Issue", "Item", "Subject",
    // Chinese (Traditional & Simplified)
    "名稱", "標題", "任務", "名称", "标题", "任务",
    // Japanese
    "名前", "タイトル", "タスク", "件名",
];

/// Common property names for status fields
#[allow(clippy::non_ascii_literal)]
pub const STATUS_CONVENTIONS: &[&str] = &[
    // English
    "Status", "State", "Progress", "Stage",
    // Chinese
    "狀態", "進度", "階段", "状态", "进度", "阶段",
    // Japanese
    "ステータス", "状態", "進捗",
];

/// Common property names for description fields
#[allow(clippy::non_ascii_literal)]
pub const DESCRIPTION_CONVENTIONS: &[&str] = &[
    // English
    "Description", "Notes", "Details", "Summary", "Content",
    // Chinese
    "描述", "備註", "說明", "內容", "备注", "说明", "内容",
    // Japanese
    "説明", "メモ", "詳細", "内容",
];

/// Common property names for time tracking fields
#[allow(clippy::non_ascii_literal)]
pub const TIME_CONVENTIONS: &[&str] = &[
    // English
    "Time", "Hours", "Duration", "Spent", "Logged", "Tracked",
    // Chinese
    "時間", "工時", "耗時", "時數", "时间", "工时", "耗时",
    // Japanese
    "時間", "工数", "作業時間",
];

/// Common property names for priority fields
#[allow(clippy::non_ascii_literal)]
pub const PRIORITY_CONVENTIONS: &[&str] = &[
    // English
    "Priority", "Importance", "Urgency",
    // Chinese
    "優先級", "重要性", "优先级", "重要程度",
    // Japanese
    "優先度", "重要度",
];

/// Common property names for assignee fields
#[allow(clippy::non_ascii_literal)]
pub const ASSIGNEE_CONVENTIONS: &[&str] = &[
    // English
    "Assignee", "Assigned", "Owner", "Responsible",
    // Chinese
    "負責人", "指派", "负责人",
    // Japanese
    "担当", "担当者",
];

/// Common property names for due date fields
#[allow(clippy::non_ascii_literal)]
pub const DUE_DATE_CONVENTIONS: &[&str] = &[
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
