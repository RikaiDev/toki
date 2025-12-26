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
