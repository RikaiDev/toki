use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use super::helpers::{parse_datetime, parse_uuid};
use super::Database;
use crate::models::ClaudeSession;

impl Database {
    /// Start a new Claude Code session
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn start_claude_session(
        &self,
        session_id: &str,
        project_id: Option<uuid::Uuid>,
    ) -> Result<ClaudeSession> {
        let session = ClaudeSession::new(session_id.to_string(), project_id);

        self.conn.execute(
            "INSERT INTO claude_sessions
             (id, session_id, project_id, started_at, ended_at, end_reason, tool_calls, prompt_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session.id.to_string(),
                session.session_id,
                session.project_id.map(|id| id.to_string()),
                session.started_at.to_rfc3339(),
                session.ended_at.map(|t| t.to_rfc3339()),
                session.end_reason,
                session.tool_calls,
                session.prompt_count,
                session.created_at.to_rfc3339(),
            ],
        )?;

        log::info!("Started Claude session: {session_id}");
        Ok(session)
    }

    /// End a Claude Code session
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn end_claude_session(&self, session_id: &str, reason: Option<&str>) -> Result<()> {
        let ended_at = Utc::now();
        self.conn.execute(
            "UPDATE claude_sessions SET ended_at = ?1, end_reason = ?2 WHERE session_id = ?3 AND ended_at IS NULL",
            params![ended_at.to_rfc3339(), reason, session_id],
        )?;
        log::info!("Ended Claude session: {session_id} (reason: {reason:?})");
        Ok(())
    }

    /// Get a Claude session by session ID
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_claude_session(&self, session_id: &str) -> Result<Option<ClaudeSession>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, session_id, project_id, started_at, ended_at, end_reason, tool_calls, prompt_count, created_at
                 FROM claude_sessions
                 WHERE session_id = ?1",
                params![session_id],
                Self::row_to_claude_session,
            )
            .optional()?;

        Ok(result)
    }

    /// Get active (unclosed) Claude sessions
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_active_claude_sessions(&self) -> Result<Vec<ClaudeSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, project_id, started_at, ended_at, end_reason, tool_calls, prompt_count, created_at
             FROM claude_sessions
             WHERE ended_at IS NULL
             ORDER BY started_at DESC",
        )?;

        let sessions = stmt
            .query_map([], Self::row_to_claude_session)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    /// Get Claude sessions for today
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_claude_sessions_today(&self) -> Result<Vec<ClaudeSession>> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, project_id, started_at, ended_at, end_reason, tool_calls, prompt_count, created_at
             FROM claude_sessions
             WHERE date(started_at) = ?1
             ORDER BY started_at DESC",
        )?;

        let sessions = stmt
            .query_map([today], Self::row_to_claude_session)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    /// Get Claude sessions within a date range
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_claude_sessions(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<ClaudeSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, project_id, started_at, ended_at, end_reason, tool_calls, prompt_count, created_at
             FROM claude_sessions
             WHERE started_at >= ?1 AND started_at <= ?2
             ORDER BY started_at DESC",
        )?;

        let sessions = stmt
            .query_map(
                params![start.to_rfc3339(), end.to_rfc3339()],
                Self::row_to_claude_session,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    /// Increment tool call count for a session
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn increment_claude_session_tools(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE claude_sessions SET tool_calls = tool_calls + 1 WHERE session_id = ?1 AND ended_at IS NULL",
            params![session_id],
        )?;
        Ok(())
    }

    /// Increment prompt count for a session
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn increment_claude_session_prompts(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE claude_sessions SET prompt_count = prompt_count + 1 WHERE session_id = ?1 AND ended_at IS NULL",
            params![session_id],
        )?;
        Ok(())
    }

    /// Helper function to parse `ClaudeSession` from database row
    pub(crate) fn row_to_claude_session(row: &rusqlite::Row) -> rusqlite::Result<ClaudeSession> {
        Ok(ClaudeSession {
            id: parse_uuid(&row.get::<_, String>(0)?)?,
            session_id: row.get(1)?,
            project_id: row
                .get::<_, Option<String>>(2)?
                .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
            started_at: parse_datetime(&row.get::<_, String>(3)?)?,
            ended_at: row
                .get::<_, Option<String>>(4)?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            end_reason: row.get(5)?,
            tool_calls: row.get(6)?,
            prompt_count: row.get(7)?,
            created_at: parse_datetime(&row.get::<_, String>(8)?)?,
        })
    }
}
