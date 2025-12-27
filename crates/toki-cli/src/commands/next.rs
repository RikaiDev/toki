//! Smart next task suggestion command
//!
//! AI-powered task suggestion based on:
//! - Open issues across projects
//! - Recent work context (session history)
//! - Time/energy constraints
//! - Issue complexity and priority

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use toki_ai::time_estimator::TimeEstimator;
use toki_storage::models::Complexity;
use toki_storage::{Database, IssueCandidate};

/// Focus level for task selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusLevel {
    /// Deep focus - complex, uninterrupted work
    Deep,
    /// Normal focus - standard tasks
    Normal,
    /// Low focus - simple tasks, post-meeting recovery
    Low,
}

impl FocusLevel {
    fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "deep" | "high" => Some(Self::Deep),
            "normal" | "medium" => Some(Self::Normal),
            "low" | "light" => Some(Self::Low),
            _ => None,
        }
    }

    fn max_complexity(self) -> Complexity {
        match self {
            Self::Deep => Complexity::Epic,
            Self::Normal => Complexity::Complex,
            Self::Low => Complexity::Simple,
        }
    }
}

/// Parse time string (e.g., "30m", "2h", "1h30m") to seconds
fn parse_time_to_seconds(s: &str) -> Option<u32> {
    let s = s.to_lowercase();
    let mut total_seconds = 0u32;
    let mut current_num = String::new();

    for c in s.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else if c == 'h' {
            if let Ok(hours) = current_num.parse::<u32>() {
                total_seconds += hours * 3600;
            }
            current_num.clear();
        } else if c == 'm' {
            if let Ok(minutes) = current_num.parse::<u32>() {
                total_seconds += minutes * 60;
            }
            current_num.clear();
        }
    }

    if total_seconds > 0 {
        Some(total_seconds)
    } else {
        None
    }
}

/// Task suggestion with reasoning
#[derive(Debug)]
struct TaskSuggestion {
    issue: IssueCandidate,
    score: f32,
    reasons: Vec<String>,
    estimated_seconds: u32,
}

/// Handle the next task suggestion command
pub fn handle_next_command(
    time: Option<&str>,
    focus: Option<&str>,
    count: usize,
) -> Result<()> {
    let db = Arc::new(Database::new(None).context("Failed to open database")?);

    // Parse constraints
    let max_time_seconds = time.and_then(parse_time_to_seconds);
    let focus_level = focus.and_then(FocusLevel::parse).unwrap_or(FocusLevel::Normal);

    // Get all projects
    let projects = db.get_all_projects()?;

    if projects.is_empty() {
        println!("No projects found. Run 'toki project auto-link' first.");
        return Ok(());
    }

    // Collect all active issues across projects
    let mut all_issues: Vec<IssueCandidate> = Vec::new();
    for project in &projects {
        if let Ok(issues) = db.get_active_issue_candidates(project.id) {
            all_issues.extend(issues);
        }
    }

    if all_issues.is_empty() {
        println!("No open issues found. Run 'toki issue-sync' to sync issues from your PM system.");
        return Ok(());
    }

    // Get recent sessions for context
    let recent_sessions = db.get_claude_sessions(
        Utc::now() - Duration::days(7),
        Utc::now(),
    )?;

    // Get recently worked issues for context continuity
    let mut recent_issue_ids: Vec<String> = Vec::new();
    for session in &recent_sessions {
        if let Ok(session_issues) = db.get_session_issues(session.id) {
            for si in session_issues {
                if !recent_issue_ids.contains(&si.issue_id) {
                    recent_issue_ids.push(si.issue_id);
                }
            }
        }
    }

    // Create time estimator
    let estimator = TimeEstimator::new(db.clone());

    // Score and rank issues
    let mut suggestions: Vec<TaskSuggestion> = Vec::new();

    for issue in all_issues {
        let mut score = 50.0f32; // Base score
        let mut reasons = Vec::new();

        // Estimate time
        let time_estimate = estimator.estimate(&issue).ok();
        let estimated_seconds = time_estimate
            .as_ref()
            .map(|e| e.estimated_seconds)
            .unwrap_or(7200); // Default 2 hours

        // Time constraint filter
        if let Some(max_seconds) = max_time_seconds {
            if estimated_seconds > max_seconds {
                continue; // Skip tasks that take too long
            }
            // Bonus for tasks that fit well within time budget
            let fit_ratio = estimated_seconds as f32 / max_seconds as f32;
            if fit_ratio > 0.5 && fit_ratio <= 1.0 {
                score += 15.0;
                reasons.push("fits available time well".to_string());
            }
        }

        // Focus level constraint
        let complexity = issue.complexity.unwrap_or(Complexity::Moderate);
        let max_complexity = focus_level.max_complexity();
        if complexity.points() > max_complexity.points() {
            continue; // Skip tasks too complex for current focus level
        }

        // Complexity scoring based on focus
        match focus_level {
            FocusLevel::Deep => {
                // Prefer complex tasks for deep focus
                if complexity.points() >= 5 {
                    score += 20.0;
                    reasons.push("good for deep focus".to_string());
                }
            }
            FocusLevel::Low => {
                // Prefer simple tasks for low focus
                if complexity.points() <= 2 {
                    score += 20.0;
                    reasons.push("suitable for low-energy work".to_string());
                }
            }
            FocusLevel::Normal => {
                // Balanced preference
                if complexity.points() >= 2 && complexity.points() <= 5 {
                    score += 10.0;
                }
            }
        }

        // Context continuity - boost related issues
        if recent_issue_ids.contains(&issue.external_id) {
            score += 25.0;
            reasons.push("continues recent work".to_string());
        }

        // Check for similar issues in recent work using embeddings
        if let Some(ref issue_embedding) = issue.embedding {
            for recent_id in &recent_issue_ids {
                if let Ok(Some(recent_issue)) = db.get_issue_candidate_by_external_id(recent_id) {
                    if let Some(ref recent_embedding) = recent_issue.embedding {
                        let similarity = cosine_similarity(issue_embedding, recent_embedding);
                        if similarity > 0.7 {
                            score += 15.0 * similarity;
                            if !reasons.iter().any(|r| r.contains("related to")) {
                                reasons.push(format!("related to recent #{}", recent_id));
                            }
                        }
                    }
                }
            }
        }

        // Priority from labels
        let labels_lower: Vec<String> = issue.labels.iter().map(|l| l.to_lowercase()).collect();
        if labels_lower.iter().any(|l| l.contains("urgent") || l.contains("critical")) {
            score += 30.0;
            reasons.push("marked as urgent/critical".to_string());
        }
        if labels_lower.iter().any(|l| l.contains("high") && l.contains("priority")) {
            score += 20.0;
            reasons.push("high priority".to_string());
        }
        if labels_lower.iter().any(|l| l.contains("blocked") || l.contains("waiting")) {
            score -= 50.0; // Deprioritize blocked tasks
        }

        // Status scoring
        if issue.status.to_lowercase().contains("in_progress") || issue.status.to_lowercase().contains("doing") {
            score += 15.0;
            reasons.push("already in progress".to_string());
        }

        suggestions.push(TaskSuggestion {
            issue,
            score,
            reasons,
            estimated_seconds,
        });
    }

    // Sort by score (highest first)
    suggestions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

    // Take top N
    suggestions.truncate(count);

    if suggestions.is_empty() {
        println!("No tasks match your constraints.");
        if max_time_seconds.is_some() {
            println!("Try increasing the --time limit or removing constraints.");
        }
        return Ok(());
    }

    // Display suggestions
    println!("Suggested next tasks:\n");

    for (i, suggestion) in suggestions.iter().enumerate() {
        let prefix = if i == 0 { ">" } else { " " };
        let complexity_str = suggestion.issue.complexity
            .map(|c| format!(" [{}]", c.label()))
            .unwrap_or_default();

        println!(
            "{} {}. #{} - {}{}",
            prefix,
            i + 1,
            suggestion.issue.external_id,
            suggestion.issue.title,
            complexity_str
        );
        println!(
            "      Est: {} | System: {}",
            format_duration(suggestion.estimated_seconds),
            suggestion.issue.external_system
        );

        if !suggestion.reasons.is_empty() {
            println!("      Why: {}", suggestion.reasons.join(", "));
        }
        println!();
    }

    // Show constraints if any
    if max_time_seconds.is_some() || focus.is_some() {
        println!("Constraints:");
        if let Some(max) = max_time_seconds {
            println!("  Time: <= {}", format_duration(max));
        }
        println!("  Focus: {:?}", focus_level);
    }

    Ok(())
}

/// Format duration in human-readable form
fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;

    if hours > 0 {
        if minutes > 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        "< 1m".to_string()
    }
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}
