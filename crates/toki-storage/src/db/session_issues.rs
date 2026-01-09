use anyhow::Result;
use rusqlite::params;

use super::helpers::{parse_datetime, parse_uuid};
use super::Database;
use crate::models::{IssueRelationship, SessionIssue};

impl Database {
    /// Link an issue to a Claude session
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn add_session_issue(&self, issue: &SessionIssue) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO session_issues (session_id, issue_id, issue_system, relationship, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                issue.session_id.to_string(),
                issue.issue_id,
                issue.issue_system,
                issue.relationship.to_string(),
                issue.created_at.to_rfc3339(),
            ],
        )?;
        log::debug!(
            "Linked issue {} to session {}: {:?}",
            issue.display_id(),
            issue.session_id,
            issue.relationship
        );
        Ok(())
    }

    /// Get all issues linked to a session
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_session_issues(&self, session_id: uuid::Uuid) -> Result<Vec<SessionIssue>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, issue_id, issue_system, relationship, created_at
             FROM session_issues
             WHERE session_id = ?1
             ORDER BY created_at ASC",
        )?;

        let issues = stmt
            .query_map([session_id.to_string()], Self::row_to_session_issue)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(issues)
    }

    /// Get issues by relationship type for a session
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_session_issues_by_relationship(
        &self,
        session_id: uuid::Uuid,
        relationship: &IssueRelationship,
    ) -> Result<Vec<SessionIssue>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, issue_id, issue_system, relationship, created_at
             FROM session_issues
             WHERE session_id = ?1 AND relationship = ?2
             ORDER BY created_at ASC",
        )?;

        let issues = stmt
            .query_map(
                params![session_id.to_string(), relationship.to_string()],
                Self::row_to_session_issue,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(issues)
    }

    /// Check if an issue is already linked to a session
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn session_issue_exists(
        &self,
        session_id: uuid::Uuid,
        issue_id: &str,
        issue_system: &str,
    ) -> Result<bool> {
        let count: i32 = self.conn.query_row(
            "SELECT COUNT(*) FROM session_issues
             WHERE session_id = ?1 AND issue_id = ?2 AND issue_system = ?3",
            params![session_id.to_string(), issue_id, issue_system],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Update the relationship type for an existing session-issue link
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn update_session_issue_relationship(
        &self,
        session_id: uuid::Uuid,
        issue_id: &str,
        issue_system: &str,
        relationship: &IssueRelationship,
    ) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE session_issues SET relationship = ?1
             WHERE session_id = ?2 AND issue_id = ?3 AND issue_system = ?4",
            params![
                relationship.to_string(),
                session_id.to_string(),
                issue_id,
                issue_system
            ],
        )?;
        Ok(updated > 0)
    }

    /// Delete all issue links for a session
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn delete_session_issues(&self, session_id: uuid::Uuid) -> Result<u32> {
        let deleted = self.conn.execute(
            "DELETE FROM session_issues WHERE session_id = ?1",
            params![session_id.to_string()],
        )?;
        Ok(u32::try_from(deleted).unwrap_or(u32::MAX))
    }

    /// Delete a specific session-issue link
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn delete_session_issue(
        &self,
        session_id: uuid::Uuid,
        issue_id: &str,
        issue_system: &str,
    ) -> Result<bool> {
        let deleted = self.conn.execute(
            "DELETE FROM session_issues
             WHERE session_id = ?1 AND issue_id = ?2 AND issue_system = ?3",
            params![session_id.to_string(), issue_id, issue_system],
        )?;
        Ok(deleted > 0)
    }

    /// Get all sessions that worked on a specific issue
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_sessions_for_issue(
        &self,
        issue_id: &str,
        issue_system: &str,
    ) -> Result<Vec<SessionIssue>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, issue_id, issue_system, relationship, created_at
             FROM session_issues
             WHERE issue_id = ?1 AND issue_system = ?2
             ORDER BY created_at DESC",
        )?;

        let issues = stmt
            .query_map(params![issue_id, issue_system], Self::row_to_session_issue)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(issues)
    }

    /// Get total time spent on an issue across all sessions (in seconds)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_issue_total_time(
        &self,
        issue_id: &str,
        issue_system: &str,
    ) -> Result<u32> {
        // Join session_issues with claude_sessions to sum up duration
        let total: Option<i64> = self.conn.query_row(
            "SELECT SUM(
                CASE
                    WHEN cs.ended_at IS NOT NULL THEN
                        CAST((julianday(cs.ended_at) - julianday(cs.started_at)) * 86400 AS INTEGER)
                    ELSE 0
                END
             )
             FROM session_issues si
             JOIN claude_sessions cs ON si.session_id = cs.id
             WHERE si.issue_id = ?1 AND si.issue_system = ?2",
            params![issue_id, issue_system],
            |row| row.get(0),
        )?;

        Ok(u32::try_from(total.unwrap_or(0).max(0)).unwrap_or(u32::MAX))
    }

    /// Get time statistics for issues with historical data
    ///
    /// Returns a list of (`issue_id`, `issue_system`, `total_seconds`, `session_count`)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_issue_time_stats(&self) -> Result<Vec<IssueTimeStats>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                si.issue_id,
                si.issue_system,
                COUNT(DISTINCT si.session_id) as session_count,
                SUM(
                    CASE
                        WHEN cs.ended_at IS NOT NULL THEN
                            CAST((julianday(cs.ended_at) - julianday(cs.started_at)) * 86400 AS INTEGER)
                        ELSE 0
                    END
                ) as total_seconds
             FROM session_issues si
             JOIN claude_sessions cs ON si.session_id = cs.id
             WHERE cs.ended_at IS NOT NULL
             GROUP BY si.issue_id, si.issue_system
             HAVING total_seconds > 0
             ORDER BY total_seconds DESC",
        )?;

        let stats = stmt
            .query_map([], |row| {
                Ok(IssueTimeStats {
                    issue_id: row.get(0)?,
                    issue_system: row.get(1)?,
                    session_count: row.get(2)?,
                    total_seconds: u32::try_from(row.get::<_, i64>(3)?.max(0)).unwrap_or(u32::MAX),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(stats)
    }

    /// Helper function to parse `SessionIssue` from database row
    pub(crate) fn row_to_session_issue(row: &rusqlite::Row) -> rusqlite::Result<SessionIssue> {
        let relationship_str: String = row.get(3)?;
        let relationship = relationship_str
            .parse::<IssueRelationship>()
            .unwrap_or(IssueRelationship::WorkedOn);

        Ok(SessionIssue {
            session_id: parse_uuid(&row.get::<_, String>(0)?)?,
            issue_id: row.get(1)?,
            issue_system: row.get(2)?,
            relationship,
            created_at: parse_datetime(&row.get::<_, String>(4)?)?,
        })
    }
}

/// Time statistics for an issue
#[derive(Debug, Clone)]
pub struct IssueTimeStats {
    pub issue_id: String,
    pub issue_system: String,
    pub session_count: u32,
    pub total_seconds: u32,
}
