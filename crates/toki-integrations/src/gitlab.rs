//! GitLab Issues API client
//!
//! Implements the `IssueManagement` trait for GitLab projects.
//! Supports both GitLab.com and self-hosted GitLab instances.
//!
//! Also implements `ProjectManagementSystem` for time tracking integration.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};

use crate::http::ResponseExt;
use crate::traits::{
    CreateIssueRequest, CreatedIssue, IssueDetails, IssueManagement, IssueState,
    ProjectManagementSystem, SyncReport, TimeEntry, UpdateIssueRequest, WorkItemDetails,
};

/// GitLab API client for issue management
pub struct GitLabClient {
    client: Client,
    /// Project ID or path (URL-encoded)
    project: String,
    /// API base URL
    api_base: String,
}

/// GitLab API issue response
#[derive(Debug, Deserialize)]
struct GitLabIssue {
    id: u64,
    iid: u64, // Internal ID (the number shown in URLs)
    title: String,
    description: Option<String>,
    state: String,
    web_url: String,
    labels: Vec<String>,
    assignees: Vec<GitLabUser>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct GitLabUser {
    username: String,
}

/// GitLab API create issue request
#[derive(Debug, Serialize)]
struct GitLabCreateIssue {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<String>, // GitLab uses comma-separated string
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    milestone_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    due_date: Option<String>,
}

/// GitLab API update issue request
#[derive(Debug, Serialize)]
struct GitLabUpdateIssue {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state_event: Option<String>, // "close" or "reopen"
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    milestone_id: Option<u64>,
}

/// GitLab time tracking statistics response
#[derive(Debug, Deserialize)]
pub struct GitLabTimeStats {
    /// Time estimate in seconds
    pub time_estimate: i64,
    /// Total time spent in seconds
    pub total_time_spent: i64,
    /// Human-readable time estimate (e.g., "3h")
    pub human_time_estimate: Option<String>,
    /// Human-readable total time spent (e.g., "1h 30m")
    pub human_total_time_spent: Option<String>,
}

/// GitLab add spent time request
#[derive(Debug, Serialize)]
struct GitLabAddSpentTimeRequest {
    /// Duration string (e.g., "1h30m", "2h", "45m")
    duration: String,
}

/// GitLab time estimate request
#[derive(Debug, Serialize)]
struct GitLabTimeEstimateRequest {
    /// Duration string (e.g., "3h", "1d")
    duration: String,
}

/// GitLab note (comment) request
#[derive(Serialize)]
struct NoteRequest<'a> {
    body: &'a str,
}

impl GitLabClient {
    /// Create a new GitLab client for gitlab.com
    ///
    /// # Arguments
    /// * `token` - GitLab Personal Access Token
    /// * `project` - Project ID or path (e.g., "123" or "group/project")
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be created
    pub fn new(token: &str, project: &str) -> Result<Self> {
        Self::with_base_url(token, project, "https://gitlab.com/api/v4")
    }

    /// Create a new GitLab client with custom API base URL (for self-hosted)
    ///
    /// # Arguments
    /// * `token` - GitLab Personal Access Token
    /// * `project` - Project ID or path
    /// * `api_base` - API base URL (e.g., "<https://gitlab.example.com/api/v4>")
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be created
    pub fn with_base_url(token: &str, project: &str, api_base: &str) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            "PRIVATE-TOKEN",
            header::HeaderValue::from_str(token).context("Invalid token format")?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static("toki-time-tracker"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create HTTP client")?;

        // URL-encode the project path if it contains slashes
        let encoded_project = urlencoding::encode(project).into_owned();

        Ok(Self {
            client,
            project: encoded_project,
            api_base: api_base.trim_end_matches('/').to_string(),
        })
    }

    /// Get the issues API URL
    fn issues_url(&self) -> String {
        format!("{}/projects/{}/issues", self.api_base, self.project)
    }

    /// Convert GitLab issue to `IssueDetails`
    fn to_issue_details(issue: GitLabIssue) -> Result<IssueDetails> {
        let state = if issue.state == "opened" {
            IssueState::Open
        } else {
            IssueState::Closed
        };

        let created_at = chrono::DateTime::parse_from_rfc3339(&issue.created_at)
            .context("Failed to parse created_at")?
            .with_timezone(&chrono::Utc);

        let updated_at = chrono::DateTime::parse_from_rfc3339(&issue.updated_at)
            .context("Failed to parse updated_at")?
            .with_timezone(&chrono::Utc);

        Ok(IssueDetails {
            id: issue.id.to_string(),
            number: issue.iid,
            title: issue.title,
            body: issue.description,
            state,
            labels: issue.labels,
            assignees: issue.assignees.into_iter().map(|u| u.username).collect(),
            url: issue.web_url,
            created_at,
            updated_at,
        })
    }

    /// Convert GitLab issue to `CreatedIssue`
    fn to_created_issue(issue: GitLabIssue) -> CreatedIssue {
        let state = if issue.state == "opened" {
            IssueState::Open
        } else {
            IssueState::Closed
        };

        CreatedIssue {
            id: issue.id.to_string(),
            number: issue.iid,
            url: issue.web_url,
            title: issue.title,
            state,
        }
    }

    // =========================================================================
    // Time Tracking Methods
    // =========================================================================

    /// Convert seconds to GitLab duration string format
    ///
    /// GitLab accepts duration strings like "1h30m", "2h", "45m", "1d", etc.
    ///
    /// # Examples
    /// - 3600 seconds → "1h"
    /// - 5400 seconds → "1h30m"
    /// - 1800 seconds → "30m"
    #[must_use]
    pub fn seconds_to_duration(seconds: u32) -> String {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;

        match (hours, minutes) {
            (0, 0) => "0m".to_string(),
            (0, m) => format!("{m}m"),
            (h, 0) => format!("{h}h"),
            (h, m) => format!("{h}h{m}m"),
        }
    }

    /// Add spent time to an issue
    ///
    /// # Arguments
    /// * `issue_iid` - Issue internal ID (the number shown in URLs, e.g., "123")
    /// * `duration_seconds` - Time spent in seconds
    /// * `summary` - Optional summary of work done (added as a note)
    ///
    /// # Errors
    /// Returns an error if the API request fails
    pub async fn add_spent_time(
        &self,
        issue_iid: &str,
        duration_seconds: u32,
        summary: Option<&str>,
    ) -> Result<()> {
        let url = format!(
            "{}/projects/{}/issues/{}/add_spent_time",
            self.api_base, self.project, issue_iid
        );

        let duration = Self::seconds_to_duration(duration_seconds);
        let request = GitLabAddSpentTimeRequest { duration };

        log::debug!("Adding spent time to GitLab issue {issue_iid}: {request:?}");

        self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send add spent time request")?
            .ensure_success("GitLab")
            .await?;

        // Optionally add a note with the summary
        if let Some(desc) = summary {
            if let Err(e) = self
                .add_note(
                    issue_iid,
                    &format!(
                        "Time logged: {} - {}",
                        Self::seconds_to_duration(duration_seconds),
                        desc
                    ),
                )
                .await
            {
                log::warn!("Failed to add time log note: {e}");
            }
        }

        log::info!(
            "Added {} to GitLab issue #{}",
            Self::seconds_to_duration(duration_seconds),
            issue_iid
        );

        Ok(())
    }

    /// Get time tracking statistics for an issue
    ///
    /// # Arguments
    /// * `issue_iid` - Issue internal ID
    ///
    /// # Errors
    /// Returns an error if the API request fails
    pub async fn get_time_stats(&self, issue_iid: &str) -> Result<GitLabTimeStats> {
        let url = format!(
            "{}/projects/{}/issues/{}/time_stats",
            self.api_base, self.project, issue_iid
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send get time stats request")?
            .ensure_success("GitLab")
            .await?;

        response
            .json()
            .await
            .context("Failed to parse time stats response")
    }

    /// Set time estimate for an issue
    ///
    /// # Arguments
    /// * `issue_iid` - Issue internal ID
    /// * `duration_seconds` - Estimated time in seconds
    ///
    /// # Errors
    /// Returns an error if the API request fails
    pub async fn set_time_estimate(&self, issue_iid: &str, duration_seconds: u32) -> Result<()> {
        let url = format!(
            "{}/projects/{}/issues/{}/time_estimate",
            self.api_base, self.project, issue_iid
        );

        let duration = Self::seconds_to_duration(duration_seconds);
        let request = GitLabTimeEstimateRequest { duration };

        self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send set time estimate request")?
            .ensure_success("GitLab")
            .await?;

        Ok(())
    }

    /// Add a note (comment) to an issue
    async fn add_note(&self, issue_iid: &str, body: &str) -> Result<()> {
        let url = format!(
            "{}/projects/{}/issues/{}/notes",
            self.api_base, self.project, issue_iid
        );

        self.client
            .post(&url)
            .json(&NoteRequest { body })
            .send()
            .await
            .context("Failed to send add note request")?
            .ensure_success("GitLab")
            .await?;

        Ok(())
    }
}

#[async_trait]
impl IssueManagement for GitLabClient {
    async fn create_issue(&self, request: &CreateIssueRequest) -> Result<CreatedIssue> {
        // Build description with source tracking if provided
        let description = if let (Some(source_id), Some(source_system)) =
            (&request.source_id, &request.source_system)
        {
            let tracking_footer =
                format!("\n\n---\n_Synced from {source_system}: `{source_id}`_");
            request
                .body
                .as_ref()
                .map(|b| format!("{b}{tracking_footer}"))
                .or(Some(tracking_footer))
        } else {
            request.body.clone()
        };

        // Convert labels to comma-separated string
        let labels = if request.labels.is_empty() {
            None
        } else {
            Some(request.labels.join(","))
        };

        let gitlab_request = GitLabCreateIssue {
            title: request.title.clone(),
            description,
            labels,
            assignee_ids: None, // Would need to resolve usernames to IDs
            milestone_id: request.milestone.as_ref().and_then(|m| m.parse().ok()),
            due_date: request.due_date.map(|d| d.format("%Y-%m-%d").to_string()),
        };

        let response = self
            .client
            .post(self.issues_url())
            .json(&gitlab_request)
            .send()
            .await
            .context("Failed to send create issue request")?
            .ensure_success("GitLab")
            .await?;

        let issue: GitLabIssue = response
            .json()
            .await
            .context("Failed to parse issue response")?;

        Ok(Self::to_created_issue(issue))
    }

    async fn update_issue(&self, issue_id: &str, update: &UpdateIssueRequest) -> Result<()> {
        let url = format!("{}/{issue_id}", self.issues_url());

        // Convert state to GitLab state_event
        let state_event = update.state.map(|s| match s {
            IssueState::Open => "reopen".to_string(),
            IssueState::Closed => "close".to_string(),
        });

        // Convert labels to comma-separated string
        let labels = update.labels.as_ref().map(|l| l.join(","));

        let gitlab_update = GitLabUpdateIssue {
            title: update.title.clone(),
            description: update.body.clone(),
            state_event,
            labels,
            assignee_ids: None, // Would need to resolve usernames to IDs
            milestone_id: update.milestone.as_ref().and_then(|m| m.parse().ok()),
        };

        self.client
            .put(&url)
            .json(&gitlab_update)
            .send()
            .await
            .context("Failed to send update issue request")?
            .ensure_success("GitLab")
            .await?;

        Ok(())
    }

    async fn get_issue(&self, issue_id: &str) -> Result<IssueDetails> {
        let url = format!("{}/{issue_id}", self.issues_url());

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send get issue request")?
            .ensure_success("GitLab")
            .await?;

        let issue: GitLabIssue = response
            .json()
            .await
            .context("Failed to parse issue response")?;

        Self::to_issue_details(issue)
    }

    async fn search_issues(&self, query: &str) -> Result<Vec<IssueDetails>> {
        let url = format!(
            "{}?search={}&per_page=100",
            self.issues_url(),
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send search request")?
            .ensure_success("GitLab")
            .await?;

        let issues: Vec<GitLabIssue> = response
            .json()
            .await
            .context("Failed to parse search response")?;

        issues.into_iter().map(Self::to_issue_details).collect()
    }

    async fn list_issues(&self, state: Option<IssueState>) -> Result<Vec<IssueDetails>> {
        let state_param = match state {
            Some(IssueState::Open) => "opened",
            Some(IssueState::Closed) => "closed",
            None => "all",
        };

        let url = format!("{}?state={}&per_page=100", self.issues_url(), state_param);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send list issues request")?
            .ensure_success("GitLab")
            .await?;

        let issues: Vec<GitLabIssue> = response
            .json()
            .await
            .context("Failed to parse issues response")?;

        issues.into_iter().map(Self::to_issue_details).collect()
    }

    async fn validate_credentials(&self) -> Result<bool> {
        // Try to get the project info to validate credentials
        let url = format!("{}/projects/{}", self.api_base, self.project);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send validation request")?;

        Ok(response.status().is_success())
    }

    fn system_name(&self) -> &'static str {
        "gitlab"
    }

    fn project_identifier(&self) -> &str {
        &self.project
    }
}

#[async_trait]
impl ProjectManagementSystem for GitLabClient {
    async fn fetch_work_item(&self, work_item_id: &str) -> Result<WorkItemDetails> {
        let issue = self.get_issue(work_item_id).await?;

        Ok(WorkItemDetails {
            id: work_item_id.to_string(),
            title: issue.title,
            description: issue.body,
            status: match issue.state {
                IssueState::Open => "open".to_string(),
                IssueState::Closed => "closed".to_string(),
            },
            project: Some(self.project.clone()),
            workspace: None,
        })
    }

    async fn add_time_entry(&self, entry: &TimeEntry) -> Result<()> {
        let description = format!("{} - {}", entry.category, entry.description);

        log::debug!(
            "Adding time entry to GitLab issue {}: {} seconds",
            entry.work_item_id,
            entry.duration_seconds
        );

        self.add_spent_time(&entry.work_item_id, entry.duration_seconds, Some(&description))
            .await
    }

    async fn batch_sync(&self, entries: Vec<TimeEntry>) -> Result<SyncReport> {
        let mut report = SyncReport::new(entries.len());

        for entry in entries {
            match self.add_time_entry(&entry).await {
                Ok(()) => report.record_success(),
                Err(e) => report.record_failure(format!("Issue {}: {e}", entry.work_item_id)),
            }
        }

        Ok(report)
    }

    async fn validate_credentials(&self) -> Result<bool> {
        // Delegate to IssueManagement::validate_credentials
        IssueManagement::validate_credentials(self).await
    }

    fn system_name(&self) -> &'static str {
        "gitlab"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_path_encoding() {
        // Test that project paths with slashes are properly encoded
        let client = GitLabClient::new("test-token", "group/project");
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.project, "group%2Fproject");
    }

    #[test]
    fn test_seconds_to_duration() {
        // Zero
        assert_eq!(GitLabClient::seconds_to_duration(0), "0m");

        // Minutes only
        assert_eq!(GitLabClient::seconds_to_duration(60), "1m");
        assert_eq!(GitLabClient::seconds_to_duration(1800), "30m");
        assert_eq!(GitLabClient::seconds_to_duration(3540), "59m");

        // Hours only
        assert_eq!(GitLabClient::seconds_to_duration(3600), "1h");
        assert_eq!(GitLabClient::seconds_to_duration(7200), "2h");
        assert_eq!(GitLabClient::seconds_to_duration(36000), "10h");

        // Hours and minutes
        assert_eq!(GitLabClient::seconds_to_duration(3660), "1h1m");
        assert_eq!(GitLabClient::seconds_to_duration(5400), "1h30m");
        assert_eq!(GitLabClient::seconds_to_duration(7260), "2h1m");
        assert_eq!(GitLabClient::seconds_to_duration(9000), "2h30m");

        // Edge cases - seconds are ignored (rounded down)
        assert_eq!(GitLabClient::seconds_to_duration(59), "0m");
        assert_eq!(GitLabClient::seconds_to_duration(3599), "59m");
        assert_eq!(GitLabClient::seconds_to_duration(3661), "1h1m");
    }
}
