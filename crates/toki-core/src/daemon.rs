use crate::{
    classifier::Classifier,
    config::get_data_dir,
    ipc::{listen, DaemonIpcHandler},
    monitor::{create_monitor, SystemMonitor},
    session_manager::SessionManager,
};
use anyhow::Result;
use std::{sync::Arc, time::Duration};
use toki_detector::WorkContextDetector;
use toki_storage::{ActivitySpan, Database};
use tokio::time::interval;
use uuid::Uuid;

pub struct Daemon {
    database: Arc<Database>,
    monitor: Box<dyn SystemMonitor>,
    classifier: Classifier,
    context_detector: WorkContextDetector,
    session_manager: SessionManager,
    ipc_handler: Arc<DaemonIpcHandler>,
    shutdown_signal: Arc<std::sync::atomic::AtomicBool>,
    current_activity_span: Option<ActivitySpan>,
    current_session_id: Option<Uuid>,
    current_project_id: Option<Uuid>,   // Primary: which project
    current_work_item_id: Option<Uuid>, // Optional: specific issue
    session_active_seconds: u32,
    session_idle_seconds: u32,
    tick_interval_seconds: u64,
}

impl Daemon {
    pub fn new(db: Database, tick_interval_seconds: u64) -> Result<Self> {
        let db = Arc::new(db);
        let shutdown_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

        Ok(Self {
            database: db.clone(),
            monitor: create_monitor()?,
            classifier: Classifier::from_database_arc(db.clone())?,
            context_detector: WorkContextDetector::new(),
            session_manager: SessionManager::new(db.clone()),
            ipc_handler: Arc::new(DaemonIpcHandler::new(shutdown_signal.clone())),
            shutdown_signal,
            current_activity_span: None,
            current_session_id: None,
            current_project_id: None,
            current_work_item_id: None,
            session_active_seconds: 0,
            session_idle_seconds: 0,
            tick_interval_seconds,
        })
    }

    pub async fn run_with_signals(&mut self) -> Result<()> {
        let sock_path = get_data_dir()?.join("toki.sock");
        let ipc_handler = self.ipc_handler.clone();

        tokio::spawn(async move {
            if let Err(e) = listen(ipc_handler, &sock_path).await {
                log::error!("IPC listener failed: {e}");
            }
        });

        let mut interval = interval(Duration::from_secs(self.tick_interval_seconds));
        log::info!("Daemon started with signal handling and IPC");

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.tick().await {
                        log::error!("Daemon tick failed: {e}");
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    log::info!("Received Ctrl-C, shutting down...");
                    self.shutdown_signal.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }

            if self
                .shutdown_signal
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                break;
            }
        }

        // Finalize current activity and session on shutdown
        self.finalize_current_span().await?;
        self.finalize_current_session()?;
        log::info!("Daemon shut down gracefully.");
        Ok(())
    }

    #[allow(clippy::cognitive_complexity)]
    async fn tick(&mut self) -> Result<()> {
        let settings = self.database.get_settings()?;
        let now = chrono::Utc::now();
        let tick_seconds = self.tick_interval_seconds as u32;

        // Check if tracking is paused
        if settings.pause_tracking {
            self.finalize_current_span().await?;
            self.finalize_current_session()?;
            return Ok(());
        }

        // Check idle state
        let is_idle = self
            .monitor
            .is_idle(settings.idle_threshold_seconds)
            .await?;

        // Session management based on work hours and idle state
        if self.session_manager.should_start_session(now)
            && self.current_session_id.is_none()
            && !is_idle
        {
            let session_id = self.session_manager.create_session()?;
            self.current_session_id = Some(session_id);
            self.session_active_seconds = 0;
            self.session_idle_seconds = 0;
            log::info!("Started new work session: {session_id}");
        }

        if is_idle {
            self.session_idle_seconds += tick_seconds;
            self.finalize_current_span().await?;

            if self
                .session_manager
                .should_end_session(self.session_idle_seconds, now)
            {
                self.finalize_current_session()?;
            }
            return Ok(());
        }

        // Active tracking - reset idle counter
        self.session_idle_seconds = 0;
        self.session_active_seconds += tick_seconds;

        let app_activity = self.monitor.get_active_app().await?;
        let window_title = app_activity.as_ref().and_then(|a| a.window_title.clone());

        // Log the detected app for debugging
        if let Some(ref app) = app_activity {
            log::debug!(
                "Active app: {} ({}) - window: {:?}",
                app.app_name,
                app.app_id,
                app.window_title
            );
        }

        // Detect project (primary) and work item (optional)
        let (project_id, work_item_id) = self
            .detect_project_and_work_item(window_title.as_deref())
            .await?;

        // Update IPC status
        self.ipc_handler
            .set_current_window(app_activity.as_ref().map(|a| a.app_id.clone()))
            .await;

        if let Some(app) = app_activity {
            if settings.excluded_apps.contains(&app.app_id) {
                self.finalize_current_span().await?;
                return Ok(());
            }

            // Use window title for better classification (e.g., AI CLI tools in terminal)
            let category = self
                .classifier
                .classify_with_context(&app.app_id, window_title.as_deref());

            // Only create new span when APP changes (not when project changes within same app)
            // This allows natural multi-window workflows without fragmenting time tracking
            let current_span = self.current_activity_span.as_ref();
            let app_changed = match current_span {
                Some(span) => span.app_bundle_id != app.app_id,
                None => true,
            };

            if app_changed {
                log::info!(
                    "App changed to {} ({}), creating new span",
                    app.app_name,
                    app.app_id
                );
                self.finalize_current_span().await?;
                self.current_project_id = project_id;
                self.current_work_item_id = work_item_id;
                self.start_new_span(app.app_id, category.to_string(), project_id, work_item_id)
                    .await?;
            } else {
                // App is the same - update project tracking without creating new span
                // Track time spent per project in parallel
                if let Some(pid) = project_id {
                    self.track_project_time(pid).await?;
                }
                // Update current context (for IPC status display)
                self.current_project_id = project_id;
                self.current_work_item_id = work_item_id;
            }
        } else {
            self.finalize_current_span().await?;
        }

        // Update session stats periodically
        if let Some(session_id) = self.current_session_id {
            if let Err(e) = self.session_manager.update_session_stats(
                session_id,
                self.session_active_seconds,
                self.session_idle_seconds,
                0,
            ) {
                log::warn!("Failed to update session stats: {e}");
            }
        }

        Ok(())
    }

    /// Detect project (primary) and optionally work item from context
    /// Project = the workspace/codebase being worked on
    /// Work item = optional issue ID (from git branch, commit, etc.)
    async fn detect_project_and_work_item(
        &self,
        window_title: Option<&str>,
    ) -> Result<(Option<Uuid>, Option<Uuid>)> {
        let settings = self.database.get_settings()?;
        if !settings.enable_work_item_tracking {
            return Ok((None, None));
        }

        log::debug!("Detecting project from window_title: {window_title:?}");

        // Try to get workspace path from IDE
        let workspace_path = self.context_detector.get_workspace_path(window_title).await;

        let project_id = if let Ok(Some(path)) = workspace_path {
            // Get project name from path
            let project_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let path_str = path.to_string_lossy().to_string();
            log::debug!("Detected project: {project_name} at {path_str}");

            // Get or create project
            let project = self
                .database
                .get_or_create_project(&project_name, &path_str)?;

            // Update IPC with project name
            self.ipc_handler.set_current_issue(Some(project_name)).await;

            // Try to detect work item from git (optional)
            let work_item_id = self.detect_work_item_from_git(&path).await?;

            Some((project.id, work_item_id))
        } else {
            self.ipc_handler.set_current_issue(None).await;
            None
        };

        match project_id {
            Some((pid, wid)) => Ok((Some(pid), wid)),
            None => Ok((None, None)),
        }
    }

    /// Try to detect work item ID from git branch (optional enrichment)
    async fn detect_work_item_from_git(
        &self,
        workspace_path: &std::path::Path,
    ) -> Result<Option<Uuid>> {
        // Use context detector to find issue ID from git
        if let Ok(Some(work_ref)) = self.context_detector.detect_from_path(workspace_path).await {
            let issue_id_str = work_ref.issue_id.full_id();
            let external_system = work_ref.source.to_string();

            let work_item = if let Some(item) = self
                .database
                .get_work_item(&issue_id_str, &external_system)?
            {
                item
            } else {
                let new_item = toki_storage::WorkItem::new(issue_id_str, external_system);
                self.database.upsert_work_item(&new_item)?;
                new_item
            };
            return Ok(Some(work_item.id));
        }
        Ok(None)
    }

    async fn start_new_span(
        &mut self,
        app_bundle_id: String,
        category: String,
        project_id: Option<Uuid>,
        work_item_id: Option<Uuid>,
    ) -> Result<()> {
        let span = ActivitySpan::new(
            app_bundle_id,
            category,
            chrono::Utc::now(),
            project_id,
            work_item_id, // Primary work item (auto-detected or None)
            self.current_session_id,
        );
        // Note: Context (git branch, edited files, etc.) can be enriched later
        // through the CLI `toki tag` command or AI analysis

        self.database.create_activity_span(&span)?;
        self.current_activity_span = Some(span);
        Ok(())
    }

    async fn finalize_current_span(&mut self) -> Result<()> {
        if let Some(span) = self.current_activity_span.take() {
            self.database
                .finalize_activity_span(span.id, chrono::Utc::now())?;
        }
        Ok(())
    }

    fn finalize_current_session(&mut self) -> Result<()> {
        if let Some(session_id) = self.current_session_id.take() {
            self.session_manager.finalize_session(session_id)?;
            log::info!(
                "Finalized session: {session_id} (active: {}s, idle: {}s)",
                self.session_active_seconds,
                self.session_idle_seconds
            );
            self.session_active_seconds = 0;
            self.session_idle_seconds = 0;
        }
        Ok(())
    }

    /// Track time spent on a project (for multi-window workflows)
    /// This updates `project_time` table without creating new activity spans
    async fn track_project_time(&mut self, project_id: Uuid) -> Result<()> {
        let tick_seconds = self.tick_interval_seconds as u32;
        self.database
            .add_project_time(project_id, tick_seconds, chrono::Utc::now())?;
        Ok(())
    }
}
