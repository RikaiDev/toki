//! Auto Project Linker - Automatically link local projects to PM systems
//!
//! Implements multiple strategies for automatic project linking:
//! - Browser URL detection: Extract project ID from visited PM pages
//! - Project name matching: Fuzzy match local project names to PM projects
//! - Git remote inference: Parse git remote URLs for project hints

#[cfg(test)]
mod tests;

use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use toki_integrations::plane::{PlaneClient, PlaneProject};
use toki_storage::db::Database;

/// Result of an auto-link attempt
#[derive(Debug, Clone)]
pub struct LinkSuggestion {
    pub local_project_id: Uuid,
    pub local_project_name: String,
    pub pm_project_id: String,
    pub pm_project_identifier: String,
    pub pm_project_name: String,
    pub confidence: f32,
    pub reason: LinkReason,
}

/// Reason for the link suggestion
#[derive(Debug, Clone)]
pub enum LinkReason {
    /// Found PM project ID in browser URL
    BrowserUrl(String),
    /// Project name exactly matches
    ExactNameMatch,
    /// Project name fuzzy matches
    FuzzyNameMatch(f32),
    /// Git remote contains project hint
    GitRemote(String),
    /// User explicitly visited issue page for this project
    IssuePageVisit(String),
}

impl std::fmt::Display for LinkReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BrowserUrl(url) => write!(f, "Browser URL: {}", truncate(url, 40)),
            Self::ExactNameMatch => write!(f, "Exact name match"),
            Self::FuzzyNameMatch(score) => write!(f, "Name similarity: {:.0}%", score * 100.0),
            Self::GitRemote(remote) => write!(f, "Git remote: {}", truncate(remote, 40)),
            Self::IssuePageVisit(issue) => write!(f, "Visited issue: {issue}"),
        }
    }
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
    }
}

/// Service for automatically linking projects to PM systems
pub struct AutoLinker {
    database: Arc<Database>,
    /// Regex to extract project identifier from Plane URLs
    plane_url_pattern: Regex,
    /// Regex to extract issue ID from URLs
    issue_id_pattern: Regex,
}

impl AutoLinker {
    /// Create a new auto linker
    ///
    /// # Panics
    ///
    /// Panics if the internal regex patterns fail to compile (should never happen
    /// as these are compile-time constant patterns).
    #[must_use]
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            // Matches: plane.so/workspace/projects/PROJECT_UUID or /PROJECT_ID/
            plane_url_pattern: Regex::new(
                r"plane\.so/[^/]+/projects?/([a-zA-Z0-9-]+)"
            ).unwrap(),
            // Matches: PROJ-123 style issue IDs
            issue_id_pattern: Regex::new(r"([A-Z]{2,10})-(\d+)").unwrap(),
        }
    }

    /// Analyze browser URLs to suggest project links
    ///
    /// When a user visits a Plane.so issue page, we can infer which PM project
    /// they're working on and suggest linking it to the current local project.
    ///
    /// # Errors
    ///
    /// Returns an error if the Plane API call fails or database operations fail.
    pub async fn suggest_from_browser_urls(
        &self,
        urls: &[String],
        current_project_id: Uuid,
        plane_client: &PlaneClient,
    ) -> Result<Vec<LinkSuggestion>> {
        let mut suggestions = Vec::new();
        let pm_projects = plane_client.list_projects().await?;

        // Get current local project
        let Some(local_project) = self.database.get_project(current_project_id)? else {
            return Ok(suggestions);
        };

        // Skip if already linked
        if local_project.pm_project_id.is_some() {
            return Ok(suggestions);
        }

        for url in urls {
            // Try to extract issue ID from URL (e.g., HYGIE-38)
            if let Some(caps) = self.issue_id_pattern.captures(url) {
                let project_id = caps.get(1).map_or("", |m| m.as_str());
                let issue_num = caps.get(2).map_or("", |m| m.as_str());

                // Find matching PM project by identifier
                if let Some(pm_project) = pm_projects.iter().find(|p| {
                    p.identifier.eq_ignore_ascii_case(project_id)
                }) {
                    suggestions.push(LinkSuggestion {
                        local_project_id: current_project_id,
                        local_project_name: local_project.name.clone(),
                        pm_project_id: pm_project.id.to_string(),
                        pm_project_identifier: pm_project.identifier.clone(),
                        pm_project_name: pm_project.name.clone(),
                        confidence: 0.9, // High confidence - user is actively viewing this project
                        reason: LinkReason::IssuePageVisit(format!("{project_id}-{issue_num}")),
                    });
                    // Only need one suggestion per project
                    break;
                }
            }

            // Try to extract project from URL path
            if let Some(caps) = self.plane_url_pattern.captures(url) {
                if let Some(project_match) = caps.get(1) {
                    let project_ref = project_match.as_str();

                    // Try to find by UUID or identifier
                    if let Some(pm_project) = pm_projects.iter().find(|p| {
                        p.id.to_string() == project_ref
                            || p.identifier.eq_ignore_ascii_case(project_ref)
                    }) {
                        suggestions.push(LinkSuggestion {
                            local_project_id: current_project_id,
                            local_project_name: local_project.name.clone(),
                            pm_project_id: pm_project.id.to_string(),
                            pm_project_identifier: pm_project.identifier.clone(),
                            pm_project_name: pm_project.name.clone(),
                            confidence: 0.85,
                            reason: LinkReason::BrowserUrl(url.clone()),
                        });
                        break;
                    }
                }
            }
        }

        Ok(suggestions)
    }

    /// Suggest links based on project name matching
    ///
    /// Compares local project names against PM project names/identifiers
    /// using exact and fuzzy matching.
    ///
    /// # Errors
    ///
    /// Returns an error if database operations or Plane API calls fail.
    pub async fn suggest_from_name_matching(
        &self,
        plane_client: &PlaneClient,
    ) -> Result<Vec<LinkSuggestion>> {
        let mut suggestions = Vec::new();

        // Get all unlinked local projects
        let local_projects = self.database.get_all_projects()?;
        let unlinked: Vec<_> = local_projects
            .into_iter()
            .filter(|p| p.pm_project_id.is_none())
            .collect();

        if unlinked.is_empty() {
            return Ok(suggestions);
        }

        // Get PM projects
        let pm_projects = plane_client.list_projects().await?;

        for local in &unlinked {
            let local_name_lower = local.name.to_lowercase();

            // 1. Exact match on name or identifier
            if let Some(pm) = pm_projects.iter().find(|p| {
                p.name.to_lowercase() == local_name_lower
                    || p.identifier.to_lowercase() == local_name_lower
            }) {
                suggestions.push(LinkSuggestion {
                    local_project_id: local.id,
                    local_project_name: local.name.clone(),
                    pm_project_id: pm.id.to_string(),
                    pm_project_identifier: pm.identifier.clone(),
                    pm_project_name: pm.name.clone(),
                    confidence: 0.95,
                    reason: LinkReason::ExactNameMatch,
                });
                continue;
            }

            // 2. Fuzzy match - check if names contain each other
            for pm in &pm_projects {
                let pm_name_lower = pm.name.to_lowercase();
                let pm_id_lower = pm.identifier.to_lowercase();

                let similarity = Self::calculate_name_similarity(&local_name_lower, &pm_name_lower);

                if similarity > 0.6 {
                    suggestions.push(LinkSuggestion {
                        local_project_id: local.id,
                        local_project_name: local.name.clone(),
                        pm_project_id: pm.id.to_string(),
                        pm_project_identifier: pm.identifier.clone(),
                        pm_project_name: pm.name.clone(),
                        confidence: similarity * 0.8, // Scale down a bit
                        reason: LinkReason::FuzzyNameMatch(similarity),
                    });
                } else if local_name_lower.contains(&pm_id_lower)
                    || pm_id_lower.contains(&local_name_lower)
                {
                    suggestions.push(LinkSuggestion {
                        local_project_id: local.id,
                        local_project_name: local.name.clone(),
                        pm_project_id: pm.id.to_string(),
                        pm_project_identifier: pm.identifier.clone(),
                        pm_project_name: pm.name.clone(),
                        confidence: 0.7,
                        reason: LinkReason::FuzzyNameMatch(0.7),
                    });
                }
            }
        }

        // Sort by confidence descending
        suggestions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        // Deduplicate - keep highest confidence for each local project
        let mut seen: HashMap<Uuid, bool> = HashMap::new();
        suggestions.retain(|s| {
            if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(s.local_project_id) {
                e.insert(true);
                true
            } else {
                false
            }
        });

        Ok(suggestions)
    }

    /// Suggest links based on git remote URL
    ///
    /// Parses git remote URLs to find hints about the PM project.
    /// Works with common patterns like:
    /// - github.com/org/project-name
    /// - gitlab.com/org/project
    ///
    /// # Errors
    ///
    /// Returns an error if database operations fail.
    pub fn suggest_from_git_remote(
        &self,
        project_path: &str,
        pm_projects: &[PlaneProject],
    ) -> Result<Option<LinkSuggestion>> {
        // Try to read git remote
        let git_config_path = format!("{project_path}/.git/config");
        let Ok(config_content) = std::fs::read_to_string(&git_config_path) else {
            return Ok(None);
        };

        // Extract remote URL
        let Some(remote_url) = Self::extract_remote_url(&config_content) else {
            return Ok(None);
        };

        // Extract project name from URL
        let Some(project_name) = Self::extract_project_from_git_url(&remote_url) else {
            return Ok(None);
        };
        let project_name = project_name.to_lowercase();

        // Try to match with PM projects
        for pm in pm_projects {
            let pm_name_lower = pm.name.to_lowercase();
            let pm_id_lower = pm.identifier.to_lowercase();

            if project_name == pm_name_lower
                || project_name == pm_id_lower
                || project_name.contains(&pm_name_lower)
                || pm_name_lower.contains(&project_name)
            {
                // Get local project by path
                if let Ok(Some(local)) = self.database.get_project_by_path(project_path) {
                    return Ok(Some(LinkSuggestion {
                        local_project_id: local.id,
                        local_project_name: local.name.clone(),
                        pm_project_id: pm.id.to_string(),
                        pm_project_identifier: pm.identifier.clone(),
                        pm_project_name: pm.name.clone(),
                        confidence: 0.75,
                        reason: LinkReason::GitRemote(remote_url),
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Apply a link suggestion - actually link the projects
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    pub fn apply_suggestion(&self, suggestion: &LinkSuggestion, workspace_slug: &str) -> Result<()> {
        self.database.link_project_to_pm(
            suggestion.local_project_id,
            "plane",
            &suggestion.pm_project_id,
            Some(workspace_slug),
        )?;

        log::info!(
            "Auto-linked '{}' -> Plane project '{}' (confidence: {:.0}%, reason: {})",
            suggestion.local_project_name,
            suggestion.pm_project_identifier,
            suggestion.confidence * 100.0,
            suggestion.reason
        );

        Ok(())
    }

    /// Auto-link all projects above a confidence threshold
    ///
    /// # Errors
    ///
    /// Returns an error if API calls or database operations fail
    pub async fn auto_link_all(
        &self,
        plane_client: &PlaneClient,
        workspace_slug: &str,
        min_confidence: f32,
    ) -> Result<Vec<LinkSuggestion>> {
        let mut applied = Vec::new();

        // Get name-based suggestions
        let suggestions = self.suggest_from_name_matching(plane_client).await?;

        for suggestion in suggestions {
            if suggestion.confidence >= min_confidence {
                if let Err(e) = self.apply_suggestion(&suggestion, workspace_slug) {
                    log::warn!(
                        "Failed to auto-link '{}': {e}",
                        suggestion.local_project_name
                    );
                } else {
                    applied.push(suggestion);
                }
            }
        }

        Ok(applied)
    }

    /// Calculate similarity between two project names
    pub(crate) fn calculate_name_similarity(a: &str, b: &str) -> f32 {
        if a == b {
            return 1.0;
        }

        // Simple Jaccard similarity on character n-grams
        let a_grams: std::collections::HashSet<_> = a.chars().collect();
        let b_grams: std::collections::HashSet<_> = b.chars().collect();

        let intersection = a_grams.intersection(&b_grams).count();
        let union = a_grams.union(&b_grams).count();

        if union == 0 {
            0.0
        } else {
            // For project names (typically < 100 chars), these counts are small enough
            // that precision loss is not a practical concern
            #[allow(clippy::cast_precision_loss)]
            let result = intersection as f32 / union as f32;
            result
        }
    }

    /// Extract remote URL from git config content
    pub(crate) fn extract_remote_url(config: &str) -> Option<String> {
        let url_pattern = Regex::new(r"url\s*=\s*(.+)").ok()?;

        for line in config.lines() {
            if let Some(caps) = url_pattern.captures(line.trim()) {
                if let Some(url) = caps.get(1) {
                    return Some(url.as_str().trim().to_string());
                }
            }
        }
        None
    }

    /// Extract project name from git URL
    pub(crate) fn extract_project_from_git_url(url: &str) -> Option<String> {
        // Handle SSH format: git@github.com:org/project.git
        if url.contains('@') && url.contains(':') {
            let parts: Vec<&str> = url.split(':').collect();
            if parts.len() >= 2 {
                let path = parts[1];
                return path
                    .trim_end_matches(".git")
                    .split('/')
                    .next_back()
                    .map(String::from);
            }
        }

        // Handle HTTPS format: https://github.com/org/project.git
        url.trim_end_matches(".git")
            .split('/')
            .next_back()
            .map(String::from)
    }
}
