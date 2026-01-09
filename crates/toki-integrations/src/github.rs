//! GitHub Issues API client
//!
//! Implements the `IssueManagement` trait for GitHub repositories.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};

use crate::http::ResponseExt;
use crate::traits::{
    CreateIssueRequest, CreatedIssue, IssueDetails, IssueManagement, IssueState,
    UpdateIssueRequest,
};

/// GitHub API client for issue management
pub struct GitHubClient {
    client: Client,
    /// Repository in "owner/repo" format
    repo: String,
    /// API base URL (for GitHub Enterprise support)
    api_base: String,
}

/// GitHub API issue response
#[derive(Debug, Deserialize)]
struct GitHubIssue {
    id: u64,
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    html_url: String,
    labels: Vec<GitHubLabel>,
    assignees: Vec<GitHubUser>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct GitHubLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    login: String,
}

/// GitHub API create issue request
#[derive(Debug, Serialize)]
struct GitHubCreateIssue {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    labels: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    assignees: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    milestone: Option<u64>,
}

/// GitHub API update issue request
#[derive(Debug, Serialize)]
struct GitHubUpdateIssue {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignees: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    milestone: Option<u64>,
}

/// GitHub search response
#[derive(Debug, Deserialize)]
struct GitHubSearchResponse {
    items: Vec<GitHubIssue>,
}

impl GitHubClient {
    /// Create a new GitHub client
    ///
    /// # Arguments
    /// * `token` - GitHub Personal Access Token or GitHub App token
    /// * `repo` - Repository in "owner/repo" format
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be created
    pub fn new(token: &str, repo: String) -> Result<Self> {
        Self::with_base_url(token, repo, "https://api.github.com")
    }

    /// Create a new GitHub client with custom API base URL (for GitHub Enterprise)
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be created
    pub fn with_base_url(token: &str, repo: String, api_base: &str) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Bearer {token}"))
                .context("Invalid token format")?,
        );
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            header::HeaderValue::from_static("2022-11-28"),
        );
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static("toki-time-tracker"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            repo,
            api_base: api_base.trim_end_matches('/').to_string(),
        })
    }

    /// Get the issues API URL
    fn issues_url(&self) -> String {
        format!("{}/repos/{}/issues", self.api_base, self.repo)
    }

    /// Convert GitHub issue to `IssueDetails`
    fn to_issue_details(issue: GitHubIssue) -> Result<IssueDetails> {
        let state = if issue.state == "open" {
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
            number: issue.number,
            title: issue.title,
            body: issue.body,
            state,
            labels: issue.labels.into_iter().map(|l| l.name).collect(),
            assignees: issue.assignees.into_iter().map(|u| u.login).collect(),
            url: issue.html_url,
            created_at,
            updated_at,
        })
    }

    /// Convert GitHub issue to `CreatedIssue`
    fn to_created_issue(issue: GitHubIssue) -> CreatedIssue {
        let state = if issue.state == "open" {
            IssueState::Open
        } else {
            IssueState::Closed
        };

        CreatedIssue {
            id: issue.id.to_string(),
            number: issue.number,
            url: issue.html_url,
            title: issue.title,
            state,
        }
    }
}

#[async_trait]
impl IssueManagement for GitHubClient {
    async fn create_issue(&self, request: &CreateIssueRequest) -> Result<CreatedIssue> {
        // Build body with source tracking if provided
        let body = if let (Some(source_id), Some(source_system)) =
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

        let github_request = GitHubCreateIssue {
            title: request.title.clone(),
            body,
            labels: request.labels.clone(),
            assignees: request.assignees.clone(),
            milestone: request.milestone.as_ref().and_then(|m| m.parse().ok()),
        };

        let response = self
            .client
            .post(self.issues_url())
            .json(&github_request)
            .send()
            .await
            .context("Failed to send create issue request")?
            .ensure_success("GitHub")
            .await?;

        let issue: GitHubIssue = response
            .json()
            .await
            .context("Failed to parse issue response")?;

        Ok(Self::to_created_issue(issue))
    }

    async fn update_issue(&self, issue_id: &str, update: &UpdateIssueRequest) -> Result<()> {
        let url = format!("{}/{issue_id}", self.issues_url());

        let github_update = GitHubUpdateIssue {
            title: update.title.clone(),
            body: update.body.clone(),
            state: update.state.map(|s| match s {
                IssueState::Open => "open".to_string(),
                IssueState::Closed => "closed".to_string(),
            }),
            labels: update.labels.clone(),
            assignees: update.assignees.clone(),
            milestone: update.milestone.as_ref().and_then(|m| m.parse().ok()),
        };

        self.client
            .patch(&url)
            .json(&github_update)
            .send()
            .await
            .context("Failed to send update issue request")?
            .ensure_success("GitHub")
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
            .ensure_success("GitHub")
            .await?;

        let issue: GitHubIssue = response
            .json()
            .await
            .context("Failed to parse issue response")?;

        Self::to_issue_details(issue)
    }

    async fn search_issues(&self, query: &str) -> Result<Vec<IssueDetails>> {
        // GitHub search API
        let search_query = format!("repo:{} is:issue {}", self.repo, query);
        let url = format!(
            "{}/search/issues?q={}",
            self.api_base,
            urlencoding::encode(&search_query)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send search request")?
            .ensure_success("GitHub")
            .await?;

        let search_response: GitHubSearchResponse = response
            .json()
            .await
            .context("Failed to parse search response")?;

        search_response
            .items
            .into_iter()
            .map(Self::to_issue_details)
            .collect()
    }

    async fn list_issues(&self, state: Option<IssueState>) -> Result<Vec<IssueDetails>> {
        let state_param = match state {
            Some(IssueState::Open) => "open",
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
            .ensure_success("GitHub")
            .await?;

        let issues: Vec<GitHubIssue> = response
            .json()
            .await
            .context("Failed to parse issues response")?;

        issues.into_iter().map(Self::to_issue_details).collect()
    }

    async fn validate_credentials(&self) -> Result<bool> {
        // Try to get the repo info to validate credentials
        let url = format!("{}/repos/{}", self.api_base, self.repo);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send validation request")?;

        Ok(response.status().is_success())
    }

    fn system_name(&self) -> &'static str {
        "github"
    }

    fn project_identifier(&self) -> &str {
        &self.repo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_issue_request_builder() {
        let request = CreateIssueRequest::new("Test issue".to_string())
            .with_body("Test body".to_string())
            .with_label("bug".to_string())
            .with_source("notion-page-123".to_string(), "notion".to_string());

        assert_eq!(request.title, "Test issue");
        assert_eq!(request.body, Some("Test body".to_string()));
        assert_eq!(request.labels, vec!["bug"]);
        assert_eq!(request.source_id, Some("notion-page-123".to_string()));
        assert_eq!(request.source_system, Some("notion".to_string()));
    }
}
