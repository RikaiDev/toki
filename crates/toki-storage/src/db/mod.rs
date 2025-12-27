//! Database operations split into domain-specific modules.
//!
//! This module re-exports the main Database struct and all its operations.

mod activity_spans;
mod claude_sessions;
mod issue_candidates;
mod projects;
mod session_issues;
mod session_outcomes;
mod synced_issues;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

use crate::encryption;
use crate::migrations;
use crate::models::{
    Activity, Category, ClassificationRule, IntegrationConfig, PatternType, Session, Settings,
    WorkItem,
};

/// Database connection wrapper
pub struct Database {
    pub(crate) conn: Connection,
}

// Implement Send and Sync for Database to allow sharing across threads
unsafe impl Send for Database {}
unsafe impl Sync for Database {}

impl Database {
    /// Create a new database connection
    ///
    /// # Errors
    ///
    /// Returns an error if database directory creation, connection opening, or schema initialization fails
    pub fn new(db_path: Option<PathBuf>) -> Result<Self> {
        Self::new_with_encryption(db_path, None)
    }

    /// Create a new database connection with optional encryption
    ///
    /// # Errors
    ///
    /// Returns an error if database operations or encryption setup fails
    pub fn new_with_encryption(
        db_path: Option<PathBuf>,
        encryption_key: Option<String>,
    ) -> Result<Self> {
        let path = db_path.unwrap_or_else(Self::default_db_path);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create database directory")?;
        }

        let conn = Connection::open(&path).context("Failed to open database connection")?;

        // Initialize encryption if key provided
        let encryption_enabled = encryption_key.is_some();
        if let Some(key) = encryption_key {
            encryption::init_encryption(&conn, &key)?;
        }

        // Initialize schema
        migrations::init_schema(&conn)?;
        migrations::insert_default_categories(&conn)?;

        log::info!(
            "Database initialized at: {} (encryption: {})",
            path.display(),
            if encryption_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );

        Ok(Self { conn })
    }

    /// Get default database path
    fn default_db_path() -> PathBuf {
        let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("toki");
        path.push("toki.db");
        path
    }

    // ==================== Activity Methods ====================

    /// Insert a new activity record
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert operation fails
    pub fn insert_activity(&self, activity: &Activity) -> Result<()> {
        self.conn.execute(
            "INSERT INTO activities (id, timestamp, app_bundle_id, category, duration_seconds, is_active, work_item_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                activity.id.to_string(),
                activity.timestamp.to_rfc3339(),
                activity.app_bundle_id,
                activity.category,
                activity.duration_seconds,
                i32::from(activity.is_active),
                activity.work_item_id.map(|id| id.to_string()),
            ],
        )?;
        Ok(())
    }

    /// Get activities for a specific date range
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID parsing or datetime parsing fails for corrupted database entries
    pub fn get_activities(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Activity>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, app_bundle_id, category, duration_seconds, is_active, work_item_id
             FROM activities
             WHERE timestamp BETWEEN ?1 AND ?2
             ORDER BY timestamp DESC",
        )?;

        let activities = stmt
            .query_map(params![start.to_rfc3339(), end.to_rfc3339()], |row| {
                Ok(Activity {
                    id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    app_bundle_id: row.get(2)?,
                    category: row.get(3)?,
                    duration_seconds: row.get(4)?,
                    is_active: row.get::<_, i32>(5)? != 0,
                    work_item_id: row
                        .get::<_, Option<String>>(6)?
                        .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(activities)
    }

    /// Delete activities in a date range
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete operation fails
    pub fn delete_activities(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM activities WHERE timestamp BETWEEN ?1 AND ?2",
            params![start.to_rfc3339(), end.to_rfc3339()],
        )?;
        Ok(deleted)
    }

    /// Get all activities for a specific work item
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID parsing or datetime parsing fails for corrupted database entries
    pub fn get_activities_by_work_item(&self, work_item_id: uuid::Uuid) -> Result<Vec<Activity>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, app_bundle_id, category, duration_seconds, is_active, work_item_id
             FROM activities
             WHERE work_item_id = ?1
             ORDER BY timestamp DESC",
        )?;

        let activities = stmt
            .query_map(params![work_item_id.to_string()], |row| {
                Ok(Activity {
                    id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    app_bundle_id: row.get(2)?,
                    category: row.get(3)?,
                    duration_seconds: row.get(4)?,
                    is_active: row.get::<_, i32>(5)? != 0,
                    work_item_id: row
                        .get::<_, Option<String>>(6)?
                        .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(activities)
    }

    // ==================== Category Methods ====================

    /// Insert or update a category
    ///
    /// # Errors
    ///
    /// Returns an error if the database upsert operation fails
    pub fn upsert_category(&self, category: &Category) -> Result<()> {
        self.conn.execute(
            "INSERT INTO categories (id, name, pattern, description)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET
                pattern = ?3,
                description = ?4",
            params![
                category.id.to_string(),
                category.name,
                category.pattern,
                category.description,
            ],
        )?;
        Ok(())
    }

    /// Get all categories
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID parsing fails for corrupted database entries
    pub fn get_categories(&self) -> Result<Vec<Category>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, pattern, description FROM categories ORDER BY name")?;

        let categories = stmt
            .query_map([], |row| {
                Ok(Category {
                    id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    name: row.get(1)?,
                    pattern: row.get(2)?,
                    description: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(categories)
    }

    // ==================== Settings Methods ====================

    /// Get or create settings
    ///
    /// # Errors
    ///
    /// Returns an error if the database query or insert operation fails
    ///
    /// # Panics
    ///
    /// May panic if UUID parsing or JSON deserialization fails for corrupted database entries
    pub fn get_settings(&self) -> Result<Settings> {
        let result: Option<Settings> = self
            .conn
            .query_row(
                "SELECT id, pause_tracking, excluded_apps, idle_threshold_seconds,
                        enable_work_item_tracking, capture_window_title, capture_browser_url, url_whitelist
                 FROM settings LIMIT 1",
                [],
                |row| {
                    let excluded_apps_json: String = row.get(2)?;
                    let excluded_apps: Vec<String> =
                        serde_json::from_str(&excluded_apps_json).unwrap_or_default();

                    let url_whitelist_json: String = row
                        .get::<_, Option<String>>(7)?
                        .unwrap_or_else(|| "[]".to_string());
                    let url_whitelist: Vec<String> =
                        serde_json::from_str(&url_whitelist_json).unwrap_or_default();

                    Ok(Settings {
                        id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                        pause_tracking: row.get::<_, i32>(1)? != 0,
                        excluded_apps,
                        idle_threshold_seconds: row.get(3)?,
                        enable_work_item_tracking: row.get::<_, Option<i32>>(4)?.unwrap_or(0) != 0,
                        capture_window_title: row.get::<_, Option<i32>>(5)?.unwrap_or(0) != 0,
                        capture_browser_url: row.get::<_, Option<i32>>(6)?.unwrap_or(0) != 0,
                        url_whitelist,
                    })
                },
            )
            .optional()?;

        if let Some(settings) = result {
            Ok(settings)
        } else {
            // Create default settings
            let settings = Settings::default_settings();
            self.update_settings(&settings)?;
            Ok(settings)
        }
    }

    /// Update settings
    ///
    /// # Errors
    ///
    /// Returns an error if the database update operation or JSON serialization fails
    pub fn update_settings(&self, settings: &Settings) -> Result<()> {
        let excluded_apps_json = serde_json::to_string(&settings.excluded_apps)?;
        let url_whitelist_json = serde_json::to_string(&settings.url_whitelist)?;

        self.conn.execute(
            "INSERT INTO settings (id, pause_tracking, excluded_apps, idle_threshold_seconds,
                                   enable_work_item_tracking, capture_window_title, capture_browser_url, url_whitelist)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                pause_tracking = ?2,
                excluded_apps = ?3,
                idle_threshold_seconds = ?4,
                enable_work_item_tracking = ?5,
                capture_window_title = ?6,
                capture_browser_url = ?7,
                url_whitelist = ?8",
            params![
                settings.id.to_string(),
                i32::from(settings.pause_tracking),
                excluded_apps_json,
                settings.idle_threshold_seconds,
                i32::from(settings.enable_work_item_tracking),
                i32::from(settings.capture_window_title),
                i32::from(settings.capture_browser_url),
                url_whitelist_json,
            ],
        )?;
        Ok(())
    }

    // ==================== Work Item Methods ====================

    /// Insert or update a work item
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert/update operation fails
    pub fn upsert_work_item(&self, work_item: &WorkItem) -> Result<()> {
        self.conn.execute(
            "INSERT INTO work_items (id, external_id, external_system, title, description, status, project, workspace, last_synced)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(external_id, external_system) DO UPDATE SET
                title = excluded.title,
                description = excluded.description,
                status = excluded.status,
                project = excluded.project,
                workspace = excluded.workspace,
                last_synced = excluded.last_synced",
            params![
                work_item.id.to_string(),
                work_item.external_id,
                work_item.external_system,
                work_item.title,
                work_item.description,
                work_item.status,
                work_item.project,
                work_item.workspace,
                work_item.last_synced.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    /// Get a work item by external ID and system
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID parsing or datetime parsing fails for corrupted database entries
    pub fn get_work_item(
        &self,
        external_id: &str,
        external_system: &str,
    ) -> Result<Option<WorkItem>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, external_id, external_system, title, description, status, project, workspace, last_synced
                 FROM work_items
                 WHERE external_id = ?1 AND external_system = ?2",
                params![external_id, external_system],
                Self::row_to_work_item,
            )
            .optional()?;

        Ok(result)
    }

    /// Get all work items
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID parsing or datetime parsing fails for corrupted database entries
    pub fn get_all_work_items(&self) -> Result<Vec<WorkItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, external_id, external_system, title, description, status, project, workspace, last_synced
             FROM work_items
             ORDER BY last_synced DESC",
        )?;

        let work_items = stmt
            .query_map([], Self::row_to_work_item)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(work_items)
    }

    /// Get a work item by its internal UUID
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_work_item_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkItem>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, external_id, external_system, title, description, status, project, workspace, last_synced
                 FROM work_items
                 WHERE id = ?1",
                params![id.to_string()],
                Self::row_to_work_item,
            )
            .optional()?;

        Ok(result)
    }

    /// Helper function to parse `WorkItem` from database row
    fn row_to_work_item(row: &rusqlite::Row) -> rusqlite::Result<WorkItem> {
        Ok(WorkItem {
            id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
            external_id: row.get(1)?,
            external_system: row.get(2)?,
            title: row.get(3)?,
            description: row.get(4)?,
            status: row.get(5)?,
            project: row.get(6)?,
            workspace: row.get(7)?,
            last_synced: row
                .get::<_, Option<String>>(8)?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
        })
    }

    // ==================== Time Block Methods ====================

    /// Get confirmed (reviewed) time blocks for syncing
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_confirmed_time_blocks(&self) -> Result<Vec<crate::models::TimeBlock>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_time, end_time, project_id, work_item_ids, description, tags, source, confidence, confirmed, created_at
             FROM time_blocks
             WHERE confirmed = 1 AND synced = 0
             ORDER BY start_time ASC",
        )?;

        let blocks = stmt
            .query_map([], |row| {
                let work_item_ids_json: String = row.get(4)?;
                let tags_json: String = row.get(6)?;
                let source_str: String = row.get(7)?;

                Ok(crate::models::TimeBlock {
                    id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    start_time: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    end_time: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    project_id: row
                        .get::<_, Option<String>>(3)?
                        .and_then(|s| uuid::Uuid::parse_str(&s).ok()),
                    work_item_ids: serde_json::from_str(&work_item_ids_json).unwrap_or_default(),
                    description: row.get(5)?,
                    tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                    source: match source_str.as_str() {
                        "Manual" => crate::models::TimeBlockSource::Manual,
                        "AiSuggested" => crate::models::TimeBlockSource::AiSuggested,
                        _ => crate::models::TimeBlockSource::AutoDetected,
                    },
                    confidence: row.get(8)?,
                    confirmed: row.get::<_, i32>(9)? != 0,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                        .unwrap()
                        .with_timezone(&Utc),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(blocks)
    }

    /// Save a time block to the database
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn save_time_block(&self, block: &crate::models::TimeBlock) -> Result<()> {
        let work_item_ids_json = serde_json::to_string(&block.work_item_ids)?;
        let tags_json = serde_json::to_string(&block.tags)?;
        let source_str = match block.source {
            crate::models::TimeBlockSource::Manual => "Manual",
            crate::models::TimeBlockSource::AiSuggested => "AiSuggested",
            crate::models::TimeBlockSource::AutoDetected => "AutoDetected",
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO time_blocks (id, start_time, end_time, project_id, work_item_ids, description, tags, source, confidence, confirmed, synced, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11)",
            params![
                block.id.to_string(),
                block.start_time.to_rfc3339(),
                block.end_time.to_rfc3339(),
                block.project_id.map(|id| id.to_string()),
                work_item_ids_json,
                block.description,
                tags_json,
                source_str,
                block.confidence,
                i32::from(block.confirmed),
                block.created_at.to_rfc3339(),
            ],
        )?;

        log::debug!("Saved time block: {}", block.id);
        Ok(())
    }

    /// Mark a time block as synced
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn mark_time_block_synced(&self, block_id: uuid::Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE time_blocks SET synced = 1 WHERE id = ?1",
            params![block_id.to_string()],
        )?;
        Ok(())
    }

    /// Confirm a time block (mark as reviewed)
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn confirm_time_block(&self, block_id: uuid::Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE time_blocks SET confirmed = 1 WHERE id = ?1",
            params![block_id.to_string()],
        )?;
        Ok(())
    }

    // ==================== Integration Config Methods ====================

    /// Get integration configuration by system type
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID parsing or datetime parsing fails for corrupted database entries
    pub fn get_integration_config(&self, system_type: &str) -> Result<Option<IntegrationConfig>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, system_type, api_url, api_key, workspace_slug, project_id, created_at, updated_at
                 FROM integration_configs
                 WHERE system_type = ?1",
                params![system_type],
                |row| {
                    Ok(IntegrationConfig {
                        id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                        system_type: row.get(1)?,
                        api_url: row.get(2)?,
                        api_key: row.get(3)?,
                        workspace_slug: row.get(4)?,
                        project_id: row.get(5)?,
                        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                            .unwrap()
                            .with_timezone(&Utc),
                        updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                            .unwrap()
                            .with_timezone(&Utc),
                    })
                },
            )
            .optional()?;

        Ok(result)
    }

    /// Upsert integration configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn upsert_integration_config(&self, config: &IntegrationConfig) -> Result<()> {
        self.conn.execute(
            "INSERT INTO integration_configs
             (id, system_type, api_url, api_key, workspace_slug, project_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(system_type) DO UPDATE SET
             api_url = excluded.api_url,
             api_key = excluded.api_key,
             workspace_slug = excluded.workspace_slug,
             project_id = excluded.project_id,
             updated_at = excluded.updated_at",
            params![
                config.id.to_string(),
                config.system_type,
                config.api_url,
                config.api_key,
                config.workspace_slug,
                config.project_id,
                config.created_at.to_rfc3339(),
                config.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    // ==================== Session Methods ====================

    /// Insert a new session
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert operation or JSON serialization fails
    pub fn insert_session(&self, session: &Session) -> Result<()> {
        let categories_json = serde_json::to_string(&session.categories)?;

        self.conn.execute(
            "INSERT INTO sessions (id, start_time, end_time, total_active_seconds, categories)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                session.id.to_string(),
                session.start_time.to_rfc3339(),
                session.end_time.map(|t| t.to_rfc3339()),
                session.total_active_seconds,
                categories_json,
            ],
        )?;
        Ok(())
    }

    /// Create a new session
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn create_session(&self, start_time: DateTime<Utc>) -> Result<uuid::Uuid> {
        let session_id = uuid::Uuid::new_v4();
        self.conn.execute(
            "INSERT INTO sessions (id, start_time, end_time, total_active_seconds, idle_seconds, interruption_count, categories, work_item_ids)
             VALUES (?1, ?2, NULL, 0, 0, 0, '[]', '[]')",
            params![session_id.to_string(), start_time.to_rfc3339()],
        )?;
        Ok(session_id)
    }

    /// Update session statistics
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn update_session_stats(
        &self,
        session_id: uuid::Uuid,
        active_seconds: u32,
        idle_seconds: u32,
        interruption_count: u32,
        categories: &[String],
        work_item_ids: &[uuid::Uuid],
    ) -> Result<()> {
        let categories_json = serde_json::to_string(categories)?;
        let work_item_ids_json = serde_json::to_string(work_item_ids)?;

        self.conn.execute(
            "UPDATE sessions
             SET total_active_seconds = ?1, idle_seconds = ?2, interruption_count = ?3,
                 categories = ?4, work_item_ids = ?5
             WHERE id = ?6",
            params![
                active_seconds,
                idle_seconds,
                interruption_count,
                categories_json,
                work_item_ids_json,
                session_id.to_string()
            ],
        )?;
        Ok(())
    }

    /// Finalize a session by setting its end time
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn finalize_session(&self, session_id: uuid::Uuid, end_time: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET end_time = ?1 WHERE id = ?2",
            params![end_time.to_rfc3339(), session_id.to_string()],
        )?;
        Ok(())
    }

    /// Get the current ongoing session (if any)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID or datetime parsing fails for corrupted database entries
    pub fn get_current_session(&self) -> Result<Option<Session>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, start_time, end_time, total_active_seconds, idle_seconds, interruption_count, categories, work_item_ids
                 FROM sessions
                 WHERE end_time IS NULL
                 ORDER BY start_time DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok(Session {
                        id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                        start_time: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
                            .unwrap()
                            .with_timezone(&Utc),
                        end_time: None,
                        total_active_seconds: row.get(3)?,
                        idle_seconds: row.get(4)?,
                        interruption_count: row.get(5)?,
                        categories: serde_json::from_str(&row.get::<_, String>(6)?)
                            .unwrap_or_default(),
                        work_item_ids: serde_json::from_str(&row.get::<_, String>(7)?)
                            .unwrap_or_default(),
                    })
                },
            )
            .optional()?;

        Ok(result)
    }

    /// Get sessions within a time range
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    ///
    /// # Panics
    ///
    /// May panic if UUID or datetime parsing fails for corrupted database entries
    pub fn get_sessions(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, start_time, end_time, total_active_seconds, idle_seconds, interruption_count, categories, work_item_ids
             FROM sessions
             WHERE start_time >= ?1 AND start_time <= ?2
             ORDER BY start_time ASC",
        )?;

        let sessions = stmt
            .query_map(params![start.to_rfc3339(), end.to_rfc3339()], |row| {
                Ok(Session {
                    id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    start_time: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    end_time: row
                        .get::<_, Option<String>>(2)?
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                    total_active_seconds: row.get(3)?,
                    idle_seconds: row.get(4)?,
                    interruption_count: row.get(5)?,
                    categories: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                    work_item_ids: serde_json::from_str(&row.get::<_, String>(7)?)
                        .unwrap_or_default(),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    // ==================== Classification Rules ====================

    /// Save a new classification rule (user correction)
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert fails
    pub fn save_classification_rule(&self, rule: &ClassificationRule) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO classification_rules
             (id, pattern, pattern_type, category, priority, created_at, hit_count, last_hit)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                rule.id.to_string(),
                rule.pattern,
                rule.pattern_type.to_string(),
                rule.category,
                rule.priority,
                rule.created_at.to_rfc3339(),
                rule.hit_count,
                rule.last_hit.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    /// Get all classification rules, ordered by priority (highest first)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_classification_rules(&self) -> Result<Vec<ClassificationRule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, pattern, pattern_type, category, priority, created_at, hit_count, last_hit
             FROM classification_rules
             ORDER BY priority DESC, hit_count DESC",
        )?;

        let rules = stmt
            .query_map([], |row| {
                Ok(ClassificationRule {
                    id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    pattern: row.get(1)?,
                    pattern_type: row
                        .get::<_, String>(2)?
                        .parse()
                        .unwrap_or(PatternType::WindowTitle),
                    category: row.get(3)?,
                    priority: row.get(4)?,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    hit_count: row.get(6)?,
                    last_hit: row
                        .get::<_, Option<String>>(7)?
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rules)
    }

    /// Record a hit for a classification rule
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails
    pub fn record_rule_hit(&self, rule_id: uuid::Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE classification_rules
             SET hit_count = hit_count + 1, last_hit = ?1
             WHERE id = ?2",
            params![Utc::now().to_rfc3339(), rule_id.to_string()],
        )?;
        Ok(())
    }

    /// Delete a classification rule
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails
    pub fn delete_classification_rule(&self, rule_id: uuid::Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM classification_rules WHERE id = ?1",
            params![rule_id.to_string()],
        )?;
        Ok(())
    }

    /// Find existing rule by pattern and type
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn find_rule_by_pattern(
        &self,
        pattern: &str,
        pattern_type: &PatternType,
    ) -> Result<Option<ClassificationRule>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, pattern, pattern_type, category, priority, created_at, hit_count, last_hit
                 FROM classification_rules
                 WHERE pattern = ?1 AND pattern_type = ?2",
                params![pattern, pattern_type.to_string()],
                |row| {
                    Ok(ClassificationRule {
                        id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                        pattern: row.get(1)?,
                        pattern_type: row
                            .get::<_, String>(2)?
                            .parse()
                            .unwrap_or(PatternType::WindowTitle),
                        category: row.get(3)?,
                        priority: row.get(4)?,
                        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                            .unwrap()
                            .with_timezone(&Utc),
                        hit_count: row.get(6)?,
                        last_hit: row
                            .get::<_, Option<String>>(7)?
                            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                            .map(|dt| dt.with_timezone(&Utc)),
                    })
                },
            )
            .optional()?;
        Ok(result)
    }
}
