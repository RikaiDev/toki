use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;

use super::helpers::{parse_datetime, parse_uuid};
use super::Database;
use crate::models::{ActivitySpan, ActivitySpanContext};

impl Database {
    /// Create a new activity span
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn create_activity_span(&self, span: &ActivitySpan) -> Result<uuid::Uuid> {
        let context_json = span
            .context
            .as_ref()
            .and_then(|c| serde_json::to_string(c).ok());

        self.conn.execute(
            "INSERT INTO activity_spans
             (id, app_bundle_id, category, start_time, end_time, duration_seconds, project_id, work_item_id, session_id, context)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                span.id.to_string(),
                span.app_bundle_id,
                span.category,
                span.start_time.to_rfc3339(),
                span.end_time.map(|t| t.to_rfc3339()),
                span.duration_seconds,
                span.project_id.map(|id| id.to_string()),
                span.work_item_id.map(|id| id.to_string()),
                span.session_id.map(|id| id.to_string()),
                context_json,
            ],
        )?;
        Ok(span.id)
    }

    /// Update activity span context (for enriching with additional data)
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn update_activity_span_context(
        &self,
        span_id: uuid::Uuid,
        context: &ActivitySpanContext,
    ) -> Result<()> {
        let context_json = serde_json::to_string(context)?;
        self.conn.execute(
            "UPDATE activity_spans SET context = ?1 WHERE id = ?2",
            params![context_json, span_id.to_string()],
        )?;
        Ok(())
    }

    /// Add tag to activity span
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn add_tag_to_span(&self, span_id: uuid::Uuid, tag: &str) -> Result<()> {
        // Get current context
        if let Some(mut span) = self.get_activity_span(span_id)? {
            span.add_tag(tag.to_string());
            if let Some(ctx) = &span.context {
                self.update_activity_span_context(span_id, ctx)?;
            }
        }
        Ok(())
    }

    /// Associate work item with activity span (for retroactive classification)
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn associate_work_item_to_span(
        &self,
        span_id: uuid::Uuid,
        work_item_id: uuid::Uuid,
    ) -> Result<()> {
        if let Some(mut span) = self.get_activity_span(span_id)? {
            span.add_work_item(work_item_id);
            if let Some(ctx) = &span.context {
                self.update_activity_span_context(span_id, ctx)?;
            }
        }
        Ok(())
    }

    /// Finalize an activity span by setting its end time and duration
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn finalize_activity_span(
        &self,
        span_id: uuid::Uuid,
        end_time: DateTime<Utc>,
    ) -> Result<()> {
        // Get the span to calculate duration
        let span = self
            .get_activity_span(span_id)?
            .ok_or_else(|| anyhow::anyhow!("Activity span not found"))?;

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let duration = end_time
            .signed_duration_since(span.start_time)
            .num_seconds()
            .max(0) as u32;

        self.conn.execute(
            "UPDATE activity_spans SET end_time = ?1, duration_seconds = ?2 WHERE id = ?3",
            params![end_time.to_rfc3339(), duration, span_id.to_string()],
        )?;
        Ok(())
    }

    /// Get an activity span by ID
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID or datetime parsing fails for corrupted database entries
    pub fn get_activity_span(&self, span_id: uuid::Uuid) -> Result<Option<ActivitySpan>> {
        use rusqlite::OptionalExtension;
        let result = self
            .conn
            .query_row(
                "SELECT id, app_bundle_id, category, start_time, end_time, duration_seconds, project_id, work_item_id, session_id, context
                 FROM activity_spans
                 WHERE id = ?1",
                params![span_id.to_string()],
                Self::row_to_activity_span,
            )
            .optional()?;

        Ok(result)
    }

    /// Helper function to parse `ActivitySpan` from database row
    pub(crate) fn row_to_activity_span(row: &rusqlite::Row) -> rusqlite::Result<ActivitySpan> {
        let context_json: Option<String> = row.get(9)?;
        let context = context_json.and_then(|s| serde_json::from_str(&s).ok());

        Ok(ActivitySpan {
            id: parse_uuid(&row.get::<_, String>(0)?)?,
            app_bundle_id: row.get(1)?,
            category: row.get(2)?,
            start_time: parse_datetime(&row.get::<_, String>(3)?)?,
            end_time: row
                .get::<_, Option<String>>(4)?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            duration_seconds: row.get(5)?,
            project_id: row
                .get::<_, Option<String>>(6)?
                .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
            work_item_id: row
                .get::<_, Option<String>>(7)?
                .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
            session_id: row
                .get::<_, Option<String>>(8)?
                .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
            context,
        })
    }

    /// Get the currently ongoing activity span (if any)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID or datetime parsing fails for corrupted database entries
    pub fn get_ongoing_span(&self) -> Result<Option<ActivitySpan>> {
        use rusqlite::OptionalExtension;
        let result = self
            .conn
            .query_row(
                "SELECT id, app_bundle_id, category, start_time, end_time, duration_seconds, project_id, work_item_id, session_id, context
                 FROM activity_spans
                 WHERE end_time IS NULL
                 ORDER BY start_time DESC
                 LIMIT 1",
                [],
                |row| {
                    let context_json: Option<String> = row.get(9)?;
                    let context = context_json.and_then(|s| serde_json::from_str(&s).ok());
                    Ok(ActivitySpan {
                        id: parse_uuid(&row.get::<_, String>(0)?)?,
                        app_bundle_id: row.get(1)?,
                        category: row.get(2)?,
                        start_time: parse_datetime(&row.get::<_, String>(3)?)?,
                        end_time: None,
                        duration_seconds: row.get(5)?,
                        project_id: row
                            .get::<_, Option<String>>(6)?
                            .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
                        work_item_id: row
                            .get::<_, Option<String>>(7)?
                            .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
                        session_id: row
                            .get::<_, Option<String>>(8)?
                            .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
                        context,
                    })
                },
            )
            .optional()?;

        Ok(result)
    }

    /// Get activity spans within a time range
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID or datetime parsing fails for corrupted database entries
    pub fn get_activity_spans(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<ActivitySpan>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, app_bundle_id, category, start_time, end_time, duration_seconds, project_id, work_item_id, session_id, context
             FROM activity_spans
             WHERE start_time >= ?1 AND start_time <= ?2
             ORDER BY start_time ASC",
        )?;

        let spans = stmt
            .query_map(
                params![start.to_rfc3339(), end.to_rfc3339()],
                Self::row_to_activity_span,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(spans)
    }

    /// Get activity spans for a specific work item
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID or datetime parsing fails for corrupted database entries
    pub fn get_activity_spans_by_work_item(
        &self,
        work_item_id: uuid::Uuid,
    ) -> Result<Vec<ActivitySpan>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, app_bundle_id, category, start_time, end_time, duration_seconds, project_id, work_item_id, session_id, context
             FROM activity_spans
             WHERE work_item_id = ?1
             ORDER BY start_time ASC",
        )?;

        let spans = stmt
            .query_map([work_item_id.to_string()], Self::row_to_activity_span)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(spans)
    }

    /// Get activity spans for a specific project
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID or datetime parsing fails for corrupted database entries
    pub fn get_activity_spans_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<ActivitySpan>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, app_bundle_id, category, start_time, end_time, duration_seconds, project_id, work_item_id, session_id, context
             FROM activity_spans
             WHERE project_id = ?1
             ORDER BY start_time ASC",
        )?;

        let spans = stmt
            .query_map([project_id.to_string()], Self::row_to_activity_span)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(spans)
    }

    /// Get activity spans for a specific session
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID or datetime parsing fails for corrupted database entries
    pub fn get_activity_spans_by_session(
        &self,
        session_id: uuid::Uuid,
    ) -> Result<Vec<ActivitySpan>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, app_bundle_id, category, start_time, end_time, duration_seconds, project_id, work_item_id, session_id, context
             FROM activity_spans
             WHERE session_id = ?1
             ORDER BY start_time ASC",
        )?;

        let spans = stmt
            .query_map([session_id.to_string()], Self::row_to_activity_span)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(spans)
    }
}
