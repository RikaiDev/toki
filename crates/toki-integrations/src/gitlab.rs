//! GitLab Issues API client
//!
//! Implements the `IssueManagement` trait for GitLab projects.
//! Supports both GitLab.com and self-hosted GitLab instances.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};

use crate::traits::{
    CreateIssueRequest, CreatedIssue, IssueDetails, IssueManagement, IssueState,
    UpdateIssueRequest,
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

impl GitLabClient {
    /// Create a new GitLab client for gitlab.com
    ///
    /// # Arguments
    /// * `token` - GitLab Personal Access Token
    /// * `project` - Project ID or path (e.g., "123" or "group/project")
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be created
    pub fn new(token: String, project: String) -> Result<Self> {
        Self::with_base_url(token, project, "https://gitlab.com/api/v4".to_string())
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
    pub fn with_base_url(token: String, project: String, api_base: String) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            "PRIVATE-TOKEN",
            header::HeaderValue::from_str(&token).context("Invalid token format")?,
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
        let encoded_project = urlencoding::encode(&project).into_owned();

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
            .context("Failed to send create issue request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("GitLab API error ({status}): {error_text}");
        }

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

        let response = self
            .client
            .put(&url)
            .json(&gitlab_update)
            .send()
            .await
            .context("Failed to send update issue request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("GitLab API error ({status}): {error_text}");
        }

        Ok(())
    }

    async fn get_issue(&self, issue_id: &str) -> Result<IssueDetails> {
        let url = format!("{}/{issue_id}", self.issues_url());

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send get issue request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("GitLab API error ({status}): {error_text}");
        }

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
            .context("Failed to send search request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("GitLab API error ({status}): {error_text}");
        }

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
            .context("Failed to send list issues request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("GitLab API error ({status}): {error_text}");
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_path_encoding() {
        // Test that project paths with slashes are properly encoded
        let client = GitLabClient::new("test-token".to_string(), "group/project".to_string());
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.project, "group%2Fproject");
    }
}
