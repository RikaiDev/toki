//! Claude Code session management commands
//!
//! Provides CLI commands for tracking Claude Code AI-assisted development sessions.
//! These are primarily called by Claude Code hooks but can also be used manually.

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use clap::Subcommand;
use toki_storage::Database;

#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// Start a new Claude Code session (called by SessionStart hook)
    Start {
        /// Claude Code session ID
        #[arg(long)]
        id: String,
        /// Project path (defaults to current directory)
        #[arg(long)]
        project: Option<String>,
    },
    /// End a Claude Code session (called by SessionEnd hook)
    End {
        /// Claude Code session ID
        #[arg(long)]
        id: String,
        /// Reason for ending (clear, logout, prompt_input_exit, other)
        #[arg(long)]
        reason: Option<String>,
    },
    /// List Claude Code sessions
    List {
        /// Show only today's sessions
        #[arg(long)]
        today: bool,
        /// Show sessions from the last N days
        #[arg(long, default_value = "7")]
        days: u32,
    },
    /// Show details of a specific session
    Show {
        /// Claude Code session ID
        id: String,
    },
    /// Show active (unclosed) sessions
    Active,
}

/// Handle session commands
pub async fn handle_session_command(action: SessionAction) -> Result<()> {
    match action {
        SessionAction::Start { id, project } => start_session(&id, project.as_deref()),
        SessionAction::End { id, reason } => end_session(&id, reason.as_deref()),
        SessionAction::List { today, days } => list_sessions(today, days),
        SessionAction::Show { id } => show_session(&id),
        SessionAction::Active => list_active_sessions(),
    }
}

/// Start a new Claude Code session
fn start_session(session_id: &str, project_path: Option<&str>) -> Result<()> {
    let db = Database::new(None).context("Failed to open database")?;

    // Try to find project by path
    let project_id = if let Some(path) = project_path {
        db.get_project_by_path(path)?.map(|p| p.id)
    } else {
        // Try current directory
        let cwd = std::env::current_dir().unwrap_or_default();
        db.get_project_by_path(cwd.to_string_lossy().as_ref())?
            .map(|p| p.id)
    };

    // Check if session already exists
    if let Some(existing) = db.get_claude_session(session_id)? {
        if existing.is_active() {
            // Session already active, just return success
            println!("Session {} already active", session_id);
            return Ok(());
        }
    }

    let session = db.start_claude_session(session_id, project_id)?;

    println!("Started session: {}", session.session_id);
    if let Some(pid) = project_id {
        if let Some(project) = db.get_project(pid)? {
            println!("Project: {}", project.name);
        }
    }

    Ok(())
}

/// End a Claude Code session
fn end_session(session_id: &str, reason: Option<&str>) -> Result<()> {
    let db = Database::new(None).context("Failed to open database")?;

    // Check if session exists
    let session = db.get_claude_session(session_id)?;
    if session.is_none() {
        println!("Session {} not found", session_id);
        return Ok(());
    }

    let session = session.unwrap();
    if !session.is_active() {
        println!("Session {} already ended", session_id);
        return Ok(());
    }

    db.end_claude_session(session_id, reason)?;

    let duration = session.duration_seconds();
    println!("Ended session: {}", session_id);
    println!("Duration: {}", format_duration(duration));
    if let Some(r) = reason {
        println!("Reason: {}", r);
    }

    Ok(())
}

/// List Claude Code sessions
fn list_sessions(today: bool, days: u32) -> Result<()> {
    let db = Database::new(None).context("Failed to open database")?;

    let sessions = if today {
        db.get_claude_sessions_today()?
    } else {
        let end = Utc::now();
        let start = end - Duration::days(i64::from(days));
        db.get_claude_sessions(start, end)?
    };

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    println!(
        "Claude Code sessions ({}):\n",
        if today {
            "today".to_string()
        } else {
            format!("last {} days", days)
        }
    );

    let mut total_duration = 0u32;

    for session in &sessions {
        let duration = session.duration_seconds();
        total_duration += duration;

        let status = if session.is_active() { "[ACTIVE]" } else { "" };
        let project_name = session
            .project_id
            .and_then(|pid| db.get_project(pid).ok().flatten())
            .map(|p| p.name)
            .unwrap_or_else(|| "-".to_string());

        println!(
            "  {} {} {}",
            session.started_at.format("%Y-%m-%d %H:%M"),
            format_duration(duration),
            status
        );
        println!(
            "    ID: {} | Project: {} | Tools: {} | Prompts: {}",
            truncate_id(&session.session_id),
            project_name,
            session.tool_calls,
            session.prompt_count
        );
        println!();
    }

    println!("Total: {} sessions, {}", sessions.len(), format_duration(total_duration));

    Ok(())
}

/// Show details of a specific session
fn show_session(session_id: &str) -> Result<()> {
    let db = Database::new(None).context("Failed to open database")?;

    let session = db
        .get_claude_session(session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

    let project_name = session
        .project_id
        .and_then(|pid| db.get_project(pid).ok().flatten())
        .map(|p| p.name)
        .unwrap_or_else(|| "-".to_string());

    println!("Session Details\n");
    println!("ID:         {}", session.session_id);
    println!("Status:     {}", if session.is_active() { "Active" } else { "Ended" });
    println!("Project:    {}", project_name);
    println!("Started:    {}", session.started_at.format("%Y-%m-%d %H:%M:%S"));
    if let Some(ended) = session.ended_at {
        println!("Ended:      {}", ended.format("%Y-%m-%d %H:%M:%S"));
    }
    println!("Duration:   {}", format_duration(session.duration_seconds()));
    if let Some(reason) = &session.end_reason {
        println!("End Reason: {}", reason);
    }
    println!("Tool Calls: {}", session.tool_calls);
    println!("Prompts:    {}", session.prompt_count);

    Ok(())
}

/// List active (unclosed) sessions
fn list_active_sessions() -> Result<()> {
    let db = Database::new(None).context("Failed to open database")?;

    let sessions = db.get_active_claude_sessions()?;

    if sessions.is_empty() {
        println!("No active sessions.");
        return Ok(());
    }

    println!("Active Claude Code sessions:\n");

    for session in &sessions {
        let duration = session.duration_seconds();
        let project_name = session
            .project_id
            .and_then(|pid| db.get_project(pid).ok().flatten())
            .map(|p| p.name)
            .unwrap_or_else(|| "-".to_string());

        println!(
            "  {} (running for {})",
            truncate_id(&session.session_id),
            format_duration(duration)
        );
        println!(
            "    Started: {} | Project: {} | Tools: {}",
            session.started_at.format("%H:%M:%S"),
            project_name,
            session.tool_calls
        );
        println!();
    }

    Ok(())
}

/// Format duration in human-readable form
fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

/// Truncate session ID for display
fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..12])
    } else {
        id.to_string()
    }
}
