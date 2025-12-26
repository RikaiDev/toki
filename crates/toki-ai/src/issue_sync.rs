//! Issue synchronization service for AI matching
//!
//! Syncs issues from PM systems (Plane.so, Notion) to local database
//! and computes embeddings for semantic matching.

use anyhow::Result;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use toki_integrations::plane::PlaneClient;
use toki_integrations::notion::{NotionClient, PropertyMappingConfig};
use toki_storage::db::Database;
use toki_storage::models::{IssueCandidate, Project};

use crate::embedding::EmbeddingService;

/// Statistics from issue sync operation
#[derive(Debug, Default)]
pub struct SyncStats {
    pub issues_synced: usize,
    pub issues_updated: usize,
    pub embeddings_computed: usize,
    pub errors: Vec<String>,
}

impl std::fmt::Display for SyncStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Synced: {}, Updated: {}, Embeddings: {}",
            self.issues_synced, self.issues_updated, self.embeddings_computed
        )?;
        if !self.errors.is_empty() {
            write!(f, ", Errors: {}", self.errors.len())?;
        }
        Ok(())
    }
}

/// Service for syncing issues from PM systems and computing embeddings
pub struct IssueSyncService {
    embedding_service: Arc<Mutex<EmbeddingService>>,
    database: Arc<Database>,
}

impl IssueSyncService {
    /// Create a new issue sync service
    ///
    /// # Errors
    ///
    /// Returns an error if the embedding service fails to initialize
    pub fn new(database: Arc<Database>) -> Result<Self> {
        let embedding_service = EmbeddingService::new()?;
        Ok(Self {
            embedding_service: Arc::new(Mutex::new(embedding_service)),
            database,
        })
    }

    /// Create with an existing embedding service (for sharing across components)
    #[must_use]
    pub fn with_embedding_service(
        database: Arc<Database>,
        embedding_service: Arc<Mutex<EmbeddingService>>,
    ) -> Self {
        Self {
            embedding_service,
            database,
        }
    }

    /// Sync issues from Plane.so for a specific project
    ///
    /// # Errors
    ///
    /// Returns an error if API calls fail or database operations fail
    pub async fn sync_project_issues(
        &self,
        plane_client: &PlaneClient,
        local_project: &Project,
    ) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        // Get the PM project ID from local project
        let pm_project_id = if let Some(id) = &local_project.pm_project_id { id.clone() } else {
            stats.errors.push(format!(
                "Project '{}' has no PM project ID linked",
                local_project.name
            ));
            return Ok(stats);
        };

        // Parse the PM project ID as UUID
        let project_uuid: Uuid = pm_project_id.parse().map_err(|_| {
            anyhow::anyhow!("Invalid PM project ID: {pm_project_id}")
        })?;

        log::info!(
            "Syncing issues from Plane project {} for local project '{}'",
            pm_project_id,
            local_project.name
        );

        // Fetch project details to get the identifier (e.g., "HYGIE")
        let plane_project = plane_client.get_project(&project_uuid).await?;
        let project_identifier = &plane_project.identifier;

        log::debug!(
            "Project identifier: {} ({})",
            project_identifier,
            plane_project.name
        );

        // Fetch states to build state_id -> state_name mapping
        let states = plane_client.list_states(&project_uuid).await?;
        let state_map: std::collections::HashMap<Uuid, String> = states
            .into_iter()
            .map(|s| (s.id, s.name))
            .collect();

        log::debug!("Loaded {} states for project", state_map.len());

        // Fetch all pages of work items
        let mut cursor: Option<String> = None;
        let mut all_items = Vec::new();

        loop {
            let response = plane_client
                .list_work_items(&project_uuid, cursor.as_deref())
                .await?;

            all_items.extend(response.results);

            if response.next_page_results {
                cursor = response.next_cursor;
            } else {
                break;
            }
        }

        log::info!("Fetched {} work items from Plane", all_items.len());

        // Process each work item
        for item in &all_items {
            let candidate_data =
                PlaneClient::work_item_to_issue_candidate(item, Some(project_identifier), Some(&state_map));

            // Check if we need to update or insert
            let existing = self
                .database
                .get_issue_candidate(&candidate_data.external_id, "plane")?;

            let needs_embedding = existing
                .as_ref()
                .is_none_or(|e| e.embedding.is_none());

            // Create IssueCandidate
            let mut candidate = IssueCandidate::new(
                local_project.id,
                candidate_data.external_id.clone(),
                candidate_data.external_system,
                candidate_data.title,
            );
            candidate.pm_project_id = Some(pm_project_id.clone());
            candidate.description = candidate_data.description;
            candidate.status = candidate_data.status;
            candidate.labels = candidate_data.labels;

            // Preserve existing ID if updating
            if let Some(existing_candidate) = &existing {
                candidate.id = existing_candidate.id;
                stats.issues_updated += 1;
            } else {
                stats.issues_synced += 1;
            }

            // Upsert to database
            self.database.upsert_issue_candidate(&candidate)?;

            // Compute embedding if needed
            if needs_embedding {
                match self.compute_and_store_embedding(&candidate) {
                    Ok(()) => stats.embeddings_computed += 1,
                    Err(e) => {
                        stats.errors.push(format!(
                            "Failed to compute embedding for {}: {e}",
                            candidate.external_id
                        ));
                    }
                }
            }
        }

        log::info!("Issue sync complete: {stats}");
        Ok(stats)
    }

    /// Sync issues from Notion for a specific project
    ///
    /// # Arguments
    /// * `notion_client` - Notion API client
    /// * `local_project` - Local project with Notion database linked
    /// * `config` - Optional property mapping configuration
    /// * `fetch_blocks` - Whether to fetch page blocks for descriptions
    ///
    /// # Errors
    ///
    /// Returns an error if API calls fail or database operations fail
    pub async fn sync_notion_project_issues(
        &self,
        notion_client: &NotionClient,
        local_project: &Project,
        config: Option<&PropertyMappingConfig>,
        fetch_blocks: bool,
    ) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        // Get the database ID from local project
        let database_id = if let Some(id) = &local_project.pm_project_id {
            id.clone()
        } else {
            stats.errors.push(format!(
                "Project '{}' has no Notion database ID linked",
                local_project.name
            ));
            return Ok(stats);
        };

        log::info!(
            "Syncing issues from Notion database {} for local project '{}'",
            database_id,
            local_project.name
        );

        // Fetch all pages as issue candidates
        let candidates = notion_client
            .fetch_database_as_issues(&database_id, config, fetch_blocks)
            .await?;

        log::info!("Fetched {} pages from Notion", candidates.len());

        // Process each candidate
        for candidate_data in &candidates {
            // Check if we need to update or insert
            let existing = self
                .database
                .get_issue_candidate(&candidate_data.external_id, "notion")?;

            let needs_embedding = existing.as_ref().is_none_or(|e| e.embedding.is_none());

            // Create IssueCandidate
            let mut candidate = IssueCandidate::new(
                local_project.id,
                candidate_data.external_id.clone(),
                candidate_data.external_system.clone(),
                candidate_data.title.clone(),
            );
            candidate.pm_project_id = Some(database_id.clone());
            candidate.description = candidate_data.description.clone();
            candidate.status = candidate_data.status.clone();
            candidate.labels = candidate_data.labels.clone();

            // Preserve existing ID if updating
            if let Some(existing_candidate) = &existing {
                candidate.id = existing_candidate.id;
                stats.issues_updated += 1;
            } else {
                stats.issues_synced += 1;
            }

            // Upsert to database
            self.database.upsert_issue_candidate(&candidate)?;

            // Compute embedding if needed
            if needs_embedding {
                match self.compute_and_store_embedding(&candidate) {
                    Ok(()) => stats.embeddings_computed += 1,
                    Err(e) => {
                        stats.errors.push(format!(
                            "Failed to compute embedding for {}: {e}",
                            candidate.external_id
                        ));
                    }
                }
            }
        }

        log::info!("Notion issue sync complete: {stats}");
        Ok(stats)
    }

    /// Sync all projects that have PM links (Plane or Notion)
    ///
    /// # Arguments
    /// * `plane_client` - Optional Plane.so client
    /// * `notion_client` - Optional Notion client
    ///
    /// # Errors
    ///
    /// Returns an error if API calls fail or database operations fail
    pub async fn sync_all_linked_projects_multi(
        &self,
        plane_client: Option<&PlaneClient>,
        notion_client: Option<&NotionClient>,
    ) -> Result<SyncStats> {
        let mut total_stats = SyncStats::default();

        let linked_projects = self.database.get_projects_with_pm_link()?;

        if linked_projects.is_empty() {
            log::info!("No projects with PM links found");
            return Ok(total_stats);
        }

        log::info!("Syncing {} linked projects", linked_projects.len());

        for project in &linked_projects {
            let result = match project.pm_system.as_deref() {
                Some("plane") => {
                    if let Some(client) = plane_client {
                        self.sync_project_issues(client, project).await
                    } else {
                        log::debug!(
                            "Skipping Plane project '{}': no Plane client provided",
                            project.name
                        );
                        continue;
                    }
                }
                Some("notion") => {
                    if let Some(client) = notion_client {
                        self.sync_notion_project_issues(client, project, None, false)
                            .await
                    } else {
                        log::debug!(
                            "Skipping Notion project '{}': no Notion client provided",
                            project.name
                        );
                        continue;
                    }
                }
                other => {
                    log::debug!(
                        "Skipping project '{}': unsupported PM system {:?}",
                        project.name,
                        other
                    );
                    continue;
                }
            };

            match result {
                Ok(stats) => {
                    total_stats.issues_synced += stats.issues_synced;
                    total_stats.issues_updated += stats.issues_updated;
                    total_stats.embeddings_computed += stats.embeddings_computed;
                    total_stats.errors.extend(stats.errors);
                }
                Err(e) => {
                    total_stats.errors.push(format!(
                        "Failed to sync project '{}': {e}",
                        project.name
                    ));
                }
            }
        }

        Ok(total_stats)
    }

    /// Sync all projects that have PM links (Plane only - for backwards compatibility)
    ///
    /// # Errors
    ///
    /// Returns an error if API calls fail or database operations fail
    pub async fn sync_all_linked_projects(
        &self,
        plane_client: &PlaneClient,
    ) -> Result<SyncStats> {
        self.sync_all_linked_projects_multi(Some(plane_client), None)
            .await
    }

    /// Compute embedding for an issue and store it
    fn compute_and_store_embedding(&self, candidate: &IssueCandidate) -> Result<()> {
        let text = candidate.embedding_text();

        let embedding = {
            let mut service = self
                .embedding_service
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to lock embedding service: {e}"))?;
            service.generate_embedding(&text)?
        };

        self.database
            .update_issue_embedding(candidate.id, &embedding)?;

        log::debug!(
            "Computed embedding for {} ({} dims)",
            candidate.external_id,
            embedding.len()
        );

        Ok(())
    }

    /// Recompute embeddings for all issues without embeddings
    ///
    /// # Errors
    ///
    /// Returns an error if database operations fail
    pub fn recompute_missing_embeddings(&self) -> Result<usize> {
        let linked_projects = self.database.get_projects_with_pm_link()?;
        let mut computed = 0;

        for project in &linked_projects {
            let candidates = self.database.get_issue_candidates_for_project(project.id)?;

            for candidate in candidates {
                if candidate.embedding.is_none() {
                    if let Err(e) = self.compute_and_store_embedding(&candidate) {
                        log::warn!(
                            "Failed to compute embedding for {}: {e}",
                            candidate.external_id
                        );
                    } else {
                        computed += 1;
                    }
                }
            }
        }

        Ok(computed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_stats_display() {
        let stats = SyncStats {
            issues_synced: 10,
            issues_updated: 5,
            embeddings_computed: 8,
            errors: vec!["error1".to_string()],
        };

        let display = format!("{stats}");
        assert!(display.contains("Synced: 10"));
        assert!(display.contains("Updated: 5"));
        assert!(display.contains("Embeddings: 8"));
        assert!(display.contains("Errors: 1"));
    }
}
