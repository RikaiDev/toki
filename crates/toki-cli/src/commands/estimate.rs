//! Issue complexity estimation command
//!
//! AI-assisted complexity estimation for issues based on codebase context.

use anyhow::{Context, Result};
use toki_storage::models::Complexity;
use toki_storage::Database;

/// Estimate complexity for an issue
pub fn handle_estimate_command(
    issue_id: &str,
    set_complexity: Option<&str>,
    system: &str,
) -> Result<()> {
    let db = Database::new(None).context("Failed to open database")?;

    // Find the issue
    let issue = db
        .get_issue_candidate(issue_id, system)?
        .or_else(|| db.get_issue_candidate_by_external_id(issue_id).ok().flatten())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Issue {} not found. Try running 'toki issue-sync' first.",
                issue_id
            )
        })?;

    println!("Issue #{}: {}", issue.external_id, issue.title);
    println!("System: {}", issue.external_system);
    println!("Status: {}", issue.status);

    if let Some(desc) = &issue.description {
        let truncated = if desc.len() > 200 {
            format!("{}...", &desc[..200])
        } else {
            desc.clone()
        };
        println!("Description: {truncated}");
    }

    println!();

    // If setting complexity manually
    if let Some(complexity_str) = set_complexity {
        let complexity: Complexity = complexity_str
            .parse()
            .map_err(|e: String| anyhow::anyhow!(e))?;

        let reason = format!("Manually set via CLI");
        db.update_issue_complexity(&issue.external_id, &issue.external_system, complexity, &reason)?;

        println!("Set complexity: {complexity}");
        println!("Reason: {reason}");
        return Ok(());
    }

    // Show existing complexity if set
    if let Some(complexity) = issue.complexity {
        println!("Current complexity: {complexity}");
        if let Some(reason) = &issue.complexity_reason {
            println!("Reason: {reason}");
        }
        println!();
        println!("Use --set <level> to update the complexity.");
        return Ok(());
    }

    // Estimate complexity using heuristics
    let (estimated, reason) = estimate_complexity(&issue.title, issue.description.as_deref(), &issue.labels);

    println!("Suggested complexity: {estimated}");
    println!("Reason: {reason}");
    println!();
    println!("Complexity Scale:");
    println!("  trivial (1) - Single-line fix, typo, obvious change");
    println!("  simple  (2) - Single file, clear implementation");
    println!("  moderate(3) - Multiple files, some design decisions");
    println!("  complex (5) - Architectural changes, multiple components");
    println!("  epic    (8) - Major feature, significant refactoring");
    println!();
    println!("To set this complexity, run:");
    println!("  toki estimate {} --set {}", issue.external_id, estimated.label());

    Ok(())
}

/// Estimate complexity based on issue metadata using heuristics
fn estimate_complexity(title: &str, description: Option<&str>, labels: &[String]) -> (Complexity, String) {
    let title_lower = title.to_lowercase();
    let desc_lower = description.map(|d| d.to_lowercase()).unwrap_or_default();
    let labels_lower: Vec<String> = labels.iter().map(|l| l.to_lowercase()).collect();

    // Check for explicit complexity labels first
    for label in &labels_lower {
        if label.contains("trivial") || label.contains("quick-fix") {
            return (Complexity::Trivial, "Label indicates trivial task".to_string());
        }
        if label.contains("simple") || label.contains("good-first-issue") || label.contains("easy") {
            return (Complexity::Simple, "Label indicates simple task".to_string());
        }
        if label.contains("complex") || label.contains("hard") {
            return (Complexity::Complex, "Label indicates complex task".to_string());
        }
        if label.contains("epic") || label.contains("major") {
            return (Complexity::Epic, "Label indicates epic/major feature".to_string());
        }
    }

    // Check title/description for complexity signals
    let mut reasons = Vec::new();
    let mut score = 0i32;

    // Trivial indicators
    if title_lower.contains("typo")
        || title_lower.contains("fix typo")
        || title_lower.contains("spelling")
        || title_lower.contains("rename")
    {
        score -= 2;
        reasons.push("likely a small text fix");
    }

    // Simple indicators
    if title_lower.contains("add comment")
        || title_lower.contains("update doc")
        || title_lower.contains("fix bug")
    {
        score -= 1;
        reasons.push("straightforward change");
    }

    // Moderate indicators
    if title_lower.contains("refactor")
        || title_lower.contains("improve")
        || title_lower.contains("enhance")
    {
        score += 1;
        reasons.push("involves refactoring");
    }

    if desc_lower.contains("multiple files") || desc_lower.contains("several") {
        score += 1;
        reasons.push("affects multiple files");
    }

    // Complex indicators
    if title_lower.contains("implement")
        || title_lower.contains("feature:")
        || title_lower.contains("feat:")
    {
        score += 2;
        reasons.push("new feature implementation");
    }

    if desc_lower.contains("database") || desc_lower.contains("migration") {
        score += 1;
        reasons.push("involves database changes");
    }

    if desc_lower.contains("api") || desc_lower.contains("endpoint") {
        score += 1;
        reasons.push("API changes");
    }

    // Epic indicators
    if title_lower.contains("rethink")
        || title_lower.contains("redesign")
        || title_lower.contains("major")
        || title_lower.contains("rewrite")
    {
        score += 3;
        reasons.push("major architectural work");
    }

    if desc_lower.contains("breaking change") || desc_lower.contains("backward") {
        score += 2;
        reasons.push("breaking changes involved");
    }

    // Length-based heuristics
    let desc_len = description.map(|d| d.len()).unwrap_or(0);
    if desc_len > 1000 {
        score += 1;
        reasons.push("detailed requirements suggest complexity");
    }

    // Map score to complexity
    let complexity = match score {
        i32::MIN..=-1 => Complexity::Trivial,
        0..=1 => Complexity::Simple,
        2..=3 => Complexity::Moderate,
        4..=5 => Complexity::Complex,
        _ => Complexity::Epic,
    };

    let reason = if reasons.is_empty() {
        "Based on general analysis of title and description".to_string()
    } else {
        format!("Based on analysis: {}", reasons.join(", "))
    };

    (complexity, reason)
}
