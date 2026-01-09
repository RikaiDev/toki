use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use super::helpers::{parse_datetime, parse_uuid};
use super::Database;
use crate::models::SyncedIssue;

impl Database {
    /// Insert or update a synced issue record
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn upsert_synced_issue(&self, synced: &SyncedIssue) -> Result<()> {
        self.conn.execute(
            "INSERT INTO synced_issues
             (id, source_page_id, source_database_id, target_system, target_project,
              target_issue_id, target_issue_number, target_issue_url, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(source_page_id, target_system, target_project) DO UPDATE SET
                target_issue_id = excluded.target_issue_id,
                target_issue_number = excluded.target_issue_number,
                target_issue_url = excluded.target_issue_url,
                title = excluded.title,
                updated_at = excluded.updated_at",
            params![
                synced.id.to_string(),
                synced.source_page_id,
                synced.source_database_id,
                synced.target_system,
                synced.target_project,
                synced.target_issue_id,
                synced.target_issue_number,
                synced.target_issue_url,
                synced.title,
                synced.created_at.to_rfc3339(),
                synced.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get a synced issue by source page and target
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_synced_issue(
        &self,
        source_page_id: &str,
        target_system: &str,
        target_project: &str,
    ) -> Result<Option<SyncedIssue>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, source_page_id, source_database_id, target_system, target_project,
                        target_issue_id, target_issue_number, target_issue_url, title, created_at, updated_at
                 FROM synced_issues
                 WHERE source_page_id = ?1 AND target_system = ?2 AND target_project = ?3",
                params![source_page_id, target_system, target_project],
                Self::row_to_synced_issue,
            )
            .optional()?;

        Ok(result)
    }

    /// Get all synced issues for a Notion database
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_synced_issues_for_database(&self, database_id: &str) -> Result<Vec<SyncedIssue>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_page_id, source_database_id, target_system, target_project,
                    target_issue_id, target_issue_number, target_issue_url, title, created_at, updated_at
             FROM synced_issues
             WHERE source_database_id = ?1
             ORDER BY updated_at DESC",
        )?;

        let issues = stmt
            .query_map([database_id], Self::row_to_synced_issue)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(issues)
    }

    /// Get all synced issues for a target project
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_synced_issues_for_target(
        &self,
        target_system: &str,
        target_project: &str,
    ) -> Result<Vec<SyncedIssue>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_page_id, source_database_id, target_system, target_project,
                    target_issue_id, target_issue_number, target_issue_url, title, created_at, updated_at
             FROM synced_issues
             WHERE target_system = ?1 AND target_project = ?2
             ORDER BY updated_at DESC",
        )?;

        let issues = stmt
            .query_map(
                params![target_system, target_project],
                Self::row_to_synced_issue,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(issues)
    }

    /// Check if a Notion page has been synced to a target
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn is_page_synced(
        &self,
        source_page_id: &str,
        target_system: &str,
        target_project: &str,
    ) -> Result<bool> {
        let count: i32 = self.conn.query_row(
            "SELECT COUNT(*) FROM synced_issues
             WHERE source_page_id = ?1 AND target_system = ?2 AND target_project = ?3",
            params![source_page_id, target_system, target_project],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Delete a synced issue record
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn delete_synced_issue(&self, id: uuid::Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM synced_issues WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    /// Helper function to parse `SyncedIssue` from database row
    pub(crate) fn row_to_synced_issue(row: &rusqlite::Row) -> rusqlite::Result<SyncedIssue> {
        Ok(SyncedIssue {
            id: parse_uuid(&row.get::<_, String>(0)?)?,
            source_page_id: row.get(1)?,
            source_database_id: row.get(2)?,
            target_system: row.get(3)?,
            target_project: row.get(4)?,
            target_issue_id: row.get(5)?,
            target_issue_number: row.get(6)?,
            target_issue_url: row.get(7)?,
            title: row.get(8)?,
            created_at: parse_datetime(&row.get::<_, String>(9)?)?,
            updated_at: parse_datetime(&row.get::<_, String>(10)?)?,
        })
    }
}
