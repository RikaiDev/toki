//! Scope analysis command - detect scope creep by comparing estimated vs actual time

use std::sync::Arc;

use anyhow::Result;
use toki_core::config::get_data_dir;
use toki_storage::{Database, IssueCandidate};

/// Scope status for an issue
#[derive(Debug)]
pub struct ScopeStatus {
    pub issue: IssueCandidate,
    pub estimated_seconds: u32,
    pub actual_seconds: u32,
    pub percentage: f64,
    pub status: ScopeHealthStatus,
}

/// Health status based on estimated vs actual
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScopeHealthStatus {
    /// Under estimate (< 80%)
    OnTrack,
    /// Approaching estimate (80-100%)
    Warning,
    /// Over estimate (100-150%)
    OverEstimate,
    /// Significantly over (> 150%)
    Critical,
}

impl ScopeHealthStatus {
    fn from_percentage(pct: f64) -> Self {
        if pct < 0.8 {
            Self::OnTrack
        } else if pct < 1.0 {
            Self::Warning
        } else if pct < 1.5 {
            Self::OverEstimate
        } else {
            Self::Critical
        }
    }

    fn emoji(self) -> &'static str {
        match self {
            Self::OnTrack => "\u{2705}",
            Self::Warning => "\u{26a0}\u{fe0f}",
            Self::OverEstimate => "\u{1f534}",
            Self::Critical => "\u{1f6a8}",
        }
    }

    fn _label(self) -> &'static str {
        match self {
            Self::OnTrack => "On Track",
            Self::Warning => "Approaching",
            Self::OverEstimate => "Over Estimate",
            Self::Critical => "Critical",
        }
    }
}

/// Handle scope analysis command
///
/// # Errors
///
/// Returns an error if database operations fail
pub fn handle_scope_command(issue_id: Option<&str>, threshold: Option<u32>) -> Result<()> {
    let data_dir = get_data_dir()?;
    let db_path = data_dir.join("toki.db");
    let db = Arc::new(Database::new(Some(db_path))?);

    let threshold_val = threshold.unwrap_or(80);
    let threshold_pct = f64::from(threshold_val) / 100.0;

    // Get issues with estimates
    let issues_with_estimates = db.get_issues_with_estimates(None)?;
    
    // Get time stats for all issues
    let time_stats = db.get_issue_time_stats()?;

    // Build scope statuses
    let mut scope_statuses: Vec<ScopeStatus> = Vec::new();

    for issue in issues_with_estimates {
        let Some(estimated) = issue.estimated_seconds else {
            continue;
        };

        // Find actual time for this issue
        let actual = time_stats
            .iter()
            .find(|ts| ts.issue_id == issue.external_id && ts.issue_system == issue.external_system)
            .map_or(0, |ts| ts.total_seconds);

        let percentage = f64::from(actual) / f64::from(estimated);
        let status = ScopeHealthStatus::from_percentage(percentage);

        // Filter by issue_id if provided
        if let Some(filter_id) = issue_id {
            if !issue.external_id.contains(filter_id) {
                continue;
            }
        }

        scope_statuses.push(ScopeStatus {
            issue,
            estimated_seconds: estimated,
            actual_seconds: actual,
            percentage,
            status,
        });
    }

    if scope_statuses.is_empty() {
        println!("No issues with estimates found.");
        println!();
        println!("To add estimates:");
        println!("  toki estimate ISSUE-123 --store");
        return Ok(());
    }

    // Sort by percentage (worst first)
    scope_statuses.sort_by(|a, b| b.percentage.partial_cmp(&a.percentage).unwrap());

    // Print header
    println!("Scope Analysis");
    println!("\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!();

    // Count by status
    let on_track = scope_statuses.iter().filter(|s| s.status == ScopeHealthStatus::OnTrack).count();
    let warning = scope_statuses.iter().filter(|s| s.status == ScopeHealthStatus::Warning).count();
    let over = scope_statuses.iter().filter(|s| s.status == ScopeHealthStatus::OverEstimate || s.status == ScopeHealthStatus::Critical).count();

    // Show issues exceeding threshold
    let exceeding: Vec<_> = scope_statuses
        .iter()
        .filter(|s| s.percentage >= threshold_pct)
        .collect();

    if !exceeding.is_empty() {
        println!("Issues Exceeding {threshold_val}% of Estimate:");
        println!();

        for s in exceeding {
            println!(
                "  {} {}: \"{}\"",
                s.status.emoji(),
                s.issue.external_id,
                truncate_title(&s.issue.title, 40)
            );
            println!(
                "    Estimated: {} | Actual: {} ({:+.0}%)",
                format_duration(s.estimated_seconds),
                format_duration(s.actual_seconds),
                (s.percentage - 1.0) * 100.0
            );
            println!("    Status: {}", s.issue.status);
            println!();
        }
    }

    // Summary
    println!("\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}");
    println!("\u{2705} On Track: {on_track}  \u{26a0}\u{fe0f}  Warning: {warning}  \u{1f534} Over: {over}");

    // Average overage
    if !scope_statuses.is_empty() {
        #[allow(clippy::cast_precision_loss)] // count will never exceed f64 mantissa precision
        let avg_pct: f64 = scope_statuses.iter().map(|s| s.percentage).sum::<f64>() / scope_statuses.len() as f64;
        if avg_pct > 1.0 {
            println!("\u{1f4ca} Average: {:+.0}% over estimate", (avg_pct - 1.0) * 100.0);
        } else {
            println!("\u{1f4ca} Average: {:.0}% of estimate", avg_pct * 100.0);
        }
    }

    Ok(())
}

fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

fn truncate_title(title: &str, max_len: usize) -> String {
    if title.len() <= max_len {
        title.to_string()
    } else {
        format!("{}...", &title[..max_len - 3])
    }
}
