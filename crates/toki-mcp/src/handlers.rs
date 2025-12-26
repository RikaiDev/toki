//! MCP Tool handlers for Toki
//!
//! Implements the actual functionality for each tool.

use std::sync::Arc;

use anyhow::Context;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo, Implementation},
    schemars, tool, tool_handler, tool_router,
    handler::server::wrapper::Parameters,
};
use toki_ai::{NotionIssueSyncService, SyncOptions, SyncOutcome};
use toki_integrations::{GitHubClient, GitLabClient, NotionClient};
use toki_storage::{Database, IntegrationConfig};

/// Request for listing pages in a Notion database
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListPagesRequest {
    #[schemars(description = "The Notion database ID")]
    pub database_id: String,
}

/// Request for syncing to GitHub
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SyncToGitHubRequest {
    #[schemars(description = "The Notion database ID")]
    pub database_id: String,
    #[schemars(description = "GitHub repository (owner/repo)")]
    pub repo: String,
}

/// Request for syncing to GitLab
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SyncToGitLabRequest {
    #[schemars(description = "The Notion database ID")]
    pub database_id: String,
    #[schemars(description = "GitLab project ID or path")]
    pub project: String,
}

/// Request for sync status
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SyncStatusRequest {
    #[schemars(description = "The Notion database ID")]
    pub database_id: String,
}

/// Request for getting a configuration value
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ConfigGetRequest {
    #[schemars(description = "Configuration key (e.g., notion.api_key)")]
    pub key: String,
}

/// Request for setting a configuration value
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ConfigSetRequest {
    #[schemars(description = "Configuration key (e.g., notion.api_key)")]
    pub key: String,
    #[schemars(description = "Value to set")]
    pub value: String,
}

/// Toki MCP Service
#[derive(Clone)]
pub struct TokiService {
    db: Arc<Database>,
    tool_router: ToolRouter<Self>,
}

impl TokiService {
    /// Create a new Toki service
    pub fn new() -> anyhow::Result<Self> {
        let db = Database::new(None).context("Failed to open database")?;
        Ok(Self {
            db: Arc::new(db),
            tool_router: Self::tool_router(),
        })
    }

    /// Get Notion client if configured
    fn get_notion_client(&self) -> anyhow::Result<NotionClient> {
        let config = self
            .db
            .get_integration_config("notion")?
            .ok_or_else(|| anyhow::anyhow!("Notion not configured. Set notion.api_key first."))?;

        NotionClient::new(config.api_key).context("Failed to create Notion client")
    }

    /// Get GitHub client if configured
    fn get_github_client(&self, repo: &str) -> anyhow::Result<GitHubClient> {
        let config = self
            .db
            .get_integration_config("github")?
            .ok_or_else(|| anyhow::anyhow!("GitHub not configured. Set github.token first."))?;

        GitHubClient::new(config.api_key, repo.to_string()).context("Failed to create GitHub client")
    }

    /// Get GitLab client if configured
    fn get_gitlab_client(&self, project: &str, api_url: Option<&str>) -> anyhow::Result<GitLabClient> {
        let config = self
            .db
            .get_integration_config("gitlab")?
            .ok_or_else(|| anyhow::anyhow!("GitLab not configured. Set gitlab.token first."))?;

        let client = if let Some(url) = api_url {
            GitLabClient::with_base_url(config.api_key, project.to_string(), url.to_string())
        } else if !config.api_url.is_empty() {
            GitLabClient::with_base_url(config.api_key, project.to_string(), config.api_url)
        } else {
            GitLabClient::new(config.api_key, project.to_string())
        };

        client.context("Failed to create GitLab client")
    }

    fn format_error(e: anyhow::Error) -> McpError {
        McpError::internal_error(e.to_string(), None)
    }
}

#[tool_router]
impl TokiService {
    /// List all accessible Notion databases
    #[tool(description = "List all accessible Notion databases. Returns database IDs and titles.")]
    async fn notion_list_databases(&self) -> Result<CallToolResult, McpError> {
        let client = self.get_notion_client().map_err(Self::format_error)?;

        let databases = client
            .list_databases()
            .await
            .map_err(|e| Self::format_error(e.into()))?;

        if databases.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No databases found. Make sure your Notion integration is connected to the databases."
            )]));
        }

        let mut result = String::from("Available Notion databases:\n\n");
        for db in &databases {
            let title = db
                .title
                .first()
                .map(|t| t.plain_text.as_str())
                .unwrap_or("Untitled");
            result.push_str(&format!("- {} (ID: {})\n", title, db.id));
        }

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// List pages in a Notion database
    #[tool(description = "List pages in a Notion database with their titles and statuses.")]
    async fn notion_list_pages(
        &self,
        Parameters(req): Parameters<ListPagesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.get_notion_client().map_err(Self::format_error)?;

        let pages = client
            .query_database_all(&req.database_id)
            .await
            .map_err(|e| Self::format_error(e.into()))?;

        if pages.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No pages found.")]));
        }

        let db_info = client.get_database(&req.database_id).await
            .map_err(|e| Self::format_error(e.into()))?;
        let mapping = db_info.detect_property_mapping(None);

        let mut result = format!("Pages in database ({} total):\n\n", pages.len());
        for page in &pages {
            let title = mapping.title.as_ref()
                .and_then(|prop| page.properties.get(prop))
                .and_then(|v| v.as_plain_text())
                .unwrap_or_else(|| "Untitled".to_string());
            let status = mapping.status.as_ref()
                .and_then(|prop| page.properties.get(prop))
                .and_then(|v| v.as_select_name())
                .unwrap_or_else(|| "-".to_string());
            let external_id = NotionClient::generate_external_id(&req.database_id, &page.id);
            result.push_str(&format!("- {} [{}] (ID: {})\n", title, status, external_id));
        }
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Sync Notion database to GitHub Issues
    #[tool(description = "Sync Notion database pages to GitHub Issues. Creates issues for pages that haven't been synced.")]
    async fn notion_sync_to_github(
        &self,
        Parameters(req): Parameters<SyncToGitHubRequest>,
    ) -> Result<CallToolResult, McpError> {
        let notion_client = Arc::new(self.get_notion_client().map_err(Self::format_error)?);
        let github_client = self.get_github_client(&req.repo).map_err(Self::format_error)?;
        let sync_service = NotionIssueSyncService::new(notion_client, self.db.clone());

        let options = SyncOptions::default();

        let (report, results) = sync_service
            .sync_database(&req.database_id, &github_client, &req.repo, None, &options)
            .await
            .map_err(|e| Self::format_error(e.into()))?;

        let mut output = String::from("Sync completed:\n\n");
        for result in &results {
            match &result.outcome {
                SyncOutcome::Created { issue_number, issue_url } => {
                    output.push_str(&format!("[CREATED] #{} {} -> {}\n", issue_number, result.title, issue_url));
                }
                SyncOutcome::Skipped { reason } => {
                    output.push_str(&format!("[SKIPPED] {} - {}\n", result.title, reason));
                }
                SyncOutcome::Failed { error } => {
                    output.push_str(&format!("[FAILED] {} - {}\n", result.title, error));
                }
                SyncOutcome::WouldCreate => {
                    output.push_str(&format!("[WOULD CREATE] {}\n", result.title));
                }
            }
        }
        output.push_str(&format!("\nSummary: {} created, {} skipped, {} failed", report.created, report.skipped, report.failed));
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Sync Notion database to GitLab Issues
    #[tool(description = "Sync Notion database pages to GitLab Issues. Creates issues for pages that haven't been synced.")]
    async fn notion_sync_to_gitlab(
        &self,
        Parameters(req): Parameters<SyncToGitLabRequest>,
    ) -> Result<CallToolResult, McpError> {
        let notion_client = Arc::new(self.get_notion_client().map_err(Self::format_error)?);
        let gitlab_client = self.get_gitlab_client(&req.project, None).map_err(Self::format_error)?;
        let sync_service = NotionIssueSyncService::new(notion_client, self.db.clone());

        let options = SyncOptions::default();

        let (report, results) = sync_service
            .sync_database(&req.database_id, &gitlab_client, &req.project, None, &options)
            .await
            .map_err(|e| Self::format_error(e.into()))?;

        let mut output = String::from("Sync completed:\n\n");
        for result in &results {
            match &result.outcome {
                SyncOutcome::Created { issue_number, issue_url } => {
                    output.push_str(&format!("[CREATED] #{} {} -> {}\n", issue_number, result.title, issue_url));
                }
                SyncOutcome::Skipped { reason } => {
                    output.push_str(&format!("[SKIPPED] {} - {}\n", result.title, reason));
                }
                SyncOutcome::Failed { error } => {
                    output.push_str(&format!("[FAILED] {} - {}\n", result.title, error));
                }
                SyncOutcome::WouldCreate => {
                    output.push_str(&format!("[WOULD CREATE] {}\n", result.title));
                }
            }
        }
        output.push_str(&format!("\nSummary: {} created, {} skipped, {} failed", report.created, report.skipped, report.failed));
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Show sync history for a Notion database
    #[tool(description = "Show sync history for a Notion database - which pages have been synced to which issues.")]
    async fn notion_sync_status(
        &self,
        Parameters(req): Parameters<SyncStatusRequest>,
    ) -> Result<CallToolResult, McpError> {
        let synced = self.db
            .get_synced_issues_for_database(&req.database_id)
            .map_err(|e| Self::format_error(e.into()))?;

        if synced.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                format!("No sync history found for database {}.", req.database_id)
            )]));
        }

        let mut result = format!("Sync history ({} issues synced):\n\n", synced.len());
        for issue in &synced {
            result.push_str(&format!("- #{} [{}] {} -> {}\n",
                issue.target_issue_number, issue.target_system, issue.title, issue.target_issue_url));
        }
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// List tracked projects
    #[tool(description = "List all tracked projects with their PM system links.")]
    async fn project_list(&self) -> Result<CallToolResult, McpError> {
        let projects = self.db.get_all_projects()
            .map_err(|e| Self::format_error(e.into()))?;

        if projects.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No projects found. Projects are automatically detected when you work in a directory."
            )]));
        }

        let mut result = format!("Tracked projects ({}):\n\n", projects.len());
        for project in &projects {
            result.push_str(&format!("- {} ({})\n", project.name, project.path));
            if let Some(pm) = &project.pm_system {
                result.push_str(&format!("  PM: {} - {}\n", pm, project.pm_project_id.as_deref().unwrap_or("-")));
            }
        }
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Get a configuration value
    #[tool(description = "Get a configuration value. Keys: notion.api_key, github.token, gitlab.token")]
    async fn config_get(
        &self,
        Parameters(req): Parameters<ConfigGetRequest>,
    ) -> Result<CallToolResult, McpError> {
        let parts: Vec<&str> = req.key.split('.').collect();
        if parts.len() != 2 {
            return Ok(CallToolResult::success(vec![Content::text("Key must be in section.key format")]));
        }

        let (section, field) = (parts[0], parts[1]);
        match section {
            "notion" | "github" | "gitlab" | "plane" => {
                if let Ok(Some(config)) = self.db.get_integration_config(section) {
                    let value = match field {
                        "api_key" | "token" => format!("{}***", &config.api_key.chars().take(8).collect::<String>()),
                        "api_url" | "url" => config.api_url,
                        _ => return Ok(CallToolResult::success(vec![Content::text(format!("Unknown field: {field}"))])),
                    };
                    Ok(CallToolResult::success(vec![Content::text(format!("{} = {}", req.key, value))]))
                } else {
                    Ok(CallToolResult::success(vec![Content::text(format!("{} is not set", req.key))]))
                }
            }
            _ => Ok(CallToolResult::success(vec![Content::text(format!("Unknown section: {section}"))])),
        }
    }

    /// Set a configuration value
    #[tool(description = "Set a configuration value. Keys: notion.api_key, github.token, gitlab.token")]
    async fn config_set(
        &self,
        Parameters(req): Parameters<ConfigSetRequest>,
    ) -> Result<CallToolResult, McpError> {
        let parts: Vec<&str> = req.key.split('.').collect();
        if parts.len() != 2 {
            return Ok(CallToolResult::success(vec![Content::text("Key must be in section.key format")]));
        }

        let (section, field) = (parts[0], parts[1]);
        match section {
            "notion" | "github" | "gitlab" | "plane" => {
                let mut config = self.db.get_integration_config(section)
                    .map_err(Self::format_error)?
                    .unwrap_or_else(|| IntegrationConfig::new(section.to_string(), String::new(), String::new()));

                match field {
                    "api_key" | "token" => config.api_key = req.value.clone(),
                    "api_url" | "url" => config.api_url = req.value.clone(),
                    _ => return Ok(CallToolResult::success(vec![Content::text(format!("Unknown field: {field}"))])),
                }

                config.updated_at = chrono::Utc::now();
                self.db.upsert_integration_config(&config).map_err(Self::format_error)?;
                Ok(CallToolResult::success(vec![Content::text(format!("Set {} successfully", req.key))]))
            }
            _ => Ok(CallToolResult::success(vec![Content::text(format!("Unknown section: {section}"))])),
        }
    }
}

#[tool_handler]
impl ServerHandler for TokiService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Toki MCP server for AI agents to interact with time tracking and Notion sync.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            ..Default::default()
        }
    }
}
