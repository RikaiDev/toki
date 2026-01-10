//! Smart Issue Matcher - AI-powered issue inference from activity context
//!
//! This module provides intelligent matching between developer activities
//! and PM system issues using multiple signals:
//! - Git commit messages
//! - Edited file paths
//! - Browser URL history (PM system pages)
//! - Window title patterns
//! - Semantic similarity analysis (embedding-based)

#[cfg(test)]
mod tests;

use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use uuid::Uuid;

use crate::embedding::EmbeddingService;
use toki_storage::db::Database;
use toki_storage::models::IssueCandidate;

/// Issue match result with confidence score
#[derive(Debug, Clone)]
pub struct IssueMatch {
    pub issue_id: String,
    pub confidence: f32, // 0.0 - 1.0
    pub match_reasons: Vec<MatchReason>,
}

/// Reason why an issue was matched
#[derive(Debug, Clone)]
pub enum MatchReason {
    CommitMessage(String),   // Found in commit message
    BranchName,              // Found in git branch
    BrowserUrl(String),      // Visited issue page
    FilePathPattern(String), // File path contains issue ID
    SemanticSimilarity(f32), // AI semantic match score
    RecentlyViewed,          // Recently viewed in PM system
    Assigned,                // User is assigned to this issue
}

/// Activity context collected for AI analysis
#[derive(Debug, Clone, Default)]
pub struct ActivitySignals {
    pub recent_commits: Vec<String>,
    pub edited_files: Vec<String>,
    pub browser_urls: Vec<String>,
    pub window_titles: Vec<String>,
    pub git_branch: Option<String>,
}

/// Issue from PM system for matching
#[derive(Debug, Clone)]
pub struct CandidateIssue {
    pub external_id: String, // e.g., "TOKI-9"
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub labels: Vec<String>,
    pub is_assigned_to_user: bool,
}

/// Smart issue matcher
pub struct IssueMatcher {
    issue_id_pattern: Regex,
}

impl IssueMatcher {
    /// Create a new issue matcher
    ///
    /// # Panics
    ///
    /// Panics if the issue ID regex pattern fails to compile (should never happen with valid pattern)
    #[must_use]
    pub fn new() -> Self {
        Self {
            // Matches common issue ID patterns: PROJ-123, ABC-1, etc.
            issue_id_pattern: Regex::new(r"(?i)([A-Z]{2,10}-\d+)").unwrap(),
        }
    }

    /// Find the best matching issue from candidates based on activity signals
    #[must_use]
    pub fn find_best_match(
        &self,
        signals: &ActivitySignals,
        candidates: &[CandidateIssue],
    ) -> Option<IssueMatch> {
        if candidates.is_empty() {
            return None;
        }

        let mut scores: HashMap<String, (f32, Vec<MatchReason>)> = HashMap::new();

        // Initialize scores for all candidates
        for candidate in candidates {
            scores.insert(candidate.external_id.clone(), (0.0, Vec::new()));
        }

        // 1. Check git branch (highest confidence)
        if let Some(branch) = &signals.git_branch {
            for id in self.extract_issue_ids(branch) {
                if let Some((score, reasons)) = scores.get_mut(&id) {
                    *score += 0.9;
                    reasons.push(MatchReason::BranchName);
                }
            }
        }

        // 2. Check browser URLs (visited issue page = high confidence)
        for url in &signals.browser_urls {
            for id in self.extract_issue_ids(url) {
                if let Some((score, reasons)) = scores.get_mut(&id) {
                    *score += 0.8;
                    reasons.push(MatchReason::BrowserUrl(url.clone()));
                }
            }
        }

        // 3. Check commit messages
        for commit in &signals.recent_commits {
            for id in self.extract_issue_ids(commit) {
                if let Some((score, reasons)) = scores.get_mut(&id) {
                    *score += 0.7;
                    reasons.push(MatchReason::CommitMessage(commit.clone()));
                }
            }
        }

        // 4. Check file paths
        for file in &signals.edited_files {
            for id in self.extract_issue_ids(file) {
                if let Some((score, reasons)) = scores.get_mut(&id) {
                    *score += 0.5;
                    reasons.push(MatchReason::FilePathPattern(file.clone()));
                }
            }
        }

        // 5. Check window titles
        for title in &signals.window_titles {
            for id in self.extract_issue_ids(title) {
                if let Some((score, reasons)) = scores.get_mut(&id) {
                    *score += 0.4;
                    reasons.push(MatchReason::RecentlyViewed);
                }
            }
        }

        // 6. Boost assigned issues
        for candidate in candidates {
            if candidate.is_assigned_to_user {
                if let Some((score, reasons)) = scores.get_mut(&candidate.external_id) {
                    *score += 0.3;
                    reasons.push(MatchReason::Assigned);
                }
            }
        }

        // 7. Semantic similarity (keyword matching as fallback)
        for candidate in candidates {
            let similarity = Self::calculate_semantic_similarity(signals, candidate);
            if similarity > 0.3 {
                if let Some((score, reasons)) = scores.get_mut(&candidate.external_id) {
                    *score += similarity * 0.5;
                    reasons.push(MatchReason::SemanticSimilarity(similarity));
                }
            }
        }

        // Find best match
        let best = scores
            .into_iter()
            .filter(|(_, (score, _))| *score > 0.0)
            .max_by(|a, b| {
                a.1 .0
                    .partial_cmp(&b.1 .0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        best.map(|(id, (score, reasons))| IssueMatch {
            issue_id: id,
            confidence: score.min(1.0),
            match_reasons: reasons,
        })
    }

    /// Extract issue IDs from text
    pub(crate) fn extract_issue_ids(&self, text: &str) -> Vec<String> {
        self.issue_id_pattern
            .find_iter(text)
            .map(|m| m.as_str().to_uppercase())
            .collect()
    }

    /// Calculate semantic similarity between signals and candidate issue
    pub(crate) fn calculate_semantic_similarity(
        signals: &ActivitySignals,
        candidate: &CandidateIssue,
    ) -> f32 {
        // Simple keyword matching (can be replaced with actual AI embeddings)
        let mut matches = 0;

        // Extract keywords from issue title and description
        let issue_text = format!(
            "{} {}",
            candidate.title,
            candidate.description.as_deref().unwrap_or("")
        )
        .to_lowercase();

        let keywords: Vec<&str> = issue_text
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .collect();

        let total_keywords = keywords.len().max(1);

        // Check if keywords appear in signals
        let signal_text = format!(
            "{} {} {}",
            signals.edited_files.join(" "),
            signals.recent_commits.join(" "),
            signals.window_titles.join(" ")
        )
        .to_lowercase();

        for keyword in &keywords {
            if signal_text.contains(*keyword) {
                matches += 1;
            }
        }

        // Check labels
        for label in &candidate.labels {
            if signal_text.contains(&label.to_lowercase()) {
                matches += 2; // Labels are more significant
            }
        }

        #[allow(clippy::cast_precision_loss)]
        let result = matches as f32 / total_keywords as f32;
        result
    }

    /// Suggest issue based on recent activity patterns
    /// Returns top N suggestions with confidence scores
    #[must_use]
    pub fn suggest_issues(
        &self,
        signals: &ActivitySignals,
        candidates: &[CandidateIssue],
        max_suggestions: usize,
    ) -> Vec<IssueMatch> {
        let mut all_scores: Vec<(String, f32, Vec<MatchReason>)> = Vec::new();

        for candidate in candidates {
            let mut score = 0.0f32;
            let mut reasons = Vec::new();

            // Apply all matching rules
            if let Some(branch) = &signals.git_branch {
                if self
                    .extract_issue_ids(branch)
                    .contains(&candidate.external_id.to_uppercase())
                {
                    score += 0.9;
                    reasons.push(MatchReason::BranchName);
                }
            }

            for url in &signals.browser_urls {
                if self
                    .extract_issue_ids(url)
                    .contains(&candidate.external_id.to_uppercase())
                {
                    score += 0.8;
                    reasons.push(MatchReason::BrowserUrl(url.clone()));
                }
            }

            for commit in &signals.recent_commits {
                if self
                    .extract_issue_ids(commit)
                    .contains(&candidate.external_id.to_uppercase())
                {
                    score += 0.7;
                    reasons.push(MatchReason::CommitMessage(commit.clone()));
                }
            }

            if candidate.is_assigned_to_user && candidate.status != "done" {
                score += 0.3;
                reasons.push(MatchReason::Assigned);
            }

            let similarity = Self::calculate_semantic_similarity(signals, candidate);
            if similarity > 0.2 {
                score += similarity * 0.4;
                reasons.push(MatchReason::SemanticSimilarity(similarity));
            }

            if score > 0.0 {
                all_scores.push((candidate.external_id.clone(), score, reasons));
            }
        }

        // Sort by score descending
        all_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top N
        all_scores
            .into_iter()
            .take(max_suggestions)
            .map(|(id, score, reasons)| IssueMatch {
                issue_id: id,
                confidence: score.min(1.0),
                match_reasons: reasons,
            })
            .collect()
    }
}

impl Default for IssueMatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Smart Issue Matcher - Embedding-based semantic matching
// ============================================================================

/// Smart issue matcher using embedding-based semantic similarity
///
/// This enhanced matcher uses local AI embeddings (fastembed) to compute
/// semantic similarity between activity context and issue candidates,
/// combined with rule-based matching for explicit issue references.
pub struct SmartIssueMatcher {
    embedding_service: Arc<Mutex<EmbeddingService>>,
    database: Arc<Database>,
    issue_id_pattern: Regex,
}

impl SmartIssueMatcher {
    /// Create a new smart issue matcher
    ///
    /// # Errors
    ///
    /// Returns an error if the embedding service fails to initialize
    ///
    /// # Panics
    ///
    /// Panics if the issue ID regex pattern fails to compile (should never happen with valid pattern)
    pub fn new(database: Arc<Database>) -> Result<Self> {
        let embedding_service = EmbeddingService::new()?;
        Ok(Self {
            embedding_service: Arc::new(Mutex::new(embedding_service)),
            database,
            issue_id_pattern: Regex::new(r"(?i)([A-Z]{2,10}-\d+)").unwrap(),
        })
    }

    /// Create with an existing embedding service (for sharing)
    ///
    /// # Panics
    ///
    /// Panics if the issue ID regex pattern fails to compile (should never happen with valid pattern)
    #[must_use]
    pub fn with_embedding_service(
        database: Arc<Database>,
        embedding_service: Arc<Mutex<EmbeddingService>>,
    ) -> Self {
        Self {
            embedding_service,
            database,
            issue_id_pattern: Regex::new(r"(?i)([A-Z]{2,10}-\d+)").unwrap(),
        }
    }

    /// Find best matching issues using hybrid scoring (rules + semantics)
    ///
    /// Signal weights:
    /// - Git branch with issue ID: 0.95 (near-certain)
    /// - Commit message with issue ID: 0.85
    /// - Browser URL with issue ID: 0.80
    /// - Semantic similarity > 0.5: 0.60 * similarity
    /// - Assigned to user: +0.20 boost
    /// - Status = `in_progress`: +0.10 boost
    ///
    /// # Errors
    ///
    /// Returns an error if database queries fail or embedding computation fails
    pub fn find_best_matches(
        &self,
        signals: &ActivitySignals,
        project_id: Uuid,
        max_results: usize,
    ) -> Result<Vec<IssueMatch>> {
        // Get active issue candidates from database
        let candidates = self.database.get_active_issue_candidates(project_id)?;

        if candidates.is_empty() {
            log::debug!("No issue candidates found for project {project_id}");
            return Ok(Vec::new());
        }

        // Generate context embedding from signals
        let context_embedding = self.generate_context_embedding(signals)?;

        let mut scores: Vec<(IssueCandidate, f32, Vec<MatchReason>)> = Vec::new();

        for candidate in candidates {
            let mut score = 0.0f32;
            let mut reasons = Vec::new();

            // 1. Explicit ID matching (highest confidence)
            if let Some(branch) = &signals.git_branch {
                if self.extract_issue_ids(branch).contains(&candidate.external_id.to_uppercase()) {
                    score += 0.95;
                    reasons.push(MatchReason::BranchName);
                }
            }

            // 2. Commit message matching
            for commit in &signals.recent_commits {
                if self.extract_issue_ids(commit).contains(&candidate.external_id.to_uppercase()) {
                    score += 0.85;
                    reasons.push(MatchReason::CommitMessage(commit.clone()));
                    break; // Only count once
                }
            }

            // 3. Browser URL matching
            for url in &signals.browser_urls {
                if self.extract_issue_ids(url).contains(&candidate.external_id.to_uppercase()) {
                    score += 0.80;
                    reasons.push(MatchReason::BrowserUrl(url.clone()));
                    break;
                }
            }

            // 4. Semantic similarity (only if we have embeddings)
            if let Some(ref issue_embedding) = candidate.embedding {
                let similarity =
                    EmbeddingService::cosine_similarity(&context_embedding, issue_embedding);

                // Lower threshold to 0.3 for weak context, stronger weight for higher similarity
                if similarity > 0.3 {
                    // Scale: 0.3-0.5 = low, 0.5-0.7 = medium, 0.7+ = high
                    let semantic_score = if similarity > 0.7 {
                        0.70 // High semantic match
                    } else if similarity > 0.5 {
                        similarity * 0.65 // Medium match
                    } else {
                        similarity * 0.50 // Low match, still useful
                    };
                    score += semantic_score;
                    reasons.push(MatchReason::SemanticSimilarity(similarity));
                }
            }

            // 5. Contextual boosts based on issue status
            let status_lower = candidate.status.to_lowercase();
            if status_lower == "in_progress" || status_lower == "in progress" || status_lower == "started" {
                score += 0.15; // Higher boost for actively worked issues
            } else if status_lower == "todo" || status_lower == "backlog" {
                score += 0.05; // Small boost for planned work
            }

            // Only include if we have some signal (lower threshold for weak context)
            if score > 0.0 {
                scores.push((candidate, score, reasons));
            }
        }

        // Sort by score descending
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top N
        Ok(scores
            .into_iter()
            .take(max_results)
            .map(|(candidate, score, reasons)| IssueMatch {
                issue_id: candidate.external_id,
                confidence: score.min(1.0),
                match_reasons: reasons,
            })
            .collect())
    }

    /// Generate embedding for activity context
    fn generate_context_embedding(&self, signals: &ActivitySignals) -> Result<Vec<f32>> {
        let context_text = Self::generate_context_text(signals);

        let mut service = self
            .embedding_service
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock embedding service: {e}"))?;

        service.generate_embedding(&context_text)
    }

    /// Generate text representation of activity signals for embedding
    pub(crate) fn generate_context_text(signals: &ActivitySignals) -> String {
        let mut parts = Vec::new();

        // Git branch (highest signal)
        if let Some(branch) = &signals.git_branch {
            parts.push(format!("Branch: {branch}"));
        }

        // Recent commits (high signal)
        for commit in signals.recent_commits.iter().take(3) {
            parts.push(format!("Commit: {commit}"));
        }

        // Edited files (extract meaningful names)
        for file in signals.edited_files.iter().take(5) {
            if let Some(name) = std::path::Path::new(file).file_name() {
                parts.push(format!("File: {}", name.to_string_lossy()));
            }
        }

        // Browser URLs (medium signal - may contain issue IDs or project context)
        for url in signals.browser_urls.iter().take(3) {
            // Extract meaningful part from URL
            if let Some(path) = url.split('/').next_back() {
                if !path.is_empty() && path.len() > 3 {
                    parts.push(format!("URL: {path}"));
                }
            }
        }

        // Window titles (important context, especially for block descriptions)
        for title in signals.window_titles.iter().take(5) {
            let title = title.trim();
            if !title.contains("Untitled") && !title.is_empty() {
                // This may be a block description like "Development on unknown" or "Meeting/Communication"
                parts.push(format!("Activity: {title}"));
            }
        }

        // If we have no signals, use a generic development context
        if parts.is_empty() {
            parts.push("Software development work".to_string());
        }

        parts.join("\n")
    }

    /// Extract issue IDs from text
    pub(crate) fn extract_issue_ids(&self, text: &str) -> Vec<String> {
        self.issue_id_pattern
            .find_iter(text)
            .map(|m| m.as_str().to_uppercase())
            .collect()
    }

    /// Format match reasons for display
    #[must_use]
    pub fn format_reasons(reasons: &[MatchReason]) -> String {
        reasons
            .iter()
            .map(|r| match r {
                MatchReason::BranchName => "Git branch".to_string(),
                MatchReason::CommitMessage(_) => "Commit message".to_string(),
                MatchReason::BrowserUrl(_) => "Browser URL".to_string(),
                MatchReason::FilePathPattern(_) => "File path".to_string(),
                MatchReason::SemanticSimilarity(s) => format!("Semantic ({:.0}%)", s * 100.0),
                MatchReason::RecentlyViewed => "Recently viewed".to_string(),
                MatchReason::Assigned => "Assigned".to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}
