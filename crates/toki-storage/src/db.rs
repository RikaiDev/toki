use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

use crate::encryption;
use crate::migrations;
use crate::models::{
    Activity, ActivitySpan, Category, ClassificationRule, IntegrationConfig, IssueCandidate,
    PatternType, Project, Session, Settings, SyncedIssue, WorkItem,
};

/// Database connection wrapper
pub struct Database {
    conn: Connection,
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

                    let url_whitelist_json: String = row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "[]".to_string());
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

    /// Insert or update a work item
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert/update operation fails
    pub fn upsert_work_item(&self, work_item: &crate::models::WorkItem) -> Result<()> {
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
    ) -> Result<Option<crate::models::WorkItem>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, external_id, external_system, title, description, status, project, workspace, last_synced
                 FROM work_items
                 WHERE external_id = ?1 AND external_system = ?2",
                params![external_id, external_system],
                |row| {
                    Ok(crate::models::WorkItem {
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
                },
            )
            .optional()?;

        Ok(result)
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
            .query_map([], |row| {
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
            })?
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
                |row| {
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
                },
            )
            .optional()?;

        Ok(result)
    }

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

    // ==================== Project Methods ====================

    /// Get or create a project by path
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn get_or_create_project(&self, name: &str, path: &str) -> Result<Project> {
        // Try to find existing project by path
        if let Some(project) = self.get_project_by_path(path)? {
            // Update last_active
            self.conn.execute(
                "UPDATE projects SET last_active = ?1 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), project.id.to_string()],
            )?;
            return Ok(project);
        }

        // Create new project
        let project = Project::new(name.to_string(), path.to_string());
        self.conn.execute(
            "INSERT INTO projects (id, name, path, description, created_at, last_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                project.id.to_string(),
                project.name,
                project.path,
                project.description,
                project.created_at.to_rfc3339(),
                project.last_active.to_rfc3339(),
            ],
        )?;
        log::info!("Created new project: {name} at {path}");
        Ok(project)
    }

    /// Get a project by path
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_project_by_path(&self, path: &str) -> Result<Option<Project>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, name, path, description, created_at, last_active, pm_system, pm_project_id, pm_workspace
                 FROM projects WHERE path = ?1",
                params![path],
                |row| {
                    Ok(Project {
                        id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                        name: row.get(1)?,
                        path: row.get(2)?,
                        description: row.get(3)?,
                        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                            .unwrap()
                            .with_timezone(&Utc),
                        last_active: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                            .unwrap()
                            .with_timezone(&Utc),
                        pm_system: row.get(6)?,
                        pm_project_id: row.get(7)?,
                        pm_workspace: row.get(8)?,
                    })
                },
            )
            .optional()?;

        Ok(result)
    }

    /// Get a project by ID
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_project(&self, project_id: uuid::Uuid) -> Result<Option<Project>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, name, path, description, created_at, last_active, pm_system, pm_project_id, pm_workspace
                 FROM projects WHERE id = ?1",
                params![project_id.to_string()],
                |row| {
                    Ok(Project {
                        id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                        name: row.get(1)?,
                        path: row.get(2)?,
                        description: row.get(3)?,
                        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                            .unwrap()
                            .with_timezone(&Utc),
                        last_active: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                            .unwrap()
                            .with_timezone(&Utc),
                        pm_system: row.get(6)?,
                        pm_project_id: row.get(7)?,
                        pm_workspace: row.get(8)?,
                    })
                },
            )
            .optional()?;

        Ok(result)
    }

    /// Get all projects
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_all_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, path, description, created_at, last_active, pm_system, pm_project_id, pm_workspace
             FROM projects ORDER BY last_active DESC",
        )?;

        let projects = stmt
            .query_map([], |row| {
                Ok(Project {
                    id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    name: row.get(1)?,
                    path: row.get(2)?,
                    description: row.get(3)?,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    last_active: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    pm_system: row.get(6)?,
                    pm_project_id: row.get(7)?,
                    pm_workspace: row.get(8)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(projects)
    }

    /// Link a project to a PM system
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails
    pub fn link_project_to_pm(
        &self,
        project_id: uuid::Uuid,
        pm_system: &str,
        pm_project_id: &str,
        pm_workspace: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE projects SET pm_system = ?1, pm_project_id = ?2, pm_workspace = ?3 WHERE id = ?4",
            params![pm_system, pm_project_id, pm_workspace, project_id.to_string()],
        )?;
        log::info!("Linked project {project_id} to {pm_system} project {pm_project_id}");
        Ok(())
    }

    /// Add time to a project for the current day
    /// This supports multi-window workflows where user frequently switches between projects
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn add_project_time(
        &self,
        project_id: uuid::Uuid,
        seconds: u32,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let date = now.format("%Y-%m-%d").to_string();
        let id = uuid::Uuid::new_v4().to_string();

        // Upsert: insert or update if exists
        self.conn.execute(
            "INSERT INTO project_time (id, project_id, date, duration_seconds, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(project_id, date) DO UPDATE SET
                duration_seconds = duration_seconds + ?4,
                updated_at = ?5",
            params![
                id,
                project_id.to_string(),
                date,
                seconds,
                now.to_rfc3339(),
            ],
        )?;

        // Also update the project's last_active timestamp
        self.conn.execute(
            "UPDATE projects SET last_active = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), project_id.to_string()],
        )?;

        Ok(())
    }

    /// Get project time for a specific date
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_project_time_for_date(
        &self,
        date: &str,
    ) -> Result<Vec<(Project, u32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.name, p.path, p.description, p.created_at, p.last_active,
                    p.pm_system, p.pm_project_id, p.pm_workspace, pt.duration_seconds
             FROM project_time pt
             JOIN projects p ON pt.project_id = p.id
             WHERE pt.date = ?1
             ORDER BY pt.duration_seconds DESC",
        )?;

        let results = stmt
            .query_map(params![date], |row| {
                let project = Project {
                    id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    name: row.get(1)?,
                    path: row.get(2)?,
                    description: row.get(3)?,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    last_active: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    pm_system: row.get(6)?,
                    pm_project_id: row.get(7)?,
                    pm_workspace: row.get(8)?,
                };
                let duration: u32 = row.get(9)?;
                Ok((project, duration))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(results)
    }

    // ==================== ActivitySpan Methods ====================

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
        context: &crate::models::ActivitySpanContext,
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
    fn row_to_activity_span(row: &rusqlite::Row) -> rusqlite::Result<ActivitySpan> {
        let context_json: Option<String> = row.get(9)?;
        let context = context_json.and_then(|s| serde_json::from_str(&s).ok());

        Ok(ActivitySpan {
            id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
            app_bundle_id: row.get(1)?,
            category: row.get(2)?,
            start_time: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                .unwrap()
                .with_timezone(&Utc),
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
                        id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                        app_bundle_id: row.get(1)?,
                        category: row.get(2)?,
                        start_time: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                            .unwrap()
                            .with_timezone(&Utc),
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

    // ==================== Session Management Methods ====================

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
                        categories: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                        work_item_ids: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
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

    // ==================== Embedding Methods ====================

    /// Save project embedding
    pub fn save_project_embedding(&self, project_id: uuid::Uuid, embedding: &[f32]) -> Result<()> {
        // Store as BLOB (raw bytes)
        // Using standard f32 representation
        let bytes: Vec<u8> = embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        self.conn.execute(
            "UPDATE projects SET embedding = ?1 WHERE id = ?2",
            params![bytes, project_id.to_string()],
        )?;
        Ok(())
    }

    /// Get project embedding
    pub fn get_project_embedding(&self, project_id: uuid::Uuid) -> Result<Option<Vec<f32>>> {
        let result = self.conn.query_row(
            "SELECT embedding FROM projects WHERE id = ?1",
            params![project_id.to_string()],
            |row| {
                let bytes: Option<Vec<u8>> = row.get(0)?;
                match bytes {
                    Some(b) => {
                        // Convert bytes back to f32
                        // Each f32 is 4 bytes
                        if b.len() % 4 != 0 {
                            return Ok(None); // Invalid data
                        }

                        let embedding: Vec<f32> = b
                            .chunks_exact(4)
                            .map(|chunk| {
                                let arr = [chunk[0], chunk[1], chunk[2], chunk[3]];
                                f32::from_le_bytes(arr)
                            })
                            .collect();
                        Ok(Some(embedding))
                    }
                    None => Ok(None),
                }
            }
        ).optional()?;

        Ok(result.flatten())
    }

    // ==================== Issue Candidate Methods ====================

    /// Upsert an issue candidate (for AI matching)
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn upsert_issue_candidate(&self, candidate: &IssueCandidate) -> Result<()> {
        let labels_json = serde_json::to_string(&candidate.labels)?;
        let embedding_bytes: Option<Vec<u8>> = candidate.embedding.as_ref().map(|e| {
            e.iter().flat_map(|f| f.to_le_bytes()).collect()
        });

        self.conn.execute(
            "INSERT INTO issue_candidates
             (id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
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
                last_synced = excluded.last_synced",
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
            ],
        )?;
        Ok(())
    }

    /// Update issue candidate embedding
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn update_issue_embedding(&self, candidate_id: uuid::Uuid, embedding: &[f32]) -> Result<()> {
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
            "SELECT id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced
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
            "SELECT id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced
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
                "SELECT id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced
                 FROM issue_candidates
                 WHERE external_id = ?1 AND external_system = ?2",
                params![external_id, external_system],
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

    /// Get projects that have PM system linked
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_projects_with_pm_link(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, path, description, created_at, last_active, pm_system, pm_project_id, pm_workspace
             FROM projects
             WHERE pm_system IS NOT NULL AND pm_project_id IS NOT NULL
             ORDER BY last_active DESC",
        )?;

        let projects = stmt
            .query_map([], |row| {
                Ok(Project {
                    id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    name: row.get(1)?,
                    path: row.get(2)?,
                    description: row.get(3)?,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    last_active: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    pm_system: row.get(6)?,
                    pm_project_id: row.get(7)?,
                    pm_workspace: row.get(8)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(projects)
    }

    /// Helper function to parse `IssueCandidate` from database row
    fn row_to_issue_candidate(row: &rusqlite::Row) -> rusqlite::Result<IssueCandidate> {
        // Column order: id, project_id, external_id, external_system, pm_project_id, source_page_id, title, description, status, labels, assignee, embedding, last_synced
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
        })
    }

    /// Get a project by name
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails
    pub fn get_project_by_name(&self, name: &str) -> Result<Option<Project>> {
        let result = self
            .conn
            .query_row(
                "SELECT id, name, path, description, created_at, last_active, pm_system, pm_project_id, pm_workspace
                 FROM projects WHERE name = ?1",
                params![name],
                |row| {
                    Ok(Project {
                        id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                        name: row.get(1)?,
                        path: row.get(2)?,
                        description: row.get(3)?,
                        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                            .unwrap()
                            .with_timezone(&Utc),
                        last_active: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                            .unwrap()
                            .with_timezone(&Utc),
                        pm_system: row.get(6)?,
                        pm_project_id: row.get(7)?,
                        pm_workspace: row.get(8)?,
                    })
                },
            )
            .optional()?;

        Ok(result)
    }

    // ==================== Synced Issues Methods ====================

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
            .query_map(params![target_system, target_project], Self::row_to_synced_issue)?
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
    fn row_to_synced_issue(row: &rusqlite::Row) -> rusqlite::Result<SyncedIssue> {
        Ok(SyncedIssue {
            id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
            source_page_id: row.get(1)?,
            source_database_id: row.get(2)?,
            target_system: row.get(3)?,
            target_project: row.get(4)?,
            target_issue_id: row.get(5)?,
            target_issue_number: row.get(6)?,
            target_issue_url: row.get(7)?,
            title: row.get(8)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                .unwrap()
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                .unwrap()
                .with_timezone(&Utc),
        })
    }
}
