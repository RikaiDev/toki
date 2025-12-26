use anyhow::Result;
use rusqlite::Connection;

/// Initialize database schema
///
/// # Errors
///
/// Returns an error if database table creation or index creation fails
#[allow(clippy::too_many_lines)]
pub fn init_schema(conn: &Connection) -> Result<()> {
    // Activities table - tracks individual app usage events
    conn.execute(
        "CREATE TABLE IF NOT EXISTS activities (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            app_bundle_id TEXT NOT NULL,
            category TEXT NOT NULL,
            duration_seconds INTEGER NOT NULL,
            is_active INTEGER NOT NULL
        )",
        [],
    )?;

    // Categories table - stores pattern-to-category mappings
    conn.execute(
        "CREATE TABLE IF NOT EXISTS categories (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            pattern TEXT NOT NULL,
            description TEXT
        )",
        [],
    )?;

    // Sessions table - aggregated work sessions
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            start_time TEXT NOT NULL,
            end_time TEXT,
            total_active_seconds INTEGER NOT NULL,
            idle_seconds INTEGER NOT NULL,
            interruption_count INTEGER NOT NULL,
            categories TEXT NOT NULL,
            work_item_ids TEXT NOT NULL
        )",
        [],
    )?;

    // Settings table - user preferences and privacy controls
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            id TEXT PRIMARY KEY,
            pause_tracking INTEGER NOT NULL,
            excluded_apps TEXT NOT NULL,
            idle_threshold_seconds INTEGER NOT NULL,
            enable_work_item_tracking INTEGER DEFAULT 0,
            capture_window_title INTEGER DEFAULT 0,
            capture_browser_url INTEGER DEFAULT 0,
            url_whitelist TEXT DEFAULT '[]'
        )",
        [],
    )?;

    // Add new columns to existing settings table if they don't exist
    let columns_to_add = vec![
        ("enable_work_item_tracking", "INTEGER DEFAULT 0"),
        ("capture_window_title", "INTEGER DEFAULT 0"),
        ("capture_browser_url", "INTEGER DEFAULT 0"),
        ("url_whitelist", "TEXT DEFAULT '[]'"),
    ];

    for (column_name, column_type) in columns_to_add {
        let column_exists: Result<i32, rusqlite::Error> = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM pragma_table_info('settings') WHERE name='{column_name}'"
            ),
            [],
            |row| row.get(0),
        );

        if column_exists.unwrap_or(0) == 0 {
            conn.execute(
                &format!("ALTER TABLE settings ADD COLUMN {column_name} {column_type}"),
                [],
            )?;
            log::info!("Added {column_name} column to settings table");
        }
    }

    // Projects table - workspaces being worked on (primary tracking unit)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE,
            description TEXT,
            created_at TEXT NOT NULL,
            last_active TEXT NOT NULL,
            pm_system TEXT,
            pm_project_id TEXT,
            pm_workspace TEXT
        )",
        [],
    )?;

    // Work items table - PM system tasks/issues (optional metadata)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS work_items (
            id TEXT PRIMARY KEY,
            external_id TEXT NOT NULL,
            external_system TEXT NOT NULL,
            title TEXT,
            description TEXT,
            status TEXT,
            project TEXT,
            workspace TEXT,
            last_synced TEXT,
            UNIQUE(external_id, external_system)
        )",
        [],
    )?;

    // Activity spans table - precise continuous time tracking
    conn.execute(
        "CREATE TABLE IF NOT EXISTS activity_spans (
            id TEXT PRIMARY KEY,
            app_bundle_id TEXT NOT NULL,
            category TEXT NOT NULL,
            start_time TEXT NOT NULL,
            end_time TEXT,
            duration_seconds INTEGER NOT NULL DEFAULT 0,
            project_id TEXT,
            work_item_id TEXT,
            session_id TEXT,
            context TEXT,
            FOREIGN KEY (project_id) REFERENCES projects(id),
            FOREIGN KEY (work_item_id) REFERENCES work_items(id),
            FOREIGN KEY (session_id) REFERENCES sessions(id)
        )",
        [],
    )?;

    // Integration configs table - PM system API configurations
    conn.execute(
        "CREATE TABLE IF NOT EXISTS integration_configs (
            id TEXT PRIMARY KEY,
            system_type TEXT NOT NULL UNIQUE,
            api_url TEXT NOT NULL,
            api_key TEXT NOT NULL,
            workspace_slug TEXT,
            project_id TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        [],
    )?;

    // Time blocks table - for reviewed/confirmed time entries
    conn.execute(
        "CREATE TABLE IF NOT EXISTS time_blocks (
            id TEXT PRIMARY KEY,
            start_time TEXT NOT NULL,
            end_time TEXT NOT NULL,
            project_id TEXT,
            work_item_ids TEXT NOT NULL,
            description TEXT NOT NULL,
            tags TEXT NOT NULL,
            source TEXT NOT NULL,
            confidence REAL,
            confirmed INTEGER NOT NULL DEFAULT 0,
            synced INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_time_blocks_confirmed ON time_blocks(confirmed)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_time_blocks_synced ON time_blocks(synced)",
        [],
    )?;

    // Project time table - accumulates time per project per day
    // This supports multi-window workflows where user switches between projects frequently
    conn.execute(
        "CREATE TABLE IF NOT EXISTS project_time (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            date TEXT NOT NULL,
            duration_seconds INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id),
            UNIQUE(project_id, date)
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_project_time_date ON project_time(date)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_project_time_project ON project_time(project_id)",
        [],
    )?;

    // Add new columns to sessions table for enhanced tracking
    let sessions_columns: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('sessions')")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    if !sessions_columns.contains(&"idle_seconds".to_string()) {
        conn.execute(
            "ALTER TABLE sessions ADD COLUMN idle_seconds INTEGER DEFAULT 0",
            [],
        )?;
    }

    if !sessions_columns.contains(&"interruption_count".to_string()) {
        conn.execute(
            "ALTER TABLE sessions ADD COLUMN interruption_count INTEGER DEFAULT 0",
            [],
        )?;
    }

    // Add work_item_id to activities table if it doesn't exist
    // SQLite doesn't support ALTER TABLE IF NOT EXISTS, so we check manually
    let column_exists: Result<i32, rusqlite::Error> = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('activities') WHERE name='work_item_id'",
        [],
        |row| row.get(0),
    );

    if column_exists.unwrap_or(0) == 0 {
        conn.execute(
            "ALTER TABLE activities ADD COLUMN work_item_id TEXT REFERENCES work_items(id)",
            [],
        )?;
        log::info!("Added work_item_id column to activities table");
    }

    // Create indexes for performance
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_activities_timestamp ON activities(timestamp)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_activities_category ON activities(category)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_activities_work_item ON activities(work_item_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_sessions_start_time ON sessions(start_time)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_work_items_external ON work_items(external_id, external_system)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_activity_spans_start_time ON activity_spans(start_time)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_activity_spans_work_item ON activity_spans(work_item_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_activity_spans_session ON activity_spans(session_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_projects_name ON projects(name)",
        [],
    )?;

    // Add project_id column to activity_spans if it doesn't exist (migration)
    let project_id_exists: Result<i32, rusqlite::Error> = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('activity_spans') WHERE name='project_id'",
        [],
        |row| row.get(0),
    );

    if project_id_exists.unwrap_or(0) == 0 {
        conn.execute(
            "ALTER TABLE activity_spans ADD COLUMN project_id TEXT REFERENCES projects(id)",
            [],
        )?;
        log::info!("Added project_id column to activity_spans table");
    }

    // Create index on project_id after ensuring column exists
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_activity_spans_project ON activity_spans(project_id)",
        [],
    )?;

    // Add context column to activity_spans if it doesn't exist (stores JSON for AI analysis)
    let context_exists: Result<i32, rusqlite::Error> = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('activity_spans') WHERE name='context'",
        [],
        |row| row.get(0),
    );

    if context_exists.unwrap_or(0) == 0 {
        conn.execute("ALTER TABLE activity_spans ADD COLUMN context TEXT", [])?;
        log::info!("Added context column to activity_spans table");
    }

    // Add PM columns to projects if they don't exist
    let pm_system_exists: Result<i32, rusqlite::Error> = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('projects') WHERE name='pm_system'",
        [],
        |row| row.get(0),
    );

    if pm_system_exists.unwrap_or(0) == 0 {
        conn.execute("ALTER TABLE projects ADD COLUMN pm_system TEXT", [])?;
        conn.execute("ALTER TABLE projects ADD COLUMN pm_project_id TEXT", [])?;
        conn.execute("ALTER TABLE projects ADD COLUMN pm_workspace TEXT", [])?;
        log::info!("Added PM integration columns to projects table");
    }

    // User classification rules table - learns from user corrections
    // Priority: user rules > contextual rules > regex rules
    conn.execute(
        "CREATE TABLE IF NOT EXISTS classification_rules (
            id TEXT PRIMARY KEY,
            pattern TEXT NOT NULL,
            pattern_type TEXT NOT NULL,
            category TEXT NOT NULL,
            priority INTEGER NOT NULL DEFAULT 100,
            created_at TEXT NOT NULL,
            hit_count INTEGER NOT NULL DEFAULT 0,
            last_hit TEXT,
            UNIQUE(pattern, pattern_type)
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_classification_rules_pattern ON classification_rules(pattern)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_classification_rules_priority ON classification_rules(priority DESC)",
        [],
    )?;

    // Add embeddings columns
    let embedding_exists: Result<i32, rusqlite::Error> = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('projects') WHERE name='embedding'",
        [],
        |row| row.get(0),
    );

    if embedding_exists.unwrap_or(0) == 0 {
        conn.execute("ALTER TABLE projects ADD COLUMN embedding BLOB", [])?;
        log::info!("Added embedding column to projects table");
    }

    let act_embedding_exists: Result<i32, rusqlite::Error> = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('activities') WHERE name='embedding'",
        [],
        |row| row.get(0),
    );

    if act_embedding_exists.unwrap_or(0) == 0 {
        conn.execute("ALTER TABLE activities ADD COLUMN embedding BLOB", [])?;
        log::info!("Added embedding column to activities table");
    }

    // Issue candidates table - caches PM system issues with embeddings for AI matching
    conn.execute(
        "CREATE TABLE IF NOT EXISTS issue_candidates (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            external_id TEXT NOT NULL,
            external_system TEXT NOT NULL,
            pm_project_id TEXT,
            title TEXT NOT NULL,
            description TEXT,
            status TEXT NOT NULL,
            labels TEXT NOT NULL DEFAULT '[]',
            assignee TEXT,
            embedding BLOB,
            last_synced TEXT NOT NULL,
            UNIQUE(external_id, external_system),
            FOREIGN KEY (project_id) REFERENCES projects(id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_issue_candidates_project ON issue_candidates(project_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_issue_candidates_status ON issue_candidates(status)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_issue_candidates_external ON issue_candidates(external_id, external_system)",
        [],
    )?;

    log::info!("Database schema initialized");
    Ok(())
}

/// Insert default categories for common development tools
///
/// # Errors
///
/// Returns an error if database insert operations fail
pub fn insert_default_categories(conn: &Connection) -> Result<()> {
    let default_categories = vec![
        (
            "Coding",
            // Cursor uses com.todesktop.* bundle ID
            // Google Antigravity, Windsurf, and other AI IDEs
            "(?i)(vscode|code|cursor|todesktop|intellij|pycharm|webstorm|sublime|vim|nvim|neovim|emacs|xcode|android.studio|zed|antigravity|windsurf|replit)",
            Some("Code editors and IDEs"),
        ),
        (
            "AI-CLI",
            // AI CLI tools: Claude CLI, Gemini CLI, OpenAI CLI, etc.
            "(?i)(claude|gemini|openai|anthropic|copilot|aider|continue)",
            Some("AI coding assistants (CLI)"),
        ),
        (
            "Terminal",
            "(?i)(terminal|iterm|konsole|gnome-terminal|wezterm|alacritty|kitty|hyper|warp)",
            Some("Terminal and shell"),
        ),
        (
            "Break",
            // Social media and entertainment - detected by window title
            "(?i)(instagram|facebook|twitter|tiktok|youtube|netflix|twitch|reddit|linkedin\\.com/feed|threads|snapchat|pinterest|tumblr|weibo|bilibili)",
            Some("Personal browsing and breaks"),
        ),
        (
            "Research",
            // Work-related browsing - detected by window title
            "(?i)(stackoverflow|github\\.com|gitlab\\.com|docs\\.|documentation|api\\s+reference|mdn\\s+web|devdocs|plane\\.so|jira|linear\\.app|notion\\.so)",
            Some("Work-related research and documentation"),
        ),
        (
            "Browser",
            "(?i)(chrome|firefox|safari|edge|brave|arc|opera|vivaldi)",
            Some("Web browsers (general)"),
        ),
        (
            "Communication",
            "(?i)(slack|discord|teams|zoom|skype|telegram|whatsapp|messages|mail)",
            Some("Communication tools"),
        ),
        (
            "Documentation",
            "(?i)(notion|obsidian|evernote|onenote|bear|typora|logseq|roam)",
            Some("Documentation and notes"),
        ),
        (
            "Design",
            "(?i)(figma|sketch|adobe|photoshop|illustrator|canva|affinity)",
            Some("Design tools"),
        ),
        (
            "Database",
            "(?i)(dbeaver|tableplus|sequel|datagrip|mongodb|postico|pgadmin)",
            Some("Database clients"),
        ),
        (
            "Git",
            "(?i)(github|gitlab|sourcetree|gitkraken|fork|tower)",
            Some("Git clients and services"),
        ),
    ];

    for (name, pattern, description) in default_categories {
        // Check if category already exists
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM categories WHERE name = ?1)",
            [name],
            |row| row.get(0),
        )?;

        if !exists {
            conn.execute(
                "INSERT INTO categories (id, name, pattern, description) VALUES (?1, ?2, ?3, ?4)",
                [
                    &uuid::Uuid::new_v4().to_string(),
                    name,
                    pattern,
                    description.unwrap_or(""),
                ],
            )?;
        }
    }

    log::info!("Default categories inserted");
    Ok(())
}
