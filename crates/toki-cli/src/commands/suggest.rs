//! Issue suggestion command
//!
//! AI-powered issue suggestion based on current git context.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use toki_ai::issue_matcher::{ActivitySignals, SmartIssueMatcher};
use toki_detector::git::GitDetector;
use toki_storage::Database;

/// Suggest issues based on current git context
pub fn run(
    path: Option<PathBuf>,
    max_suggestions: usize,
    apply: bool,
) -> Result<()> {
    let working_dir = path.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Detect git repository
    let git_detector = GitDetector::new();
    let repo_path = git_detector
        .find_repo(&working_dir)?
        .ok_or_else(|| anyhow::anyhow!("No git repository found in {}", working_dir.display()))?;

    println!("Analyzing git context in {}...\n", repo_path.display());

    // Collect activity signals from git
    let signals = collect_git_signals(&git_detector, &repo_path)?;

    // Display collected signals
    println!("Context signals:");
    if let Some(ref branch) = signals.git_branch {
        println!("  Branch: {branch}");
    }
    if !signals.recent_commits.is_empty() {
        println!("  Recent commits:");
        for commit in &signals.recent_commits {
            println!("    - {commit}");
        }
    }
    if !signals.edited_files.is_empty() {
        println!("  Changed files: {} files", signals.edited_files.len());
    }
    println!();

    // Create database Arc for sharing
    let db = Arc::new(Database::new(None)?);

    // Find project for this path
    let project = db
        .get_project_by_path(repo_path.to_string_lossy().as_ref())?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No project found for {}. Run 'toki init' or work in the directory first.",
                repo_path.display()
            )
        })?;

    // Create matcher and find suggestions
    let matcher = SmartIssueMatcher::new(db.clone())
        .context("Failed to initialize issue matcher")?;

    let suggestions = matcher.find_best_matches(&signals, project.id, max_suggestions)?;

    if suggestions.is_empty() {
        println!("No matching issues found.");
        println!("\nPossible reasons:");
        println!("  - No issues synced for this project (run 'toki issue-sync')");
        println!("  - Branch/commits don't match any issue patterns");
        println!("  - Try adding issue ID to branch name (e.g., feature/PROJ-123-description)");
        return Ok(());
    }

    println!("Suggested issues:\n");
    for (i, suggestion) in suggestions.iter().enumerate() {
        let confidence_bar = get_confidence_bar(suggestion.confidence);
        let reasons = SmartIssueMatcher::format_reasons(&suggestion.match_reasons);

        // Get issue details from database
        let issue_title = db
            .get_issue_candidate_by_external_id(&suggestion.issue_id)?
            .map(|c| c.title)
            .unwrap_or_else(|| "(title not found)".to_string());

        println!(
            "  {}. {} - {} [{confidence_bar}] {:.0}%",
            i + 1,
            suggestion.issue_id,
            issue_title,
            suggestion.confidence * 100.0
        );
        println!("     Matched by: {reasons}");
        println!();
    }

    if apply && !suggestions.is_empty() {
        let best = &suggestions[0];
        println!("Applying best match: {}", best.issue_id);
        // TODO: Link current work to this issue
        println!("(Auto-linking not yet implemented)");
    } else if !suggestions.is_empty() {
        println!("Use --apply to automatically link to the best match.");
    }

    Ok(())
}

/// Collect activity signals from git repository
fn collect_git_signals(detector: &GitDetector, repo_path: &std::path::Path) -> Result<ActivitySignals> {
    let branch = detector.get_branch_name(repo_path)?;
    let commits = detector.get_recent_commits(repo_path, 5)?;
    let files = detector.get_changed_files(repo_path)?;

    Ok(ActivitySignals {
        git_branch: branch,
        recent_commits: commits,
        edited_files: files,
        browser_urls: Vec::new(),
        window_titles: Vec::new(),
    })
}

/// Generate a visual confidence bar
fn get_confidence_bar(confidence: f32) -> String {
    let filled = (confidence * 10.0).round() as usize;
    let empty = 10 - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}
