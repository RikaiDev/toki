//! Notion to GitHub/GitLab issue sync service
//!
//! Syncs Notion database pages to GitHub or GitLab issues,
//! tracking sync state to avoid duplicates.

use std::sync::Arc;

use anyhow::{Context, Result};
use toki_integrations::notion::{NotionClient, NotionIssueCandidateData, PropertyMappingConfig};
use toki_integrations::traits::{CreatedIssue, IssueManagement, IssueSyncReport};
use toki_storage::db::Database;
use toki_storage::models::SyncedIssue;

use crate::notion_mapper::{IssueMappingConfig, NotionIssueMapper};

/// Target system for issue sync
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTarget {
    GitHub,
    GitLab,
}

impl SyncTarget {
    /// Get the system name as a string
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
        }
    }
}

/// Options for the sync operation
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct SyncOptions {
    /// Dry run - don't actually create issues
    pub dry_run: bool,
    /// Force sync even if already synced
    pub force: bool,
    /// Maximum number of issues to create in one run
    pub limit: Option<usize>,
    /// Status filter - only sync pages with these statuses
    pub status_filter: Vec<String>,
}


impl SyncOptions {
    /// Create default options for a dry run
    #[must_use]
    pub fn dry_run() -> Self {
        Self {
            dry_run: true,
            ..Default::default()
        }
    }
}

/// Result of a single issue sync operation
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Source Notion page ID
    pub page_id: String,
    /// Issue title
    pub title: String,
    /// Result of the sync
    pub outcome: SyncOutcome,
}

/// Outcome of a sync operation
#[derive(Debug, Clone)]
pub enum SyncOutcome {
    /// Issue was created successfully
    Created {
        issue_number: u64,
        issue_url: String,
    },
    /// Issue was skipped (already synced)
    Skipped { reason: String },
    /// Issue creation failed
    Failed { error: String },
    /// Would be created (dry run)
    WouldCreate,
}

/// Service for syncing Notion pages to GitHub/GitLab issues
pub struct NotionIssueSyncService {
    notion_client: Arc<NotionClient>,
    db: Arc<Database>,
    mapper: NotionIssueMapper,
}

impl NotionIssueSyncService {
    /// Create a new sync service
    #[must_use]
    pub fn new(notion_client: Arc<NotionClient>, db: Arc<Database>) -> Self {
        Self {
            notion_client,
            db,
            mapper: NotionIssueMapper::new(),
        }
    }

    /// Create a new sync service with custom mapping config
    #[must_use]
    pub fn with_config(
        notion_client: Arc<NotionClient>,
        db: Arc<Database>,
        mapping_config: IssueMappingConfig,
    ) -> Self {
        Self {
            notion_client,
            db,
            mapper: NotionIssueMapper::with_config(mapping_config),
        }
    }

    /// Sync a Notion database to GitHub/GitLab
    ///
    /// # Arguments
    /// * `database_id` - Notion database ID
    /// * `target_client` - GitHub or GitLab client
    /// * `target_project` - Target project identifier (e.g., "owner/repo")
    /// * `property_config` - Optional property mapping configuration
    /// * `options` - Sync options
    ///
    /// # Errors
    ///
    /// Returns an error if fetching from Notion or creating issues fails
    pub async fn sync_database<T: IssueManagement>(
        &self,
        database_id: &str,
        target_client: &T,
        target_project: &str,
        property_config: Option<&PropertyMappingConfig>,
        options: &SyncOptions,
    ) -> Result<(IssueSyncReport, Vec<SyncResult>)> {
        let target_system = target_client.system_name();

        log::info!(
            "Starting sync from Notion database {database_id} to {target_system} project {target_project}"
        );

        // Fetch all pages from Notion database as issue candidates
        let candidates = self
            .notion_client
            .fetch_database_as_issues(database_id, property_config, false)
            .await
            .context("Failed to fetch Notion pages")?;

        log::info!("Fetched {} pages from Notion database", candidates.len());

        // Filter candidates
        let filtered: Vec<_> = candidates
            .iter()
            .filter(|c| {
                // Apply status filter from options
                if !options.status_filter.is_empty() {
                    return options.status_filter.iter().any(|s| s.eq_ignore_ascii_case(&c.status));
                }
                // Otherwise use mapper's default filtering
                self.mapper.should_sync(&c.status)
            })
            .collect();

        log::info!(
            "Filtered to {} pages eligible for sync",
            filtered.len()
        );

        // Apply limit
        let to_process: Vec<_> = if let Some(limit) = options.limit {
            filtered.into_iter().take(limit).collect()
        } else {
            filtered
        };

        let mut report = IssueSyncReport::new();
        let mut results = Vec::new();

        for candidate in to_process {
            let result = self
                .sync_single_page(
                    candidate,
                    database_id,
                    target_client,
                    target_project,
                    target_system,
                    options,
                )
                .await;

            match &result.outcome {
                SyncOutcome::Created { .. } => report.record_created(),
                SyncOutcome::Skipped { .. } => report.record_skipped(),
                SyncOutcome::Failed { error } => report.record_failure(error.clone()),
                SyncOutcome::WouldCreate => report.record_skipped(), // Dry run counts as skipped
            }

            results.push(result);
        }

        log::info!(
            "Sync complete: {} created, {} skipped, {} failed",
            report.created,
            report.skipped,
            report.failed
        );

        Ok((report, results))
    }

    /// Sync a single Notion page to an issue
    async fn sync_single_page<T: IssueManagement>(
        &self,
        candidate: &NotionIssueCandidateData,
        database_id: &str,
        target_client: &T,
        target_project: &str,
        target_system: &str,
        options: &SyncOptions,
    ) -> SyncResult {
        let page_id = &candidate.page_id;
        let title = candidate.title.clone();

        // Check if already synced (unless force)
        if !options.force {
            match self.db.is_page_synced(page_id, target_system, target_project) {
                Ok(true) => {
                    log::debug!("Page {page_id} already synced, skipping");
                    return SyncResult {
                        page_id: page_id.clone(),
                        title,
                        outcome: SyncOutcome::Skipped {
                            reason: "Already synced".to_string(),
                        },
                    };
                }
                Ok(false) => {}
                Err(e) => {
                    log::warn!("Failed to check sync status for {page_id}: {e}");
                }
            }
        }

        // Dry run - just report what would happen
        if options.dry_run {
            log::info!("[DRY RUN] Would create issue: {title}");
            return SyncResult {
                page_id: page_id.clone(),
                title,
                outcome: SyncOutcome::WouldCreate,
            };
        }

        // Map to issue request
        let issue_request = self.mapper.map_to_issue_request(candidate);

        // Create the issue
        match target_client.create_issue(&issue_request).await {
            Ok(created) => {
                log::info!(
                    "Created issue #{} in {}: {}",
                    created.number,
                    target_system,
                    created.title
                );

                // Record the sync
                if let Err(e) = self.record_sync(
                    page_id,
                    database_id,
                    target_system,
                    target_project,
                    &created,
                ) {
                    log::error!("Failed to record sync: {e}");
                }

                SyncResult {
                    page_id: page_id.clone(),
                    title,
                    outcome: SyncOutcome::Created {
                        issue_number: created.number,
                        issue_url: created.url,
                    },
                }
            }
            Err(e) => {
                log::error!("Failed to create issue for {title}: {e}");
                SyncResult {
                    page_id: page_id.clone(),
                    title,
                    outcome: SyncOutcome::Failed {
                        error: e.to_string(),
                    },
                }
            }
        }
    }

    /// Record a successful sync in the database
    fn record_sync(
        &self,
        page_id: &str,
        database_id: &str,
        target_system: &str,
        target_project: &str,
        created: &CreatedIssue,
    ) -> Result<()> {
        let synced = SyncedIssue::new(
            page_id.to_string(),
            database_id.to_string(),
            target_system.to_string(),
            target_project.to_string(),
            created.id.clone(),
            created.number,
            created.url.clone(),
            created.title.clone(),
        );

        self.db
            .upsert_synced_issue(&synced)
            .context("Failed to save sync record")?;

        Ok(())
    }

    /// Get sync history for a database
    ///
    /// # Errors
    ///
    /// Returns an error if database query fails
    pub fn get_sync_history(&self, database_id: &str) -> Result<Vec<SyncedIssue>> {
        self.db
            .get_synced_issues_for_database(database_id)
            .context("Failed to get sync history")
    }

    /// Get sync history for a target project
    ///
    /// # Errors
    ///
    /// Returns an error if database query fails
    pub fn get_target_sync_history(
        &self,
        target_system: &str,
        target_project: &str,
    ) -> Result<Vec<SyncedIssue>> {
        self.db
            .get_synced_issues_for_target(target_system, target_project)
            .context("Failed to get target sync history")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_target_as_str() {
        assert_eq!(SyncTarget::GitHub.as_str(), "github");
        assert_eq!(SyncTarget::GitLab.as_str(), "gitlab");
    }

    #[test]
    fn test_sync_options_default() {
        let opts = SyncOptions::default();
        assert!(!opts.dry_run);
        assert!(!opts.force);
        assert!(opts.limit.is_none());
        assert!(opts.status_filter.is_empty());
    }

    #[test]
    fn test_sync_options_dry_run() {
        let opts = SyncOptions::dry_run();
        assert!(opts.dry_run);
        assert!(!opts.force);
    }
}
