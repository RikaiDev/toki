use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Time entry to be synced to PM system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeEntry {
    pub work_item_id: String, // External ID (e.g., "PROJ-123")
    pub start_time: DateTime<Utc>,
    pub duration_seconds: u32,
    pub description: String,
    pub category: String,
}

/// Result of a sync operation
#[derive(Debug, Clone)]
pub struct SyncReport {
    pub total_entries: usize,
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

/// Work item fetched from PM system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemDetails {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub project: Option<String>,
    pub workspace: Option<String>,
}

/// Generic trait for Project Management System integrations
#[async_trait]
pub trait ProjectManagementSystem: Send + Sync {
    /// Fetch work item details from the PM system
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or the item is not found
    async fn fetch_work_item(&self, work_item_id: &str) -> Result<WorkItemDetails>;

    /// Add a single time entry to the PM system
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    async fn add_time_entry(&self, entry: &TimeEntry) -> Result<()>;

    /// Batch sync multiple time entries
    ///
    /// # Errors
    ///
    /// Returns an error if the batch operation fails completely
    async fn batch_sync(&self, entries: Vec<TimeEntry>) -> Result<SyncReport>;

    /// Validate API credentials and connectivity
    ///
    /// # Errors
    ///
    /// Returns an error if credentials are invalid or API is unreachable
    async fn validate_credentials(&self) -> Result<bool>;

    /// Get the system name
    #[must_use]
    fn system_name(&self) -> &'static str;
}

impl SyncReport {
    /// Create a new sync report
    #[must_use]
    pub fn new(total: usize) -> Self {
        Self {
            total_entries: total,
            successful: 0,
            failed: 0,
            errors: Vec::new(),
        }
    }

    /// Record a successful sync
    pub fn record_success(&mut self) {
        self.successful += 1;
    }

    /// Record a failed sync with error message
    pub fn record_failure(&mut self, error: String) {
        self.failed += 1;
        self.errors.push(error);
    }

    /// Check if all entries were synced successfully
    #[must_use]
    pub fn is_complete_success(&self) -> bool {
        self.failed == 0 && self.successful == self.total_entries
    }
}

impl TimeEntry {
    /// Create a new time entry
    #[must_use]
    pub fn new(
        work_item_id: String,
        start_time: DateTime<Utc>,
        duration_seconds: u32,
        description: String,
        category: String,
    ) -> Self {
        Self {
            work_item_id,
            start_time,
            duration_seconds,
            description,
            category,
        }
    }

    /// Get duration in hours (rounded to 2 decimal places)
    #[must_use]
    pub fn duration_hours(&self) -> f64 {
        (f64::from(self.duration_seconds) / 3600.0 * 100.0).round() / 100.0
    }
}

// ============================================================================
// Issue Management Trait (for GitHub/GitLab integration)
// ============================================================================

/// Request to create a new issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIssueRequest {
    /// Issue title
    pub title: String,
    /// Issue body/description (markdown supported)
    pub body: Option<String>,
    /// Labels to apply to the issue
    pub labels: Vec<String>,
    /// Assignees (usernames)
    pub assignees: Vec<String>,
    /// Milestone ID or name (optional)
    pub milestone: Option<String>,
    /// Priority (optional, for systems that support it)
    pub priority: Option<String>,
    /// Due date (optional)
    pub due_date: Option<DateTime<Utc>>,
    /// Source metadata (e.g., Notion page ID for tracking)
    pub source_id: Option<String>,
    /// Source system (e.g., "notion")
    pub source_system: Option<String>,
}

/// Request to update an existing issue
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateIssueRequest {
    /// New title (if updating)
    pub title: Option<String>,
    /// New body (if updating)
    pub body: Option<String>,
    /// New state (open/closed)
    pub state: Option<IssueState>,
    /// Labels to set (replaces existing)
    pub labels: Option<Vec<String>>,
    /// Assignees to set (replaces existing)
    pub assignees: Option<Vec<String>>,
    /// Milestone to set
    pub milestone: Option<String>,
}

/// Issue state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
}

/// Created issue response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedIssue {
    /// Issue ID in the target system
    pub id: String,
    /// Issue number (e.g., #123)
    pub number: u64,
    /// Full URL to the issue
    pub url: String,
    /// Issue title
    pub title: String,
    /// Current state
    pub state: IssueState,
}

/// Detailed issue information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueDetails {
    /// Issue ID
    pub id: String,
    /// Issue number
    pub number: u64,
    /// Issue title
    pub title: String,
    /// Issue body
    pub body: Option<String>,
    /// Current state
    pub state: IssueState,
    /// Labels
    pub labels: Vec<String>,
    /// Assignees
    pub assignees: Vec<String>,
    /// URL
    pub url: String,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Updated at
    pub updated_at: DateTime<Utc>,
}

/// Result of an issue sync operation
#[derive(Debug, Clone, Default)]
pub struct IssueSyncReport {
    /// Total issues processed
    pub total: usize,
    /// Issues created
    pub created: usize,
    /// Issues updated
    pub updated: usize,
    /// Issues skipped (already in sync)
    pub skipped: usize,
    /// Failures
    pub failed: usize,
    /// Error messages
    pub errors: Vec<String>,
}

impl IssueSyncReport {
    /// Create a new empty report
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a created issue
    pub fn record_created(&mut self) {
        self.total += 1;
        self.created += 1;
    }

    /// Record an updated issue
    pub fn record_updated(&mut self) {
        self.total += 1;
        self.updated += 1;
    }

    /// Record a skipped issue
    pub fn record_skipped(&mut self) {
        self.total += 1;
        self.skipped += 1;
    }

    /// Record a failure
    pub fn record_failure(&mut self, error: String) {
        self.total += 1;
        self.failed += 1;
        self.errors.push(error);
    }
}

impl CreateIssueRequest {
    /// Create a new issue request with just title
    #[must_use]
    pub fn new(title: String) -> Self {
        Self {
            title,
            body: None,
            labels: Vec::new(),
            assignees: Vec::new(),
            milestone: None,
            priority: None,
            due_date: None,
            source_id: None,
            source_system: None,
        }
    }

    /// Set the body
    #[must_use]
    pub fn with_body(mut self, body: String) -> Self {
        self.body = Some(body);
        self
    }

    /// Add a label
    #[must_use]
    pub fn with_label(mut self, label: String) -> Self {
        self.labels.push(label);
        self
    }

    /// Set labels
    #[must_use]
    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.labels = labels;
        self
    }

    /// Set source tracking info
    #[must_use]
    pub fn with_source(mut self, source_id: String, source_system: String) -> Self {
        self.source_id = Some(source_id);
        self.source_system = Some(source_system);
        self
    }
}

/// Trait for issue management systems (GitHub, GitLab, etc.)
#[async_trait]
pub trait IssueManagement: Send + Sync {
    /// Create a new issue
    ///
    /// # Errors
    /// Returns an error if the API request fails
    async fn create_issue(&self, request: &CreateIssueRequest) -> Result<CreatedIssue>;

    /// Update an existing issue
    ///
    /// # Errors
    /// Returns an error if the API request fails or issue not found
    async fn update_issue(&self, issue_id: &str, update: &UpdateIssueRequest) -> Result<()>;

    /// Get issue details by ID or number
    ///
    /// # Errors
    /// Returns an error if the API request fails or issue not found
    async fn get_issue(&self, issue_id: &str) -> Result<IssueDetails>;

    /// Search for issues
    ///
    /// # Errors
    /// Returns an error if the API request fails
    async fn search_issues(&self, query: &str) -> Result<Vec<IssueDetails>>;

    /// List all open issues (with optional filters)
    ///
    /// # Errors
    /// Returns an error if the API request fails
    async fn list_issues(&self, state: Option<IssueState>) -> Result<Vec<IssueDetails>>;

    /// Validate API credentials
    ///
    /// # Errors
    /// Returns an error if credentials are invalid
    async fn validate_credentials(&self) -> Result<bool>;

    /// Get the system name
    #[must_use]
    fn system_name(&self) -> &'static str;

    /// Get the repository/project identifier
    #[must_use]
    fn project_identifier(&self) -> &str;
}
