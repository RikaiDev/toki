//! Issue complexity and time estimation command
//!
//! AI-assisted estimation for issues based on:
//! - Complexity analysis (heuristics)
//! - Historical data from similar issues
//! - Embedding similarity

use std::sync::Arc;

use anyhow::{Context, Result};
use toki_ai::AiService;
use toki_ai::time_estimator::{TimeBreakdown, TimeEstimate, TimeEstimator};
use toki_storage::models::Complexity;
use toki_storage::Database;

/// Estimate complexity and time for an issue
pub async fn handle_estimate_command(
    issue_id: &str,
    set_complexity: Option<&str>,
    system: &str,
) -> Result<()> {
    let db = Arc::new(Database::new(None).context("Failed to open database")?);

    // Find the issue
    let issue = db
        .get_issue_candidate(issue_id, system)?
        .or_else(|| db.get_issue_candidate_by_external_id(issue_id).ok().flatten())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Issue {issue_id} not found. Try running 'toki issue-sync' first."
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

        let reason = "Manually set via CLI".to_string();
        db.update_issue_complexity(&issue.external_id, &issue.external_system, complexity, &reason)?;

        println!("Set complexity: {complexity}");
        println!("Reason: {reason}");
        return Ok(());
    }

    // Show complexity (existing or estimated)
    let complexity = if let Some(c) = issue.complexity {
        println!("Complexity: {c}");
        if let Some(reason) = &issue.complexity_reason {
            println!("Reason: {reason}");
        }
        c
    } else {
        let (estimated, reason) = estimate_complexity(&issue.title, issue.description.as_deref(), &issue.labels);
        println!("Suggested complexity: {estimated}");
        println!("Reason: {reason}");
        estimated
    };

    println!();

    // Time estimation
    println!("Time Estimation");
    println!("{}", "\u{2500}".repeat(40));

    // Initialize AI service
    let ai_service = match db.get_ai_config() {
        Ok(config) => {
            if config.enabled {
                match AiService::new(config) {
                    Ok(service) => Some(service),
                    Err(e) => {
                        log::warn!("Failed to initialize AI service: {e}");
                        None
                    }
                }
            } else {
                None
            }
        }
        Err(_) => None,
    };

    let estimator = TimeEstimator::new(db.clone(), ai_service);
    let time_estimate = estimator.estimate(&issue).await?;

    print_time_estimate(&time_estimate, &complexity);

    // Show complexity scale reference
    println!();
    println!("Complexity Scale:");
    println!("  trivial (1) - Single-line fix, typo (~5 min)");
    println!("  simple  (2) - Single file, clear implementation (~30 min)");
    println!("  moderate(3) - Multiple files, some design (~2 hours)");
    println!("  complex (5) - Architectural changes (~6 hours)");
    println!("  epic    (8) - Major feature, significant refactoring (~20 hours)");

    if issue.complexity.is_none() {
        println!();
        println!("To set complexity: toki estimate {} --set {}", issue.external_id, complexity.label());
    }

    Ok(())
}

/// Print time estimate details
fn print_time_estimate(estimate: &TimeEstimate, _complexity: &Complexity) {
    println!();
    println!("Estimated time: {}", estimate.formatted());
    println!("Range: {} (80% confidence)", estimate.formatted_range());
    println!("Method: {}", estimate.method);
    println!("Confidence: {:.0}%", estimate.confidence * 100.0);

    // Show similar issues if any
    if !estimate.similar_issues.is_empty() {
        println!();
        println!("Based on similar issues:");
        for similar in &estimate.similar_issues {
            let complexity_str = similar
                .complexity
                .map(|c| format!(" [{}]", c.label()))
                .unwrap_or_default();
            println!(
                "  - {} ({}) - {:.0}% similar{}",
                similar.issue_id,
                TimeEstimate::format_duration(similar.actual_seconds),
                similar.similarity * 100.0,
                complexity_str
            );
        }
    }

    // Show breakdown
    if let Some(breakdown) = &estimate.breakdown {
        println!();
        println!("Suggested breakdown:");
        print_breakdown(breakdown);
    }
}

/// Print time breakdown
fn print_breakdown(breakdown: &TimeBreakdown) {
    println!(
        "  Implementation: {}",
        TimeEstimate::format_duration(breakdown.implementation_seconds)
    );
    println!(
        "  Testing:        {}",
        TimeEstimate::format_duration(breakdown.testing_seconds)
    );
    println!(
        "  Documentation:  {}",
        TimeEstimate::format_duration(breakdown.documentation_seconds)
    );
}

/// Estimate complexity based on issue metadata using heuristics
fn estimate_complexity(title: &str, description: Option<&str>, labels: &[String]) -> (Complexity, String) {
    let title_lower = title.to_lowercase();
    let desc_lower = description.map(str::to_lowercase).unwrap_or_default();
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
    let desc_len = description.map_or(0, str::len);
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
