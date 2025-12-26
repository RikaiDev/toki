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
            let pattern = self.detect_pattern(segment);
            let should_merge = current_pattern.as_ref() == Some(&pattern)
                && self.should_merge_segments(
                    current_block.as_ref().map(|b| b.end_time),
                    segment.start_time,
                );

            if should_merge {
                // Merge into current block
                if let Some(block) = &mut current_block {
                    block.end_time = segment.end_time;
                    block.duration_seconds =
                        (block.end_time - block.start_time).num_seconds().max(0) as u32;

                    // Update suggested issues
                    self.update_suggested_issues(block, segment);
                }
            } else {
                // Save current block, start new one
                if let Some(block) = current_block.take() {
                    if block.duration_seconds >= self.min_block_duration.num_seconds() as u32 {
                        suggestions.push(block);
                    }
                }

                current_block = Some(self.create_time_block(segment, &pattern));
                current_pattern = Some(pattern);
            }
        }

        // Save the last time block
        if let Some(block) = current_block {
            if block.duration_seconds >= self.min_block_duration.num_seconds() as u32 {
                suggestions.push(block);
            }
        }

        suggestions
    }

    /// Detect work pattern
    fn detect_pattern(&self, segment: &ActivitySegment) -> WorkPattern {
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
    fn should_merge_segments(
        &self,
        prev_end: Option<DateTime<Utc>>,
        next_start: DateTime<Utc>,
    ) -> bool {
        if let Some(end) = prev_end {
            let gap = next_start - end;
            // If gap is less than 10 minutes, treat as same block
            gap < Duration::minutes(10)
        } else {
            false
        }
    }

    /// Create new time block
    fn create_time_block(
        &self,
        segment: &ActivitySegment,
        pattern: &WorkPattern,
    ) -> SuggestedTimeBlock {
        let duration = (segment.end_time - segment.start_time).num_seconds().max(0) as u32;

        let description = self.generate_description(segment, pattern);
        let suggested_issues = self.extract_issues(segment);
        let confidence = self.calculate_confidence(&suggested_issues, pattern);
        let reasoning = self.generate_reasoning(segment, pattern, &suggested_issues);

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
    fn update_suggested_issues(&self, block: &mut SuggestedTimeBlock, segment: &ActivitySegment) {
        let new_issues = self.extract_issues(segment);
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
            self.calculate_confidence(&block.suggested_issues, &WorkPattern::Unknown);
    }

    /// Generate description
    fn generate_description(&self, segment: &ActivitySegment, pattern: &WorkPattern) -> String {
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
    fn extract_issues(&self, segment: &ActivitySegment) -> Vec<SuggestedIssue> {
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
    fn calculate_confidence(&self, issues: &[SuggestedIssue], pattern: &WorkPattern) -> f32 {
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
    fn generate_reasoning(
        &self,
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
            .map(|s| (s.end_time - s.start_time).num_seconds().max(0) as u32)
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
            let duration = (segment.end_time - segment.start_time).num_seconds().max(0) as u32;
            *project_times.entry(project).or_insert(0) += duration;
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

        report.push_str(&format!("=== {} Work Summary ===\n", self.date));
        report.push_str(&format!(
            "Total active time: {}\n",
            format_duration(self.total_active_seconds)
        ));
        report.push_str(&format!(
            "Classified time: {} ({:.0}%)\n",
            format_duration(self.classified_seconds),
            if self.total_active_seconds > 0 {
                self.classified_seconds as f32 / self.total_active_seconds as f32 * 100.0
            } else {
                0.0
            }
        ));
        report.push_str(&format!(
            "Unclassified time: {}\n\n",
            format_duration(self.unclassified_seconds)
        ));

        report.push_str("Project breakdown:\n");
        let mut projects: Vec<_> = self.project_breakdown.iter().collect();
        projects.sort_by(|a, b| b.1.cmp(a.1));
        for (project, seconds) in projects {
            report.push_str(&format!(
                "   \u{2022} {}: {}\n",
                project,
                format_duration(*seconds)
            ));
        }

        if !self.suggested_blocks.is_empty() {
            report.push_str("\nAI suggested time blocks:\n");
            for (i, block) in self.suggested_blocks.iter().enumerate() {
                let start = block.start_time.format("%H:%M");
                let end = block.end_time.format("%H:%M");
                report.push_str(&format!(
                    "   {}. {} - {} ({})\n",
                    i + 1,
                    start,
                    end,
                    format_duration(block.duration_seconds)
                ));
                report.push_str(&format!("      {}\n", block.suggested_description));
                if !block.suggested_issues.is_empty() {
                    let issues: Vec<_> = block
                        .suggested_issues
                        .iter()
                        .map(|i| i.issue_id.as_str())
                        .collect();
                    report.push_str(&format!("      Related: {}\n", issues.join(", ")));
                }
                report.push_str(&format!(
                    "      Confidence: {:.0}%\n",
                    block.confidence * 100.0
                ));
            }
        }

        report
    }
}

/// Format duration
fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_pattern_from_commits() {
        let analyzer = TimeAnalyzer::new();

        let segment = ActivitySegment {
            start_time: Utc::now(),
            end_time: Utc::now(),
            project_name: Some("test".to_string()),
            category: "Coding".to_string(),
            edited_files: vec![],
            git_commits: vec!["fix: resolve button bug".to_string()],
            git_branch: None,
            browser_urls: vec![],
        };

        assert_eq!(analyzer.detect_pattern(&segment), WorkPattern::Debugging);
    }

    #[test]
    fn test_extract_issues_from_branch() {
        let analyzer = TimeAnalyzer::new();

        let segment = ActivitySegment {
            start_time: Utc::now(),
            end_time: Utc::now(),
            project_name: None,
            category: "Coding".to_string(),
            edited_files: vec![],
            git_commits: vec![],
            git_branch: Some("feature/TOKI-42-add-feature".to_string()),
            browser_urls: vec![],
        };

        let issues = analyzer.extract_issues(&segment);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_id, "TOKI-42");
    }
}
