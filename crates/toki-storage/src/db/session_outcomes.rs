use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::models::{OutcomeSummary, OutcomeType, SessionOutcome};

use super::Database;

impl Database {
    /// Add an outcome to a Claude session
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn add_session_outcome(&self, outcome: &SessionOutcome) -> Result<()> {
        self.conn.execute(
            "INSERT INTO session_outcomes (id, session_id, outcome_type, reference_id, description, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                outcome.id.to_string(),
                outcome.session_id.to_string(),
                outcome.outcome_type.to_string(),
                outcome.reference_id,
                outcome.description,
                outcome.created_at.to_rfc3339(),
            ],
        )?;
        log::debug!(
            "Added outcome to session {}: {} ({:?})",
            outcome.session_id,
            outcome.outcome_type,
            outcome.reference_id
        );
        Ok(())
    }

    /// Get all outcomes for a session
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_session_outcomes(&self, session_id: uuid::Uuid) -> Result<Vec<SessionOutcome>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, outcome_type, reference_id, description, created_at
             FROM session_outcomes
             WHERE session_id = ?1
             ORDER BY created_at ASC",
        )?;

        let outcomes = stmt
            .query_map([session_id.to_string()], Self::row_to_session_outcome)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(outcomes)
    }

    /// Get outcome summary for a session
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_session_outcome_summary(&self, session_id: uuid::Uuid) -> Result<OutcomeSummary> {
        let outcomes = self.get_session_outcomes(session_id)?;
        Ok(OutcomeSummary::from_outcomes(&outcomes))
    }

    /// Get outcomes by type for a session
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_session_outcomes_by_type(
        &self,
        session_id: uuid::Uuid,
        outcome_type: &OutcomeType,
    ) -> Result<Vec<SessionOutcome>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, outcome_type, reference_id, description, created_at
             FROM session_outcomes
             WHERE session_id = ?1 AND outcome_type = ?2
             ORDER BY created_at ASC",
        )?;

        let outcomes = stmt
            .query_map(
                params![session_id.to_string(), outcome_type.to_string()],
                Self::row_to_session_outcome,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(outcomes)
    }

    /// Get all outcomes within a date range
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_outcomes_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<SessionOutcome>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, outcome_type, reference_id, description, created_at
             FROM session_outcomes
             WHERE created_at >= ?1 AND created_at <= ?2
             ORDER BY created_at DESC",
        )?;

        let outcomes = stmt
            .query_map(
                params![start.to_rfc3339(), end.to_rfc3339()],
                Self::row_to_session_outcome,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(outcomes)
    }

    /// Get aggregated outcome summary for a date range
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_outcome_summary_for_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<OutcomeSummary> {
        let outcomes = self.get_outcomes_in_range(start, end)?;
        Ok(OutcomeSummary::from_outcomes(&outcomes))
    }

    /// Check if an outcome with the same reference already exists for a session
    ///
    /// This prevents duplicate outcomes (e.g., same commit recorded twice)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn outcome_exists(
        &self,
        session_id: uuid::Uuid,
        outcome_type: &OutcomeType,
        reference_id: &str,
    ) -> Result<bool> {
        let count: i32 = self.conn.query_row(
            "SELECT COUNT(*) FROM session_outcomes
             WHERE session_id = ?1 AND outcome_type = ?2 AND reference_id = ?3",
            params![
                session_id.to_string(),
                outcome_type.to_string(),
                reference_id
            ],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Delete all outcomes for a session
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn delete_session_outcomes(&self, session_id: uuid::Uuid) -> Result<u32> {
        let deleted = self.conn.execute(
            "DELETE FROM session_outcomes WHERE session_id = ?1",
            params![session_id.to_string()],
        )?;
        Ok(u32::try_from(deleted).unwrap_or(u32::MAX))
    }

    /// Helper function to parse `SessionOutcome` from database row
    pub(crate) fn row_to_session_outcome(
        row: &rusqlite::Row,
    ) -> rusqlite::Result<SessionOutcome> {
        let outcome_type_str: String = row.get(2)?;
        let outcome_type = outcome_type_str
            .parse::<OutcomeType>()
            .unwrap_or(OutcomeType::Commit);

        Ok(SessionOutcome {
            id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
            session_id: uuid::Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
            outcome_type,
            reference_id: row.get(3)?,
            description: row.get(4)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                .unwrap()
                .with_timezone(&Utc),
        })
    }
}
