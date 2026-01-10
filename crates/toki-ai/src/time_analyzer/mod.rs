//! Time Analyzer - Intelligent retroactive activity analysis and classification
//!
//! This module analyzes user work activities and suggests time classifications,
//! supporting various work habits:
//! - Multi-issue parallel processing
//! - Work without issues (refactoring, exploration)
//! - Product polishing phase
//! - Fragmented work sessions

use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::fmt::Write;

#[cfg(test)]
mod tests;

/// Convert i64 seconds to u32, clamping negative values to 0 and large values to `u32::MAX`
pub(crate) fn seconds_to_u32(seconds: i64) -> u32 {
    if seconds <= 0 {
        0
    } else if seconds > i64::from(u32::MAX) {
        u32::MAX
    } else {
        // Safety: value is guaranteed to be in range [1, u32::MAX] after the above checks
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let result = seconds as u32;
        result
    }
}

/// Calculate duration in seconds between two timestamps as u32
pub(crate) fn duration_seconds(start: DateTime<Utc>, end: DateTime<Utc>) -> u32 {
    seconds_to_u32((end - start).num_seconds())
}

/// Format duration
pub(crate) fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Activity segment (for analysis)
#[derive(Debug, Clone)]
pub struct ActivitySegment {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub project_name: Option<String>,
    pub category: String,
    pub edited_files: Vec<String>,
    pub git_commits: Vec<String>,
    pub git_branch: Option<String>,
    pub browser_urls: Vec<String>,
}

/// Suggested time block
#[derive(Debug, Clone)]
pub struct SuggestedTimeBlock {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_seconds: u32,
    pub suggested_description: String,
    pub suggested_issues: Vec<SuggestedIssue>,
    pub confidence: f32,
    pub reasoning: Vec<String>, // Explanation for the suggestion
}

#[derive(Debug, Clone)]
pub struct SuggestedIssue {
    pub issue_id: String,
    pub confidence: f32,
    pub reason: String,
}

/// Work pattern
#[derive(Debug, Clone, PartialEq)]
pub enum WorkPattern {
    SingleFocus,  // Focused on single task
    MultiTasking, // Multiple tasks in parallel
    Exploration,  // Exploration/learning
    Maintenance,  // Maintenance/polishing
    CodeReview,
    Debugging,
    Meeting,
    Documentation,
    Unknown,
}

/// Time analyzer
pub struct TimeAnalyzer {
    min_block_duration: Duration, // Minimum time block duration
}

impl TimeAnalyzer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            min_block_duration: Duration::minutes(5),
        }
    }

    /// Analyze activity segments and suggest time block classifications
    #[must_use]
    pub fn analyze_and_suggest(&self, segments: &[ActivitySegment]) -> Vec<SuggestedTimeBlock> {
        if segments.is_empty() {
            return Vec::new();
        }

        let mut suggestions = Vec::new();
        let mut current_block: Option<SuggestedTimeBlock> = None;
        let mut current_pattern: Option<WorkPattern> = None;

        for segment in segments {
            let pattern = Self::detect_pattern(segment);
            let should_merge = current_pattern.as_ref() == Some(&pattern)
                && Self::should_merge_segments(
                    current_block.as_ref().map(|b| b.end_time),
                    segment.start_time,
                );

            if should_merge {
                // Merge into current block
                if let Some(block) = &mut current_block {
                    block.end_time = segment.end_time;
                    block.duration_seconds = duration_seconds(block.start_time, block.end_time);

                    // Update suggested issues
                    Self::update_suggested_issues(block, segment);
                }
            } else {
                // Save current block, start new one
                if let Some(block) = current_block.take() {
                    let min_seconds = seconds_to_u32(self.min_block_duration.num_seconds());
                    if block.duration_seconds >= min_seconds {
                        suggestions.push(block);
                    }
                }

                current_block = Some(Self::create_time_block(segment, &pattern));
                current_pattern = Some(pattern);
            }
        }

        // Save the last time block
        if let Some(block) = current_block {
            let min_seconds = seconds_to_u32(self.min_block_duration.num_seconds());
            if block.duration_seconds >= min_seconds {
                suggestions.push(block);
            }
        }

        suggestions
    }

    /// Detect work pattern
    pub(crate) fn detect_pattern(segment: &ActivitySegment) -> WorkPattern {
        // Infer from file types
        let file_patterns: HashMap<&str, WorkPattern> = [
            ("test", WorkPattern::Debugging),
            ("spec", WorkPattern::Debugging),
            ("readme", WorkPattern::Documentation),
            ("doc", WorkPattern::Documentation),
            ("changelog", WorkPattern::Documentation),
        ]
        .into_iter()
        .collect();

        for file in &segment.edited_files {
            let lower = file.to_lowercase();
            for (pattern, work_type) in &file_patterns {
                if lower.contains(pattern) {
                    return work_type.clone();
                }
            }
        }

        // Infer from commit messages
        for commit in &segment.git_commits {
            let lower = commit.to_lowercase();
            if lower.contains("fix") || lower.contains("bug") {
                return WorkPattern::Debugging;
            }
            if lower.contains("refactor") || lower.contains("clean") {
                return WorkPattern::Maintenance;
            }
            if lower.contains("docs") || lower.contains("readme") {
                return WorkPattern::Documentation;
            }
            if lower.contains("review") {
                return WorkPattern::CodeReview;
            }
        }

        // Infer from URLs
        for url in &segment.browser_urls {
            let lower = url.to_lowercase();
            if lower.contains("pull") || lower.contains("merge") {
                return WorkPattern::CodeReview;
            }
            if lower.contains("issue") || lower.contains("ticket") {
                return WorkPattern::SingleFocus;
            }
        }

        // Infer from number of edited files
        if segment.edited_files.len() > 5 {
            return WorkPattern::MultiTasking;
        }

        // Infer from category
        match segment.category.as_str() {
            "Browser" => {
                if segment.browser_urls.iter().any(|u| {
                    u.contains("stackoverflow") || u.contains("docs") || u.contains("learn")
                }) {
                    return WorkPattern::Exploration;
                }
            }
            "Communication" => return WorkPattern::Meeting,
            _ => {}
        }

        WorkPattern::SingleFocus
    }

    /// Determine if two time segments should be merged
    pub(crate) fn should_merge_segments(prev_end: Option<DateTime<Utc>>, next_start: DateTime<Utc>) -> bool {
        let Some(end) = prev_end else {
            return false;
        };
        let gap = next_start - end;
        // If gap is less than 10 minutes, treat as same block
        gap < Duration::minutes(10)
    }

    /// Create new time block
    fn create_time_block(segment: &ActivitySegment, pattern: &WorkPattern) -> SuggestedTimeBlock {
        let duration = duration_seconds(segment.start_time, segment.end_time);

        let description = Self::generate_description(segment, pattern);
        let suggested_issues = Self::extract_issues(segment);
        let confidence = Self::calculate_confidence(&suggested_issues, pattern);
        let reasoning = Self::generate_reasoning(segment, pattern, &suggested_issues);

        SuggestedTimeBlock {
            start_time: segment.start_time,
            end_time: segment.end_time,
            duration_seconds: duration,
            suggested_description: description,
            suggested_issues,
            confidence,
            reasoning,
        }
    }

    /// Update time block's suggested issues
    fn update_suggested_issues(block: &mut SuggestedTimeBlock, segment: &ActivitySegment) {
        let new_issues = Self::extract_issues(segment);
        for issue in new_issues {
            if !block
                .suggested_issues
                .iter()
                .any(|i| i.issue_id == issue.issue_id)
            {
                block.suggested_issues.push(issue);
            }
        }
        block.confidence =
            Self::calculate_confidence(&block.suggested_issues, &WorkPattern::Unknown);
    }

    /// Generate description
    pub(crate) fn generate_description(segment: &ActivitySegment, pattern: &WorkPattern) -> String {
        let project = segment.project_name.as_deref().unwrap_or("unknown");

        match pattern {
            WorkPattern::SingleFocus => {
                if let Some(commit) = segment.git_commits.first() {
                    return commit.clone();
                }
                format!("Development on {project}")
            }
            WorkPattern::MultiTasking => format!("Multi-tasking - {project}"),
            WorkPattern::Exploration => "Exploration/Learning".to_string(),
            WorkPattern::Maintenance => format!("{project} maintenance/refactoring"),
            WorkPattern::CodeReview => "Code Review".to_string(),
            WorkPattern::Debugging => format!("{project} debugging"),
            WorkPattern::Meeting => "Meeting/Communication".to_string(),
            WorkPattern::Documentation => "Documentation".to_string(),
            WorkPattern::Unknown => format!("Working on {project}"),
        }
    }

    /// Extract possible issues from activity
    pub(crate) fn extract_issues(segment: &ActivitySegment) -> Vec<SuggestedIssue> {
        let mut issues = Vec::new();
        let issue_pattern = regex::Regex::new(r"(?i)([A-Z]{2,10}-\d+)").unwrap();

        // From git branch
        if let Some(branch) = &segment.git_branch {
            for cap in issue_pattern.captures_iter(branch) {
                issues.push(SuggestedIssue {
                    issue_id: cap[1].to_uppercase(),
                    confidence: 0.9,
                    reason: "Detected from Git branch".to_string(),
                });
            }
        }

        // From commit messages
        for commit in &segment.git_commits {
            for cap in issue_pattern.captures_iter(commit) {
                let id = cap[1].to_uppercase();
                if !issues.iter().any(|i| i.issue_id == id) {
                    issues.push(SuggestedIssue {
                        issue_id: id,
                        confidence: 0.8,
                        reason: format!("From commit: {commit}"),
                    });
                }
            }
        }

        // From URLs
        for url in &segment.browser_urls {
            for cap in issue_pattern.captures_iter(url) {
                let id = cap[1].to_uppercase();
                if !issues.iter().any(|i| i.issue_id == id) {
                    issues.push(SuggestedIssue {
                        issue_id: id,
                        confidence: 0.7,
                        reason: "Visited this issue page".to_string(),
                    });
                }
            }
        }

        issues
    }

    /// Calculate confidence score
    pub(crate) fn calculate_confidence(issues: &[SuggestedIssue], pattern: &WorkPattern) -> f32 {
        if issues.is_empty() {
            // No issue found, but may be valid work
            return match pattern {
                WorkPattern::Exploration
                | WorkPattern::Maintenance
                | WorkPattern::Documentation => 0.6,
                WorkPattern::Meeting | WorkPattern::CodeReview => 0.7,
                _ => 0.3,
            };
        }

        // Take highest confidence
        issues.iter().map(|i| i.confidence).fold(0.0, f32::max)
    }

    /// Generate reasoning explanation
    pub(crate) fn generate_reasoning(
        segment: &ActivitySegment,
        pattern: &WorkPattern,
        issues: &[SuggestedIssue],
    ) -> Vec<String> {
        let mut reasons = Vec::new();

        reasons.push(format!("Work pattern: {pattern:?}"));

        if !segment.edited_files.is_empty() {
            reasons.push(format!("Edited {} files", segment.edited_files.len()));
        }

        if !segment.git_commits.is_empty() {
            reasons.push(format!("Made {} commits", segment.git_commits.len()));
        }

        for issue in issues {
            reasons.push(format!("{}: {}", issue.issue_id, issue.reason));
        }

        if issues.is_empty() {
            reasons.push("No related Issue ID detected".to_string());
            reasons
                .push("Suggestion: Can be manually tagged or kept as general dev time".to_string());
        }

        reasons
    }

    /// Generate daily summary
    #[must_use]
    pub fn generate_daily_summary(
        &self,
        date: chrono::NaiveDate,
        segments: &[ActivitySegment],
    ) -> DailySummaryReport {
        let suggestions = self.analyze_and_suggest(segments);

        let total_seconds: u32 = segments
            .iter()
            .map(|s| duration_seconds(s.start_time, s.end_time))
            .sum();

        let classified_seconds: u32 = suggestions
            .iter()
            .filter(|s| !s.suggested_issues.is_empty())
            .map(|s| s.duration_seconds)
            .sum();

        let unclassified_seconds = total_seconds.saturating_sub(classified_seconds);

        // Count time per project
        let mut project_times: HashMap<String, u32> = HashMap::new();
        for segment in segments {
            let project = segment
                .project_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let segment_duration = duration_seconds(segment.start_time, segment.end_time);
            *project_times.entry(project).or_insert(0) += segment_duration;
        }

        DailySummaryReport {
            date,
            total_active_seconds: total_seconds,
            classified_seconds,
            unclassified_seconds,
            project_breakdown: project_times,
            suggested_blocks: suggestions,
        }
    }
}

impl Default for TimeAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Daily summary report
#[derive(Debug)]
pub struct DailySummaryReport {
    pub date: chrono::NaiveDate,
    pub total_active_seconds: u32,
    pub classified_seconds: u32,
    pub unclassified_seconds: u32,
    pub project_breakdown: HashMap<String, u32>,
    pub suggested_blocks: Vec<SuggestedTimeBlock>,
}

impl DailySummaryReport {
    /// Format as readable report
    #[must_use]
    pub fn format_report(&self) -> String {
        let mut report = String::new();

        let _ = writeln!(report, "=== {} Work Summary ===", self.date);
        let _ = writeln!(
            report,
            "Total active time: {}",
            format_duration(self.total_active_seconds)
        );
        let percentage = if self.total_active_seconds > 0 {
            f64::from(self.classified_seconds) / f64::from(self.total_active_seconds) * 100.0
        } else {
            0.0
        };
        let _ = writeln!(
            report,
            "Classified time: {} ({:.0}%)",
            format_duration(self.classified_seconds),
            percentage
        );
        let _ = writeln!(
            report,
            "Unclassified time: {}\n",
            format_duration(self.unclassified_seconds)
        );

        report.push_str("Project breakdown:\n");
        let mut projects: Vec<_> = self.project_breakdown.iter().collect();
        projects.sort_by(|a, b| b.1.cmp(a.1));
        for (project, seconds) in projects {
            let _ = writeln!(
                report,
                "   \u{2022} {}: {}",
                project,
                format_duration(*seconds)
            );
        }

        if !self.suggested_blocks.is_empty() {
            report.push_str("\nAI suggested time blocks:\n");
            for (i, block) in self.suggested_blocks.iter().enumerate() {
                let start = block.start_time.format("%H:%M");
                let end = block.end_time.format("%H:%M");
                let _ = writeln!(
                    report,
                    "   {}. {} - {} ({})",
                    i + 1,
                    start,
                    end,
                    format_duration(block.duration_seconds)
                );
                let _ = writeln!(report, "      {}", block.suggested_description);
                if !block.suggested_issues.is_empty() {
                    let issues: Vec<_> = block
                        .suggested_issues
                        .iter()
                        .map(|i| i.issue_id.as_str())
                        .collect();
                    let _ = writeln!(report, "      Related: {}", issues.join(", "));
                }
                let _ = writeln!(
                    report,
                    "      Confidence: {:.0}%",
                    block.confidence * 100.0
                );
            }
        }

        report
    }
}
