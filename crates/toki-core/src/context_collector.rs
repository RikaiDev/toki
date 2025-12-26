//! Context Signal Collector
//!
//! Collects contextual signals for AI work item inference:
//! - Git branch and recent commits
//! - Recently edited files (from IDE workspace)
//! - Window titles over time
//! - Browser URLs (if enabled, for PM system pages)
//!
//! Also computes context vectors for Semantic Gravity calculation.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;
use toki_detector::GitDetector;
use uuid::Uuid;

/// Maximum number of signals to keep in memory before flushing
const MAX_SIGNALS_IN_MEMORY: usize = 100;

/// Maximum age of signals to keep (24 hours)
const MAX_SIGNAL_AGE_HOURS: i64 = 24;

/// A collected context signal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSignal {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub signal_type: SignalType,
    pub value: String,
    /// Optional project association
    pub project_id: Option<Uuid>,
    /// Optional activity span association
    pub span_id: Option<Uuid>,
}

/// Types of context signals we collect
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SignalType {
    /// Git branch name (e.g., "feature/CMUMS-1")
    GitBranch,
    /// Git commit message
    GitCommit,
    /// File being edited
    EditedFile,
    /// Window title
    WindowTitle,
    /// Browser URL (PM system pages)
    BrowserUrl,
    /// IDE workspace/project name
    WorkspaceName,
}

impl SignalType {
    /// Get the weight of this signal type for AI matching
    /// Higher weight = more important for work item inference
    #[must_use]
    pub const fn weight(&self) -> f32 {
        match self {
            Self::GitBranch => 1.0,      // Highest - explicit issue reference
            Self::GitCommit => 0.9,      // Very high - explicit description
            Self::BrowserUrl => 0.8,     // High - PM system pages
            Self::WorkspaceName => 0.6,  // Medium - project context
            Self::EditedFile => 0.4,     // Low-medium - indirect signal
            Self::WindowTitle => 0.3,    // Low - can be noisy
        }
    }
}

/// Collects and manages context signals
pub struct ContextCollector {
    /// Recent signals in memory
    signals: VecDeque<ContextSignal>,
    /// Git detector for branch/commit info
    git_detector: GitDetector,
    /// Current project ID (if known)
    current_project_id: Option<Uuid>,
    /// Current activity span ID (if active)
    current_span_id: Option<Uuid>,
    /// Last collected git branch (to avoid duplicates)
    last_git_branch: Option<String>,
    /// Last collected window title (to avoid duplicates)
    last_window_title: Option<String>,
    /// Cached project context vector
    context_vector: Option<Vec<f32>>,
}

impl ContextCollector {
    /// Create a new context collector
    #[must_use]
    pub fn new() -> Self {
        Self {
            signals: VecDeque::with_capacity(MAX_SIGNALS_IN_MEMORY),
            git_detector: GitDetector::new(),
            current_project_id: None,
            current_span_id: None,
            last_git_branch: None,
            last_window_title: None,
            context_vector: None,
        }
    }

    /// Set the current project context
    pub fn set_project(&mut self, project_id: Option<Uuid>) {
        if self.current_project_id != project_id {
            self.current_project_id = project_id;
            // Invalidate context vector when project changes
            self.context_vector = None;
        }
    }

    /// Set the current activity span
    pub fn set_span(&mut self, span_id: Option<Uuid>) {
        self.current_span_id = span_id;
    }

    /// Collect git branch signal from a workspace path
    pub fn collect_git_branch(&mut self, workspace_path: &Path) -> Result<Option<String>> {
        if let Ok(Some(issue_id)) = self.git_detector.detect_from_git(workspace_path) {
            let branch_name = issue_id.full_id();
            
            // Only add if different from last collected
            if self.last_git_branch.as_ref() != Some(&branch_name) {
                self.add_signal(SignalType::GitBranch, branch_name.clone());
                self.last_git_branch = Some(branch_name.clone());
            }
            
            return Ok(Some(branch_name));
        }
        Ok(None)
    }

    /// Collect window title signal
    pub fn collect_window_title(&mut self, title: &str) {
        // Only add if different from last collected and not empty
        if !title.is_empty() && self.last_window_title.as_deref() != Some(title) {
            self.add_signal(SignalType::WindowTitle, title.to_string());
            self.last_window_title = Some(title.to_string());
        }
    }

    /// Collect edited file signal
    pub fn collect_edited_file(&mut self, file_path: &str) {
        // Normalize path and add
        let normalized = file_path.replace('\\', "/");
        self.add_signal(SignalType::EditedFile, normalized);
    }

    /// Collect git commit message
    pub fn collect_git_commit(&mut self, message: &str) {
        if !message.is_empty() {
            self.add_signal(SignalType::GitCommit, message.to_string());
        }
    }

    /// Collect browser URL (only for allowed domains)
    pub fn collect_browser_url(&mut self, url: &str, allowed_domains: &[String]) {
        // Check if URL is from an allowed domain
        for domain in allowed_domains {
            if url.contains(domain) {
                self.add_signal(SignalType::BrowserUrl, url.to_string());
                break;
            }
        }
    }

    /// Collect workspace/project name
    pub fn collect_workspace_name(&mut self, name: &str) {
        if !name.is_empty() {
            self.add_signal(SignalType::WorkspaceName, name.to_string());
        }
    }

    /// Add a signal to the collection
    fn add_signal(&mut self, signal_type: SignalType, value: String) {
        let signal = ContextSignal {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            signal_type,
            value,
            project_id: self.current_project_id,
            span_id: self.current_span_id,
        };

        self.signals.push_back(signal);

        // Trim old signals if we have too many
        while self.signals.len() > MAX_SIGNALS_IN_MEMORY {
            self.signals.pop_front();
        }
        
        // Invalidate context vector as signals change
        self.context_vector = None;
    }

    /// Get all signals collected in the last N hours
    #[must_use]
    pub fn get_recent_signals(&self, hours: i64) -> Vec<&ContextSignal> {
        let cutoff = Utc::now() - chrono::Duration::hours(hours);
        self.signals
            .iter()
            .filter(|s| s.timestamp > cutoff)
            .collect()
    }

    /// Get signals for a specific project
    #[must_use]
    pub fn get_signals_for_project(&self, project_id: Uuid) -> Vec<&ContextSignal> {
        self.signals
            .iter()
            .filter(|s| s.project_id == Some(project_id))
            .collect()
    }

    /// Get signals for a specific span
    #[must_use]
    pub fn get_signals_for_span(&self, span_id: Uuid) -> Vec<&ContextSignal> {
        self.signals
            .iter()
            .filter(|s| s.span_id == Some(span_id))
            .collect()
    }

    /// Get unique git branches from recent signals
    #[must_use]
    pub fn get_unique_branches(&self) -> Vec<String> {
        let mut branches: Vec<String> = self.signals
            .iter()
            .filter(|s| s.signal_type == SignalType::GitBranch)
            .map(|s| s.value.clone())
            .collect();
        branches.sort();
        branches.dedup();
        branches
    }

    /// Get unique edited files from recent signals
    #[must_use]
    pub fn get_unique_files(&self) -> Vec<String> {
        let mut files: Vec<String> = self.signals
            .iter()
            .filter(|s| s.signal_type == SignalType::EditedFile)
            .map(|s| s.value.clone())
            .collect();
        files.sort();
        files.dedup();
        files
    }

    /// Clear old signals (older than `MAX_SIGNAL_AGE_HOURS`)
    pub fn clear_old_signals(&mut self) {
        let cutoff = Utc::now() - chrono::Duration::hours(MAX_SIGNAL_AGE_HOURS);
        self.signals.retain(|s| s.timestamp > cutoff);
    }

    /// Get all signals and clear the buffer
    pub fn drain_signals(&mut self) -> Vec<ContextSignal> {
        self.signals.drain(..).collect()
    }

    /// Get signal count
    #[must_use]
    pub fn signal_count(&self) -> usize {
        self.signals.len()
    }

    /// Aggregate signals into a summary for AI analysis
    #[must_use]
    pub fn get_signal_summary(&self) -> SignalSummary {
        let signals = self.get_recent_signals(24);
        
        let mut git_branches = Vec::new();
        let mut git_commits = Vec::new();
        let mut edited_files = Vec::new();
        let mut window_titles = Vec::new();
        let mut browser_urls = Vec::new();

        for signal in signals {
            match signal.signal_type {
                SignalType::GitBranch => git_branches.push(signal.value.clone()),
                SignalType::GitCommit => git_commits.push(signal.value.clone()),
                SignalType::EditedFile => edited_files.push(signal.value.clone()),
                SignalType::WindowTitle => window_titles.push(signal.value.clone()),
                SignalType::BrowserUrl => browser_urls.push(signal.value.clone()),
                SignalType::WorkspaceName => {}
            }
        }

        // Deduplicate
        git_branches.sort();
        git_branches.dedup();
        edited_files.sort();
        edited_files.dedup();
        
        SignalSummary {
            git_branches,
            git_commits,
            edited_files,
            window_titles,
            browser_urls,
        }
    }

    /// Calculate current context text for embedding
    /// Combines recent project signals into a single text block
    #[must_use]
    pub fn get_context_text(&self) -> String {
        let summary = self.get_signal_summary();
        
        let mut parts = Vec::new();
        
        // Project name from workspace signals
        for signal in self.get_recent_signals(1) {
            if signal.signal_type == SignalType::WorkspaceName {
                parts.push(format!("Project: {}", signal.value));
            }
        }

        if !summary.git_branches.is_empty() {
            parts.push(format!("Branch: {}", summary.git_branches.join(", ")));
        }

        if !summary.git_commits.is_empty() {
             let commits: Vec<_> = summary.git_commits.iter().take(3).collect();
             parts.push(format!("Commits: {}", commits.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("; ")));
        }

        if !summary.edited_files.is_empty() {
            let files: Vec<_> = summary.edited_files.iter().take(5).collect();
            parts.push(format!("Files: {}", files.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
        }
        
        parts.join("\n")
    }
}

impl Default for ContextCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregated signal summary for AI analysis
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SignalSummary {
    pub git_branches: Vec<String>,
    pub git_commits: Vec<String>,
    pub edited_files: Vec<String>,
    pub window_titles: Vec<String>,
    pub browser_urls: Vec<String>,
}

impl SignalSummary {
    /// Check if we have any meaningful signals
    #[must_use]
    pub fn has_signals(&self) -> bool {
        !self.git_branches.is_empty()
            || !self.git_commits.is_empty()
            || !self.edited_files.is_empty()
            || !self.browser_urls.is_empty()
    }

    /// Get a text representation for AI prompt
    #[must_use]
    pub fn to_prompt_text(&self) -> String {
        let mut parts = Vec::new();

        if !self.git_branches.is_empty() {
            parts.push(format!("Git branches: {}", self.git_branches.join(", ")));
        }
        if !self.git_commits.is_empty() {
            let commits: Vec<_> = self.git_commits.iter().take(5).collect();
            parts.push(format!("Recent commits: {}", commits.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("; ")));
        }
        if !self.edited_files.is_empty() {
            let files: Vec<_> = self.edited_files.iter().take(10).collect();
            parts.push(format!("Edited files: {}", files.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
        }
        if !self.browser_urls.is_empty() {
            parts.push(format!("PM URLs: {}", self.browser_urls.join(", ")));
        }

        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_collection() {
        let mut collector = ContextCollector::new();
        
        collector.collect_window_title("main.rs - toki - Cursor");
        collector.collect_edited_file("/path/to/main.rs");
        
        assert_eq!(collector.signal_count(), 2);
    }

    #[test]
    fn test_duplicate_prevention() {
        let mut collector = ContextCollector::new();
        
        collector.collect_window_title("same title");
        collector.collect_window_title("same title");
        collector.collect_window_title("different title");
        
        // Should only have 2 signals (duplicates filtered)
        assert_eq!(collector.signal_count(), 2);
    }

    #[test]
    fn test_signal_summary() {
        let mut collector = ContextCollector::new();
        
        collector.add_signal(SignalType::GitBranch, "feature/PROJ-123".to_string());
        collector.add_signal(SignalType::EditedFile, "src/main.rs".to_string());
        
        let summary = collector.get_signal_summary();
        assert!(summary.has_signals());
        assert_eq!(summary.git_branches.len(), 1);
        assert_eq!(summary.edited_files.len(), 1);
    }
}
