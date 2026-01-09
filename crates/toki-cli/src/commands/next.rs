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
use toki_ai::AiService;
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

/// Scoring context for issue evaluation
struct ScoringContext<'a> {
    max_time_seconds: Option<u32>,
    focus_level: FocusLevel,
    recent_issue_ids: &'a [String],
    db: &'a Database,
}

/// Try to create AI service from database config
fn create_ai_service(db: &Database) -> Option<AiService> {
    let config = db.get_ai_config().ok()?;
    if !config.enabled {
        return None;
    }
    AiService::new(config)
        .map_err(|e| log::warn!("Failed to initialize AI service: {e}"))
        .ok()
}

/// Collect recent issue IDs from sessions
fn collect_recent_issue_ids(db: &Database) -> Vec<String> {
    let sessions = db
        .get_claude_sessions(Utc::now() - Duration::days(7), Utc::now())
        .unwrap_or_default();

    let mut recent_ids = Vec::new();
    for session in &sessions {
        if let Ok(session_issues) = db.get_session_issues(session.id) {
            for si in session_issues {
                if !recent_ids.contains(&si.issue_id) {
                    recent_ids.push(si.issue_id);
                }
            }
        }
    }
    recent_ids
}

/// Check if issue passes time constraint, returns bonus score if it fits well
fn check_time_constraint(estimated_seconds: u32, max_seconds: Option<u32>) -> Option<(bool, f32, Option<String>)> {
    let max = max_seconds?;
    if estimated_seconds > max {
        return Some((false, 0.0, None)); // Filter out
    }
    let fit_ratio = estimated_seconds as f32 / max as f32;
    if fit_ratio > 0.5 && fit_ratio <= 1.0 {
        Some((true, 15.0, Some("fits available time well".to_string())))
    } else {
        Some((true, 0.0, None))
    }
}

/// Check if issue passes focus level constraint and calculate focus-based score
fn check_focus_constraint(complexity: Complexity, focus_level: FocusLevel) -> Option<(f32, Option<String>)> {
    let max_complexity = focus_level.max_complexity();
    if complexity.points() > max_complexity.points() {
        return None; // Filter out
    }

    match focus_level {
        FocusLevel::Deep if complexity.points() >= 5 => {
            Some((20.0, Some("good for deep focus".to_string())))
        }
        FocusLevel::Low if complexity.points() <= 2 => {
            Some((20.0, Some("suitable for low-energy work".to_string())))
        }
        FocusLevel::Normal if complexity.points() >= 2 && complexity.points() <= 5 => {
            Some((10.0, None))
        }
        _ => Some((0.0, None)),
    }
}

/// Calculate embedding similarity bonus
fn calculate_embedding_bonus(
    issue: &IssueCandidate,
    recent_issue_ids: &[String],
    db: &Database,
) -> (f32, Option<String>) {
    let issue_embedding = match &issue.embedding {
        Some(e) => e,
        None => return (0.0, None),
    };

    let mut bonus = 0.0f32;
    let mut reason = None;

    for recent_id in recent_issue_ids {
        let recent_issue = match db.get_issue_candidate_by_external_id(recent_id) {
            Ok(Some(i)) => i,
            _ => continue,
        };
        let recent_embedding = match &recent_issue.embedding {
            Some(e) => e,
            None => continue,
        };

        let similarity = cosine_similarity(issue_embedding, recent_embedding);
        if similarity > 0.7 {
            bonus += 15.0 * similarity;
            if reason.is_none() {
                reason = Some(format!("related to recent #{recent_id}"));
            }
        }
    }

    (bonus, reason)
}

/// Calculate label-based score adjustments
fn calculate_label_score(labels: &[String]) -> (f32, Vec<String>) {
    let labels_lower: Vec<String> = labels.iter().map(|l| l.to_lowercase()).collect();
    let mut score = 0.0f32;
    let mut reasons = Vec::new();

    if labels_lower.iter().any(|l| l.contains("urgent") || l.contains("critical")) {
        score += 30.0;
        reasons.push("marked as urgent/critical".to_string());
    }
    if labels_lower.iter().any(|l| l.contains("high") && l.contains("priority")) {
        score += 20.0;
        reasons.push("high priority".to_string());
    }
    if labels_lower.iter().any(|l| l.contains("blocked") || l.contains("waiting")) {
        score -= 50.0;
    }

    (score, reasons)
}

/// Score a single issue based on all criteria
fn score_issue(
    issue: &IssueCandidate,
    estimated_seconds: u32,
    ctx: &ScoringContext<'_>,
) -> Option<(f32, Vec<String>)> {
    let mut score = 50.0f32;
    let mut reasons = Vec::new();

    // Time constraint
    if let Some((passes, bonus, reason)) = check_time_constraint(estimated_seconds, ctx.max_time_seconds) {
        if !passes {
            return None;
        }
        score += bonus;
        reasons.extend(reason);
    }

    // Focus level constraint
    let complexity = issue.complexity.unwrap_or(Complexity::Moderate);
    let (focus_bonus, focus_reason) = check_focus_constraint(complexity, ctx.focus_level)?;
    score += focus_bonus;
    reasons.extend(focus_reason);

    // Context continuity
    if ctx.recent_issue_ids.contains(&issue.external_id) {
        score += 25.0;
        reasons.push("continues recent work".to_string());
    }

    // Embedding similarity
    let (embed_bonus, embed_reason) = calculate_embedding_bonus(issue, ctx.recent_issue_ids, ctx.db);
    score += embed_bonus;
    reasons.extend(embed_reason);

    // Label scoring
    let (label_score, label_reasons) = calculate_label_score(&issue.labels);
    score += label_score;
    reasons.extend(label_reasons);

    // Status scoring
    let status_lower = issue.status.to_lowercase();
    if status_lower.contains("in_progress") || status_lower.contains("doing") {
        score += 15.0;
        reasons.push("already in progress".to_string());
    }

    Some((score, reasons))
}

/// Display task suggestions
fn display_suggestions(suggestions: &[TaskSuggestion], max_time_seconds: Option<u32>, focus_level: FocusLevel, has_focus: bool) {
    println!("Suggested next tasks:\n");

    for (i, suggestion) in suggestions.iter().enumerate() {
        let prefix = if i == 0 { ">" } else { " " };
        let complexity_str = suggestion.issue.complexity
            .map(|c| format!(" [{}]", c.label()))
            .unwrap_or_default();

        println!(
            "{prefix} {}. #{} - {}{}",
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

    if max_time_seconds.is_some() || has_focus {
        println!("Constraints:");
        if let Some(max) = max_time_seconds {
            println!("  Time: <= {}", format_duration(max));
        }
        println!("  Focus: {focus_level:?}");
    }
}

/// Handle the next task suggestion command
pub async fn handle_next_command(
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
    let all_issues: Vec<IssueCandidate> = projects
        .iter()
        .filter_map(|p| db.get_active_issue_candidates(p.id).ok())
        .flatten()
        .collect();

    if all_issues.is_empty() {
        println!("No open issues found. Run 'toki issue-sync' to sync issues from your PM system.");
        return Ok(());
    }

    // Get recent issue IDs for context
    let recent_issue_ids = collect_recent_issue_ids(&db);

    // Create time estimator
    let ai_service = create_ai_service(&db);
    let estimator = TimeEstimator::new(db.clone(), ai_service);

    // Create scoring context
    let ctx = ScoringContext {
        max_time_seconds,
        focus_level,
        recent_issue_ids: &recent_issue_ids,
        db: &db,
    };

    // Score and rank issues
    let mut suggestions: Vec<TaskSuggestion> = Vec::new();
    for issue in all_issues {
        let estimated_seconds = estimator
            .estimate(&issue)
            .await
            .ok()
            .map_or(7200, |e| e.estimated_seconds);

        if let Some((score, reasons)) = score_issue(&issue, estimated_seconds, &ctx) {
            suggestions.push(TaskSuggestion {
                issue,
                score,
                reasons,
                estimated_seconds,
            });
        }
    }

    // Sort by score (highest first) and take top N
    suggestions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    suggestions.truncate(count);

    if suggestions.is_empty() {
        println!("No tasks match your constraints.");
        if max_time_seconds.is_some() {
            println!("Try increasing the --time limit or removing constraints.");
        }
        return Ok(());
    }

    display_suggestions(&suggestions, max_time_seconds, focus_level, focus.is_some());
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
