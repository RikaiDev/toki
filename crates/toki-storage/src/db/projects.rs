use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use crate::models::Project;

use super::Database;

impl Database {
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
                Self::row_to_project,
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
                Self::row_to_project,
            )
            .optional()?;

        Ok(result)
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
                Self::row_to_project,
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
            .query_map([], Self::row_to_project)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(projects)
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
            .query_map([], Self::row_to_project)?
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
            params![id, project_id.to_string(), date, seconds, now.to_rfc3339(),],
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
    pub fn get_project_time_for_date(&self, date: &str) -> Result<Vec<(Project, u32)>> {
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

    // ==================== Embedding Methods ====================

    /// Save project embedding
    pub fn save_project_embedding(&self, project_id: uuid::Uuid, embedding: &[f32]) -> Result<()> {
        // Store as BLOB (raw bytes)
        // Using standard f32 representation
        let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

        self.conn.execute(
            "UPDATE projects SET embedding = ?1 WHERE id = ?2",
            params![bytes, project_id.to_string()],
        )?;
        Ok(())
    }

    /// Get project embedding
    pub fn get_project_embedding(&self, project_id: uuid::Uuid) -> Result<Option<Vec<f32>>> {
        let result = self
            .conn
            .query_row(
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
                },
            )
            .optional()?;

        Ok(result.flatten())
    }

    /// Helper function to parse `Project` from database row
    pub(crate) fn row_to_project(row: &rusqlite::Row) -> rusqlite::Result<Project> {
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
    }
}
