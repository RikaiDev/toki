use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::http::ResponseExt;
use crate::traits::{ProjectManagementSystem, SyncReport, TimeEntry, WorkItemDetails};

// ============================================================================
// API Response Types
// ============================================================================

/// Plane.so paginated response wrapper
#[derive(Debug, Deserialize)]
pub struct PaginatedResponse<T> {
    pub results: Vec<T>,
    pub next_cursor: Option<String>,
    pub prev_cursor: Option<String>,
    pub next_page_results: bool,
    pub prev_page_results: bool,
    pub count: usize,
    pub total_pages: usize,
    pub total_results: usize,
}

/// Plane.so Project
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlaneProject {
    pub id: Uuid,
    pub name: String,
    pub identifier: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_time_tracking_enabled: bool,
}

/// Plane.so State (workflow status)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlaneState {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}

/// Plane.so Work Item (formerly Issue)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlaneWorkItem {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description_html: Option<String>,
    #[serde(default)]
    pub description_stripped: Option<String>,
    pub sequence_id: i64,
    pub project: Uuid,
    #[serde(default)]
    pub project_detail: Option<PlaneProjectDetail>,
    #[serde(default)]
    pub state: Option<Uuid>,
    #[serde(default)]
    pub state_detail: Option<PlaneState>,
    #[serde(default)]
    pub assignees: Vec<Uuid>,
    #[serde(default)]
    pub priority: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Plane.so Project detail (embedded in work item)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlaneProjectDetail {
    pub id: Uuid,
    pub name: String,
    pub identifier: String,
}

/// Plane.so Worklog entry (time tracking)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlaneWorklog {
    pub id: Uuid,
    pub description: String,
    pub duration: i64, // seconds
    pub logged_by: Uuid,
    pub created_at: String,
    pub updated_at: String,
    pub project_id: Uuid,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

/// Plane.so Worklog create/update request
#[derive(Debug, Serialize)]
struct PlaneWorklogRequest {
    description: String,
    duration: i64, // seconds
}

/// Plane.so User
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlaneUser {
    pub id: Uuid,
    pub email: String,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// Plane.so Workspace
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlaneWorkspace {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
}

// ============================================================================
// Plane Client
// ============================================================================

/// Plane.so API client
pub struct PlaneClient {
    api_key: String,
    workspace_slug: String,
    base_url: String,
    client: reqwest::Client,
}

impl PlaneClient {
    /// Create a new Plane.so client
    ///
    /// # Arguments
    /// * `api_key` - Plane API key (from Profile Settings > Personal Access Token)
    /// * `workspace_slug` - Workspace slug (from URL)
    /// * `base_url` - Optional base URL for self-hosted instances
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created
    pub fn new(api_key: String, workspace_slug: String, base_url: Option<String>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        // Default to self-hosted URL format (no /api subdomain)
        let base_url = base_url.unwrap_or_else(|| "https://app.plane.so".to_string());
        // Remove trailing slash if present
        let base_url = base_url.trim_end_matches('/').to_string();

        Ok(Self {
            api_key,
            workspace_slug,
            base_url,
            client,
        })
    }

    /// Build API URL for workspace-scoped endpoints
    fn build_url(&self, path: &str) -> String {
        format!(
            "{}/api/v1/workspaces/{}/{}",
            self.base_url, self.workspace_slug, path
        )
    }

    /// Make an authenticated GET request
    async fn get<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        log::debug!("GET {url}");

        let response = self
            .client
            .get(url)
            .header("X-Api-Key", &self.api_key)
            .header("Content-Type", "application/json")
            .send()
            .await
            .context("Failed to send request to Plane API")?
            .ensure_success("Plane")
            .await?;

        response
            .json()
            .await
            .context("Failed to parse Plane API response")
    }

    /// Make an authenticated POST request
    async fn post<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        log::debug!("POST {url}");

        let response = self
            .client
            .post(url)
            .header("X-Api-Key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("Failed to send request to Plane API")?
            .ensure_success("Plane")
            .await?;

        response
            .json()
            .await
            .context("Failed to parse Plane API response")
    }

    // ========================================================================
    // Project APIs
    // ========================================================================

    /// List all projects in the workspace
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn list_projects(&self) -> Result<Vec<PlaneProject>> {
        let url = self.build_url("projects/");
        let response: PaginatedResponse<PlaneProject> = self.get(&url).await?;
        Ok(response.results)
    }

    /// Get a project by ID
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or project not found
    pub async fn get_project(&self, project_id: &Uuid) -> Result<PlaneProject> {
        let url = self.build_url(&format!("projects/{project_id}/"));
        self.get(&url).await
    }

    /// List all states for a project
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn list_states(
        &self,
        project_id: &Uuid,
    ) -> Result<Vec<PlaneState>> {
        let url = self.build_url(&format!("projects/{project_id}/states/"));
        let response: PaginatedResponse<PlaneState> = self.get(&url).await?;
        Ok(response.results)
    }

    // ========================================================================
    // Work Item APIs
    // ========================================================================

    /// List work items in a project
    ///
    /// # Arguments
    /// * `project_id` - Project UUID
    /// * `cursor` - Optional pagination cursor
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn list_work_items(
        &self,
        project_id: &Uuid,
        cursor: Option<&str>,
    ) -> Result<PaginatedResponse<PlaneWorkItem>> {
        // Include expand parameter to get state_detail and project_detail
        let mut url = self.build_url(&format!(
            "projects/{project_id}/work-items/?expand=state_detail,project_detail"
        ));
        if let Some(cursor) = cursor {
            url = format!("{url}&cursor={cursor}");
        }
        self.get(&url).await
    }

    /// Get a work item by project identifier and sequence number (e.g., "PROJ-123")
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or work item not found
    pub async fn get_work_item_by_identifier(
        &self,
        project_identifier: &str,
        sequence_id: &str,
    ) -> Result<PlaneWorkItem> {
        let url = self.build_url(&format!(
            "work-items/{project_identifier}-{sequence_id}/"
        ));
        self.get(&url).await
    }

    /// Get a work item by UUID
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or work item not found
    pub async fn get_work_item(&self, project_id: &Uuid, work_item_id: &Uuid) -> Result<PlaneWorkItem> {
        let url = self.build_url(&format!("projects/{project_id}/work-items/{work_item_id}/"));
        self.get(&url).await
    }

    /// Search work items across the workspace
    ///
    /// # Arguments
    /// * `query` - Search query string
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn search_work_items(&self, query: &str) -> Result<Vec<PlaneWorkItem>> {
        let base_url = self.build_url("work-items/search/");

        log::debug!("Searching work items: {query}");

        let response = self
            .client
            .get(&base_url)
            .header("X-Api-Key", &self.api_key)
            .header("Content-Type", "application/json")
            .query(&[("search", query)])
            .send()
            .await
            .context("Failed to send request to Plane API")?
            .ensure_success("Plane")
            .await?;

        let paginated: PaginatedResponse<PlaneWorkItem> = response
            .json()
            .await
            .context("Failed to parse Plane API response")?;

        Ok(paginated.results)
    }

    /// Get the current authenticated user
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn get_current_user(&self) -> Result<PlaneUser> {
        self.get(&format!("{}/api/v1/users/me/", self.base_url)).await
    }

    /// Get work items assigned to the current user
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn get_assigned_work_items(&self) -> Result<Vec<PlaneWorkItem>> {
        // First get current user
        let user = self.get_current_user().await?;
        
        // Get all projects and filter work items assigned to user
        let projects = self.list_projects().await?;
        let mut assigned_items = Vec::new();
        
        for project in projects {
            let response = self.list_work_items(&project.id, None).await?;
            for item in response.results {
                if item.assignees.contains(&user.id) {
                    assigned_items.push(item);
                }
            }
        }
        
        Ok(assigned_items)
    }

    /// Get all active (non-closed) work items for AI matching
    /// This includes items assigned to the user plus any recently updated items
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn get_active_work_items_for_matching(&self) -> Result<Vec<PlaneWorkItem>> {
        let projects = self.list_projects().await?;
        let mut all_items = Vec::new();
        
        for project in projects {
            let response = self.list_work_items(&project.id, None).await?;
            // Filter out completed/cancelled items if state indicates so
            for item in response.results {
                // Include items that are likely active
                // (can be refined based on state group if needed)
                if let Some(ref state) = item.state_detail {
                    let group = state.group.as_deref().unwrap_or("");
                    // Skip items in "completed" or "cancelled" groups
                    if group == "completed" || group == "cancelled" {
                        continue;
                    }
                }
                all_items.push(item);
            }
        }
        
        Ok(all_items)
    }

    /// Convert a `PlaneWorkItem` to a format suitable for local `IssueCandidate` storage
    ///
    /// # Arguments
    /// * `item` - The work item to convert
    /// * `project_identifier` - Optional fallback project identifier (e.g., "HYGIE")
    ///   when `project_detail` is not included in the API response
    /// * `state_map` - Optional mapping from state UUID to state name
    #[must_use]
    pub fn work_item_to_issue_candidate(
        item: &PlaneWorkItem,
        project_identifier: Option<&str>,
        state_map: Option<&std::collections::HashMap<Uuid, String>>,
    ) -> IssueCandidateData {
        let external_id = item
            .project_detail
            .as_ref()
            .map(|p| format!("{}-{}", p.identifier, item.sequence_id))
            .or_else(|| project_identifier.map(|id| format!("{id}-{}", item.sequence_id)))
            .unwrap_or_else(|| format!("UNKNOWN-{}", item.sequence_id));

        // Try state_detail first, then fall back to state_map lookup
        let status = item
            .state_detail
            .as_ref()
            .map(|s| s.name.clone())
            .or_else(|| {
                item.state.as_ref().and_then(|state_id| {
                    state_map.and_then(|map| map.get(state_id).cloned())
                })
            })
            .unwrap_or_else(|| "Unknown".to_string());

        IssueCandidateData {
            external_id,
            external_system: "plane".to_string(),
            title: item.name.clone(),
            description: item.description_stripped.clone(),
            status,
            project_id: item.project,
            project_name: item.project_detail.as_ref().map(|p| p.name.clone()),
            labels: Vec::new(), // Plane doesn't include labels in basic work item response
        }
    }

    // ========================================================================
    // Worklog (Time Tracking) APIs
    // ========================================================================

    /// List worklogs for a work item
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or time tracking is disabled
    pub async fn list_worklogs(
        &self,
        project_id: &Uuid,
        work_item_id: &Uuid,
    ) -> Result<Vec<PlaneWorklog>> {
        let url = self.build_url(&format!(
            "projects/{project_id}/work-items/{work_item_id}/worklogs/"
        ));
        self.get(&url).await
    }

    /// Create a worklog entry
    ///
    /// # Arguments
    /// * `project_id` - Project UUID
    /// * `work_item_id` - Work item UUID
    /// * `description` - Description of work done
    /// * `duration_seconds` - Duration in seconds
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or time tracking is disabled
    pub async fn create_worklog(
        &self,
        project_id: &Uuid,
        work_item_id: &Uuid,
        description: &str,
        duration_seconds: i64,
    ) -> Result<PlaneWorklog> {
        let url = self.build_url(&format!(
            "projects/{project_id}/work-items/{work_item_id}/worklogs/"
        ));

        let request = PlaneWorklogRequest {
            description: description.to_string(),
            duration: duration_seconds,
        };

        self.post(&url, &request).await
    }

    /// Get total worklog duration for all work items in a project
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn get_project_worklog_summary(
        &self,
        project_id: &Uuid,
    ) -> Result<Vec<WorklogSummary>> {
        let url = self.build_url(&format!("projects/{project_id}/total-worklogs/"));
        self.get(&url).await
    }
}

/// Summary of worklog duration per work item
#[derive(Debug, Clone, Deserialize)]
pub struct WorklogSummary {
    pub issue_id: Uuid,
    pub duration: i64, // seconds
}

/// Data for creating a local `IssueCandidate` entry
#[derive(Debug, Clone, Serialize)]
pub struct IssueCandidateData {
    pub external_id: String,
    pub external_system: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub project_id: Uuid,
    pub project_name: Option<String>,
    pub labels: Vec<String>,
}

// ============================================================================
// ProjectManagementSystem Trait Implementation
// ============================================================================

#[async_trait]
impl ProjectManagementSystem for PlaneClient {
    async fn fetch_work_item(&self, work_item_id: &str) -> Result<WorkItemDetails> {
        // Parse PROJ-123 format
        let parts: Vec<&str> = work_item_id.split('-').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid work item ID format: {work_item_id}. Expected format: PROJ-123");
        }

        let project_identifier = parts[0];
        let sequence_id = parts[1];

        log::debug!("Fetching work item from Plane: {work_item_id}");

        let work_item = self
            .get_work_item_by_identifier(project_identifier, sequence_id)
            .await?;

        let status = work_item
            .state_detail
            .map_or_else(|| "Unknown".to_string(), |s| s.name);

        let project_name = work_item
            .project_detail
            .map_or_else(|| "Unknown".to_string(), |p| p.name);

        Ok(WorkItemDetails {
            id: work_item_id.to_string(),
            title: work_item.name,
            description: work_item.description_stripped,
            status,
            project: Some(project_name),
            workspace: Some(self.workspace_slug.clone()),
        })
    }

    async fn add_time_entry(&self, entry: &TimeEntry) -> Result<()> {
        // Parse PROJ-123 format to get work item details
        let parts: Vec<&str> = entry.work_item_id.split('-').collect();
        if parts.len() != 2 {
            anyhow::bail!(
                "Invalid work item ID format: {}. Expected format: PROJ-123",
                entry.work_item_id
            );
        }

        let project_identifier = parts[0];
        let sequence_id = parts[1];

        // First, fetch the work item to get UUIDs
        let work_item = self
            .get_work_item_by_identifier(project_identifier, sequence_id)
            .await
            .context("Failed to find work item for time entry")?;

        let description = format!("{} - {}", entry.category, entry.description);

        log::debug!(
            "Adding worklog to Plane: {} ({} seconds)",
            entry.work_item_id,
            entry.duration_seconds
        );

        self.create_worklog(
            &work_item.project,
            &work_item.id,
            &description,
            i64::from(entry.duration_seconds),
        )
        .await?;

        log::info!("Worklog added to Plane: {}", entry.work_item_id);
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
        let url = format!("{}/api/v1/users/me/", self.base_url);

        log::debug!("Validating Plane credentials: {url}");

        let response = self
            .client
            .get(&url)
            .header("X-Api-Key", &self.api_key)
            .send()
            .await
            .context("Failed to connect to Plane API")?;

        Ok(response.status().is_success())
    }

    fn system_name(&self) -> &'static str {
        "plane"
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_entry_duration() {
        let entry = TimeEntry::new(
            "PROJ-123".to_string(),
            chrono::Utc::now(),
            3600,
            "Test".to_string(),
            "Coding".to_string(),
        );

        assert!((entry.duration_hours() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    #[ignore] // Skip due to macOS system configuration issues in test environment
    fn test_plane_client_creation() {
        let client = PlaneClient::new(
            "test-key".to_string(),
            "test-workspace".to_string(),
            None,
        );

        assert!(client.is_ok());
        assert_eq!(client.unwrap().system_name(), "plane");
    }

    #[test]
    fn test_build_url() {
        let client = PlaneClient::new(
            "test-key".to_string(),
            "my-workspace".to_string(),
            Some("https://plane.example.com".to_string()),
        )
        .unwrap();

        let url = client.build_url("projects/");
        assert_eq!(
            url,
            "https://plane.example.com/api/v1/workspaces/my-workspace/projects/"
        );
    }

    #[test]
    fn test_build_url_removes_trailing_slash() {
        let client = PlaneClient::new(
            "test-key".to_string(),
            "my-workspace".to_string(),
            Some("https://plane.example.com/".to_string()),
        )
        .unwrap();

        let url = client.build_url("projects/");
        assert_eq!(
            url,
            "https://plane.example.com/api/v1/workspaces/my-workspace/projects/"
        );
    }
}
