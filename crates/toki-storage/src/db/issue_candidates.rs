use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use crate::models::{Complexity, IssueCandidate};

use super::Database;

impl Database {
    /// Upsert an issue candidate (for AI matching)
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn upsert_issue_candidate(&self, candidate: &IssueCandidate) -> Result<()> {
        let labels_json = serde_json::to_string(&candidate.labels)?;
        let embedding_bytes: Option<Vec<u8>> = candidate
            .embedding
            .as_ref()
            .map(|e| e.iter().flat_map(|f| f.to_le_bytes()).collect());
        let complexity_value: Option<i32> = candidate.complexity.map(|c| c.points() as i32);

        self.conn.execute(
            "INSERT INTO issue_candidates
             (id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced, complexity, complexity_reason, estimated_seconds, estimate_source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
             ON CONFLICT(external_id, external_system) DO UPDATE SET
                project_id = excluded.project_id,
                pm_project_id = excluded.pm_project_id,
                source_page_id = COALESCE(excluded.source_page_id, issue_candidates.source_page_id),
                title = excluded.title,
                description = excluded.description,
                status = excluded.status,
                labels = excluded.labels,
                assignee = excluded.assignee,
                embedding = COALESCE(excluded.embedding, issue_candidates.embedding),
                last_synced = excluded.last_synced,
                complexity = COALESCE(excluded.complexity, issue_candidates.complexity),
                complexity_reason = COALESCE(excluded.complexity_reason, issue_candidates.complexity_reason),
                estimated_seconds = COALESCE(excluded.estimated_seconds, issue_candidates.estimated_seconds),
                estimate_source = COALESCE(excluded.estimate_source, issue_candidates.estimate_source)",
            params![
                candidate.id.to_string(),
                candidate.project_id.to_string(),
                candidate.external_id,
                candidate.external_system,
                candidate.pm_project_id,
                candidate.source_page_id,
                candidate.title,
                candidate.description,
                candidate.status,
                labels_json,
                candidate.assignee,
                embedding_bytes,
                candidate.last_synced.to_rfc3339(),
                complexity_value,
                candidate.complexity_reason,
                candidate.estimated_seconds,
                candidate.estimate_source,
            ],
        )?;
        Ok(())
    }

    /// Update issue candidate embedding
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn update_issue_embedding(
        &self,
        candidate_id: uuid::Uuid,
        embedding: &[f32],
    ) -> Result<()> {
        let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        self.conn.execute(
            "UPDATE issue_candidates SET embedding = ?1 WHERE id = ?2",
            params![bytes, candidate_id.to_string()],
        )?;
        Ok(())
    }

    /// Get all issue candidates for a project
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_issue_candidates_for_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<IssueCandidate>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced, complexity, complexity_reason, estimated_seconds, estimate_source
             FROM issue_candidates
             WHERE project_id = ?1
             ORDER BY last_synced DESC",
        )?;

        let candidates = stmt
            .query_map([project_id.to_string()], Self::row_to_issue_candidate)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(candidates)
    }

    /// Get active issue candidates for a project (excludes done/cancelled)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_active_issue_candidates(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<IssueCandidate>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced, complexity, complexity_reason, estimated_seconds, estimate_source
             FROM issue_candidates
             WHERE project_id = ?1 AND status NOT IN ('done', 'cancelled', 'completed')
             ORDER BY last_synced DESC",
        )?;

        let candidates = stmt
            .query_map([project_id.to_string()], Self::row_to_issue_candidate)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(candidates)
    }

    /// Get issue candidate by external ID
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_issue_candidate(
        &self,
        external_id: &str,
        external_system: &str,
    ) -> Result<Option<IssueCandidate>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced, complexity, complexity_reason, estimated_seconds, estimate_source
                 FROM issue_candidates
                 WHERE external_id = ?1 AND external_system = ?2",
                params![external_id, external_system],
                Self::row_to_issue_candidate,
            )
            .optional()?;

        Ok(result)
    }

    /// Get an issue candidate by external_id (any system)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_issue_candidate_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<IssueCandidate>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced, complexity, complexity_reason, estimated_seconds, estimate_source
                 FROM issue_candidates
                 WHERE external_id = ?1",
                params![external_id],
                Self::row_to_issue_candidate,
            )
            .optional()?;

        Ok(result)
    }

    /// Get an issue candidate by UUID
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_issue_candidate_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<IssueCandidate>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced, complexity, complexity_reason, estimated_seconds, estimate_source
                 FROM issue_candidates
                 WHERE id = ?1",
                params![id.to_string()],
                Self::row_to_issue_candidate,
            )
            .optional()?;

        Ok(result)
    }

    /// Get all Notion issue candidates with their page IDs
    ///
    /// Returns a map of external_id -> source_page_id for populating the NotionClient cache
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_notion_page_id_map(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare(
            "SELECT external_id, source_page_id
             FROM issue_candidates
             WHERE external_system = 'notion' AND source_page_id IS NOT NULL",
        )?;

        let mut map = std::collections::HashMap::new();
        let rows = stmt.query_map([], |row| {
            let external_id: String = row.get(0)?;
            let source_page_id: String = row.get(1)?;
            Ok((external_id, source_page_id))
        })?;

        for row in rows {
            let (external_id, source_page_id) = row?;
            map.insert(external_id, source_page_id);
        }

        Ok(map)
    }

    /// Helper function to parse `IssueCandidate` from database row
    pub(crate) fn row_to_issue_candidate(row: &rusqlite::Row) -> rusqlite::Result<IssueCandidate> {
        // Column order: id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced, complexity, complexity_reason, estimated_seconds, estimate_source
        let labels_json: String = row.get(9)?;
        let labels: Vec<String> = serde_json::from_str(&labels_json).unwrap_or_default();

        let embedding_bytes: Option<Vec<u8>> = row.get(11)?;
        let embedding = embedding_bytes.and_then(|b| {
            if b.len() % 4 != 0 {
                return None;
            }
            Some(
                b.chunks_exact(4)
                    .map(|chunk| {
                        let arr = [chunk[0], chunk[1], chunk[2], chunk[3]];
                        f32::from_le_bytes(arr)
                    })
                    .collect(),
            )
        });

        // Parse complexity from integer (safely cast i32 -> u8)
        let complexity_value: Option<i32> = row.get(13)?;
        let complexity =
            complexity_value.and_then(|v| u8::try_from(v).ok().and_then(Complexity::from_points));

        // Parse estimate fields (safely cast i64 -> u32, clamping to valid range)
        let estimated_seconds: Option<u32> = row
            .get::<_, Option<i64>>(15)?
            .map(|v| u32::try_from(v.max(0)).unwrap_or(u32::MAX));
        let estimate_source: Option<String> = row.get(16)?;

        Ok(IssueCandidate {
            id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
            project_id: uuid::Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
            external_id: row.get(2)?,
            external_system: row.get(3)?,
            pm_project_id: row.get(4)?,
            source_page_id: row.get(5)?,
            title: row.get(6)?,
            description: row.get(7)?,
            status: row.get(8)?,
            labels,
            assignee: row.get(10)?,
            embedding,
            last_synced: DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                .unwrap()
                .with_timezone(&Utc),
            complexity,
            complexity_reason: row.get(14)?,
            estimated_seconds,
            estimate_source,
        })
    }

    /// Update complexity for an issue candidate
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn update_issue_complexity(
        &self,
        external_id: &str,
        external_system: &str,
        complexity: Complexity,
        reason: &str,
    ) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE issue_candidates SET complexity = ?1, complexity_reason = ?2
             WHERE external_id = ?3 AND external_system = ?4",
            params![complexity.points() as i32, reason, external_id, external_system],
        )?;
        Ok(updated > 0)
    }

    /// Update time estimate for an issue candidate (for scope tracking)
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn update_issue_estimate(
        &self,
        external_id: &str,
        external_system: &str,
        estimated_seconds: u32,
        source: &str,
    ) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE issue_candidates SET estimated_seconds = ?1, estimate_source = ?2
             WHERE external_id = ?3 AND external_system = ?4",
            params![estimated_seconds as i64, source, external_id, external_system],
        )?;
        Ok(updated > 0)
    }

    /// Get all issues with estimates for scope analysis
    ///
    /// Returns issues that have both an estimate and tracked time
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn get_issues_with_estimates(&self, project_id: Option<uuid::Uuid>) -> Result<Vec<IssueCandidate>> {
        let query = match project_id {
            Some(_) => {
                "SELECT id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced, complexity, complexity_reason, estimated_seconds, estimate_source
                 FROM issue_candidates
                 WHERE estimated_seconds IS NOT NULL AND project_id = ?1
                 ORDER BY last_synced DESC"
            }
            None => {
                "SELECT id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced, complexity, complexity_reason, estimated_seconds, estimate_source
                 FROM issue_candidates
                 WHERE estimated_seconds IS NOT NULL
                 ORDER BY last_synced DESC"
            }
        };

        let mut stmt = self.conn.prepare(query)?;
        let candidates = match project_id {
            Some(pid) => stmt
                .query_map([pid.to_string()], Self::row_to_issue_candidate)?
                .collect::<std::result::Result<Vec<_>, _>>()?,
            None => stmt
                .query_map([], Self::row_to_issue_candidate)?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        };

        Ok(candidates)
    }
}
