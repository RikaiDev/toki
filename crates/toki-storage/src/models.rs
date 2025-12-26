use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Activity record - tracks time spent on applications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub app_bundle_id: String,
    pub category: String,
    pub duration_seconds: u32,
    pub is_active: bool,
    pub work_item_id: Option<Uuid>, // Link to WorkItem for PM integration
}

/// Category mapping - maps application patterns to work categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub pattern: String,
    pub description: Option<String>,
}

/// Work session - aggregated work periods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub total_active_seconds: u32,
    pub idle_seconds: u32,
    pub interruption_count: u32,
    pub categories: Vec<String>,
    pub work_item_ids: Vec<Uuid>,
}

/// User settings and privacy controls
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub id: Uuid,
    pub pause_tracking: bool,
    pub excluded_apps: Vec<String>,
    pub idle_threshold_seconds: u32,
    // Work item tracking settings
    pub enable_work_item_tracking: bool,
    pub capture_window_title: bool,
    pub capture_browser_url: bool,
    pub url_whitelist: Vec<String>, // Domains like "plane.so", "github.com"
}

impl Activity {
    #[must_use]
    pub fn new(app_bundle_id: String, category: String, duration_seconds: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            app_bundle_id,
            category,
            duration_seconds,
            is_active: true,
            work_item_id: None,
        }
    }
}

impl Category {
    #[must_use]
    pub fn new(name: String, pattern: String, description: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            pattern,
            description,
        }
    }
}

impl Session {
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            start_time: Utc::now(),
            end_time: None,
            total_active_seconds: 0,
            idle_seconds: 0,
            interruption_count: 0,
            categories: Vec::new(),
            work_item_ids: Vec::new(),
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Precise activity span - tracks continuous usage of an application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySpan {
    pub id: Uuid,
    pub app_bundle_id: String,
    pub category: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_seconds: u32,
    pub project_id: Option<Uuid>, // Primary: which project being worked on
    pub work_item_id: Option<Uuid>, // Primary work item (auto-detected or manual)
    pub session_id: Option<Uuid>,
    // Rich context stored as JSON (for AI analysis and retroactive classification)
    pub context: Option<ActivitySpanContext>,
}

/// Rich context for activity span - enables AI analysis and flexible issue association
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivitySpanContext {
    pub work_item_ids: Vec<Uuid>, // Additional work items (multi-issue parallel work)
    pub edited_files: Vec<String>, // Files edited during this span
    pub git_commits: Vec<String>, // Commit messages during this span
    pub git_branch: Option<String>, // Current branch
    pub browser_urls: Vec<String>, // Visited PM/doc URLs
    pub tags: Vec<String>,        // Manual tags from user
    pub notes: Option<String>,    // Free-form notes
}

impl ActivitySpan {
    #[must_use]
    pub fn new(
        app_bundle_id: String,
        category: String,
        start_time: DateTime<Utc>,
        project_id: Option<Uuid>,
        work_item_id: Option<Uuid>,
        session_id: Option<Uuid>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            app_bundle_id,
            category,
            start_time,
            end_time: None,
            duration_seconds: 0,
            project_id,
            work_item_id,
            session_id,
            context: None, // Can be enriched later
        }
    }

    /// Get or create mutable context
    pub fn context_mut(&mut self) -> &mut ActivitySpanContext {
        self.context
            .get_or_insert_with(ActivitySpanContext::default)
    }

    /// Add a work item association (can add multiple via context)
    pub fn add_work_item(&mut self, work_item_id: Uuid) {
        let ctx = self.context_mut();
        if !ctx.work_item_ids.contains(&work_item_id) {
            ctx.work_item_ids.push(work_item_id);
        }
    }

    /// Add a tag
    pub fn add_tag(&mut self, tag: String) {
        let ctx = self.context_mut();
        if !ctx.tags.contains(&tag) {
            ctx.tags.push(tag);
        }
    }

    /// Add an edited file
    pub fn add_edited_file(&mut self, file: String) {
        let ctx = self.context_mut();
        if !ctx.edited_files.contains(&file) {
            ctx.edited_files.push(file);
        }
    }

    /// Set git branch
    pub fn set_git_branch(&mut self, branch: String) {
        self.context_mut().git_branch = Some(branch);
    }

    /// Add a git commit
    pub fn add_git_commit(&mut self, commit: String) {
        let ctx = self.context_mut();
        if !ctx.git_commits.contains(&commit) {
            ctx.git_commits.push(commit);
        }
    }

    /// Get all associated work item IDs (primary + additional from context)
    #[must_use]
    pub fn all_work_item_ids(&self) -> Vec<Uuid> {
        let mut ids = Vec::new();
        if let Some(id) = self.work_item_id {
            ids.push(id);
        }
        if let Some(ctx) = &self.context {
            for id in &ctx.work_item_ids {
                if !ids.contains(id) {
                    ids.push(*id);
                }
            }
        }
        ids
    }

    /// Calculate duration if span is finalized
    #[must_use]
    pub fn calculate_duration(&self) -> u32 {
        if let Some(end) = self.end_time {
            let duration = end.signed_duration_since(self.start_time);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let secs = duration.num_seconds().max(0) as u32;
            secs
        } else {
            self.duration_seconds
        }
    }

    /// Check if span is ongoing
    #[must_use]
    pub fn is_ongoing(&self) -> bool {
        self.end_time.is_none()
    }
}

impl Settings {
    #[must_use]
    pub fn default_settings() -> Self {
        Self {
            id: Uuid::new_v4(),
            pause_tracking: false,
            excluded_apps: Vec::new(),
            idle_threshold_seconds: 300,     // 5 minutes
            enable_work_item_tracking: true, // Auto-detect work items
            capture_window_title: true,      // Required for IDE workspace detection
            capture_browser_url: false,
            url_whitelist: vec![
                "plane.so".to_string(),
                "github.com".to_string(),
                "jira.atlassian.com".to_string(),
            ],
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::default_settings()
    }
}

/// Project - represents a workspace/codebase being worked on
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String, // e.g., "toki", "inboxd"
    pub path: String, // e.g., "/Users/xxx/Workspace/toki"
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    // PM system integration
    pub pm_system: Option<String>, // "plane", "github", "jira", "linear"
    pub pm_project_id: Option<String>, // Project ID in PM system
    pub pm_workspace: Option<String>, // Workspace/org in PM system
}

impl Project {
    #[must_use]
    pub fn new(name: String, path: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            path,
            description: None,
            created_at: now,
            last_active: now,
            pm_system: None,
            pm_project_id: None,
            pm_workspace: None,
        }
    }
}

/// Activity context for AI analysis - collected signals for issue inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityContext {
    pub project_id: Uuid,
    pub collected_at: DateTime<Utc>,
    // Signals for AI analysis
    pub recent_commits: Vec<String>, // Recent git commit messages
    pub edited_files: Vec<String>,   // Recently edited files
    pub browser_urls: Vec<String>,   // Visited PM system URLs
    pub window_titles: Vec<String>,  // Recent window titles
}

/// Issue candidate from PM system (cached for AI matching)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueCandidate {
    pub id: Uuid,
    pub project_id: Uuid,            // Local project reference
    pub external_id: String,         // e.g., "TOKI-9" or "2a24-f7e3c2a1"
    pub external_system: String,     // "plane", "github", "notion", etc.
    pub pm_project_id: Option<String>, // Project ID in PM system
    pub source_page_id: Option<String>, // Full page ID for Notion (for time updates)
    pub title: String,
    pub description: Option<String>,
    pub status: String,              // "backlog", "in_progress", "done"
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>, // 384-dim vector for semantic matching
    pub last_synced: DateTime<Utc>,
}

impl IssueCandidate {
    #[must_use]
    pub fn new(
        project_id: Uuid,
        external_id: String,
        external_system: String,
        title: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            external_id,
            external_system,
            pm_project_id: None,
            source_page_id: None,
            title,
            description: None,
            status: "backlog".to_string(),
            labels: Vec::new(),
            assignee: None,
            embedding: None,
            last_synced: Utc::now(),
        }
    }

    /// Generate text for embedding computation
    #[must_use]
    pub fn embedding_text(&self) -> String {
        let mut parts = vec![self.external_id.clone(), self.title.clone()];
        if let Some(desc) = &self.description {
            parts.push(desc.clone());
        }
        if !self.labels.is_empty() {
            parts.push(self.labels.join(" "));
        }
        parts.join("\n")
    }
}

/// Work item - represents a task/issue from PM systems (optional metadata)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: Uuid,
    pub external_id: String,     // e.g., "PROJ-123"
    pub external_system: String, // "plane", "github", "jira"
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub project: Option<String>,
    pub workspace: Option<String>,
    pub last_synced: Option<DateTime<Utc>>,
}

impl WorkItem {
    #[must_use]
    pub fn new(external_id: String, external_system: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            external_id,
            external_system,
            title: None,
            description: None,
            status: None,
            project: None,
            workspace: None,
            last_synced: None,
        }
    }
}

/// Time block - for retroactive classification
/// Groups activities into classifiable time segments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBlock {
    pub id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub project_id: Option<Uuid>,
    pub work_item_ids: Vec<Uuid>, // Can associate multiple issues
    pub description: String,      // User description (e.g., "UI polish", "bug fixes")
    pub tags: Vec<String>,
    pub source: TimeBlockSource, // Source: manual, AI suggested, auto-detected
    pub confidence: Option<f32>, // AI suggestion confidence score
    pub confirmed: bool,         // Whether user has confirmed
    pub created_at: DateTime<Utc>,
}

/// Time block source
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimeBlockSource {
    Manual,       // User manually created
    AiSuggested,  // AI auto-suggested
    AutoDetected, // System auto-detected (e.g., from git branch)
}

impl TimeBlock {
    #[must_use]
    pub fn manual(start_time: DateTime<Utc>, end_time: DateTime<Utc>, description: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            start_time,
            end_time,
            project_id: None,
            work_item_ids: Vec::new(),
            description,
            tags: Vec::new(),
            source: TimeBlockSource::Manual,
            confidence: None,
            confirmed: true, // Manual creation is auto-confirmed
            created_at: Utc::now(),
        }
    }

    #[must_use]
    pub fn ai_suggested(
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        description: String,
        work_item_ids: Vec<Uuid>,
        confidence: f32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            start_time,
            end_time,
            project_id: None,
            work_item_ids,
            description,
            tags: Vec::new(),
            source: TimeBlockSource::AiSuggested,
            confidence: Some(confidence),
            confirmed: false, // AI suggestions require user confirmation
            created_at: Utc::now(),
        }
    }
}

/// Daily summary - for display and retroactive classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySummary {
    pub date: chrono::NaiveDate,
    pub total_active_seconds: u32,
    pub projects: Vec<ProjectSummary>,
    pub unclassified_seconds: u32,   // Unclassified time
    pub time_blocks: Vec<TimeBlock>, // AI suggested time blocks
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub project_id: Uuid,
    pub project_name: String,
    pub total_seconds: u32,
    pub categories: std::collections::HashMap<String, u32>, // category -> seconds
    pub top_files: Vec<(String, u32)>,                      // (file_path, seconds)
}

/// Integration configuration for PM systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationConfig {
    pub id: Uuid,
    pub system_type: String, // "plane", "github", "jira"
    pub api_url: String,
    pub api_key: String,
    pub workspace_slug: Option<String>,
    pub project_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl IntegrationConfig {
    #[must_use]
    pub fn new(system_type: String, api_url: String, api_key: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            system_type,
            api_url,
            api_key,
            workspace_slug: None,
            project_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Classification rule type - determines how the pattern is matched
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PatternType {
    /// Exact URL domain match (e.g., "instagram.com")
    Domain,
    /// Window title substring match (e.g., "Cake")
    WindowTitle,
    /// App bundle ID match (e.g., "com.brave.Browser")
    BundleId,
    /// URL path contains (e.g., "/feed" for social feeds)
    UrlPath,
}

impl std::fmt::Display for PatternType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Domain => write!(f, "domain"),
            Self::WindowTitle => write!(f, "window_title"),
            Self::BundleId => write!(f, "bundle_id"),
            Self::UrlPath => write!(f, "url_path"),
        }
    }
}

impl std::str::FromStr for PatternType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "domain" => Ok(Self::Domain),
            "window_title" => Ok(Self::WindowTitle),
            "bundle_id" => Ok(Self::BundleId),
            "url_path" => Ok(Self::UrlPath),
            _ => Err(format!("Unknown pattern type: {s}")),
        }
    }
}

/// User-defined classification rule - learns from user corrections
/// Higher priority rules are checked first
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationRule {
    pub id: Uuid,
    pub pattern: String,       // The pattern to match (e.g., "instagram.com", "Cake")
    pub pattern_type: PatternType,
    pub category: String,      // Target category (e.g., "Break", "Research")
    pub priority: i32,         // Higher = checked first (user rules default to 100)
    pub created_at: DateTime<Utc>,
    pub hit_count: u32,        // How many times this rule matched
    pub last_hit: Option<DateTime<Utc>>,
}

impl ClassificationRule {
    /// Create a new user-defined rule from a correction
    #[must_use]
    pub fn from_correction(pattern: String, pattern_type: PatternType, category: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            pattern,
            pattern_type,
            category,
            priority: 100, // User rules have high priority
            created_at: Utc::now(),
            hit_count: 0,
            last_hit: None,
        }
    }

    /// Check if this rule matches the given context
    #[must_use]
    pub fn matches(&self, window_title: Option<&str>, app_id: &str) -> bool {
        match self.pattern_type {
            PatternType::Domain => {
                // Check if window title contains the domain
                window_title.is_some_and(|title| {
                    title.to_lowercase().contains(&self.pattern.to_lowercase())
                })
            }
            PatternType::WindowTitle => {
                window_title.is_some_and(|title| {
                    title.to_lowercase().contains(&self.pattern.to_lowercase())
                })
            }
            PatternType::BundleId => {
                app_id.to_lowercase().contains(&self.pattern.to_lowercase())
            }
            PatternType::UrlPath => {
                // Check if window title contains URL path pattern
                window_title.is_some_and(|title| {
                    title.to_lowercase().contains(&self.pattern.to_lowercase())
                })
            }
        }
    }

    /// Record a hit for this rule
    pub fn record_hit(&mut self) {
        self.hit_count += 1;
        self.last_hit = Some(Utc::now());
    }
}
