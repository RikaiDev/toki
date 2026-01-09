use anyhow::{Context, Result};
use git2::Repository;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::parser::{IssueId, IssueIdParser};

/// Git repository detector for extracting work item information
pub struct GitDetector {
    parser: IssueIdParser,
}

impl GitDetector {
    /// Create a new Git detector
    #[must_use]
    pub fn new() -> Self {
        Self {
            parser: IssueIdParser::new(),
        }
    }

    /// Find the Git repository containing the given path
    ///
    /// Searches upward from the given directory
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or no repository is found
    pub fn find_repo(&self, start_path: &Path) -> Result<Option<PathBuf>> {
        let mut current = start_path.to_path_buf();

        loop {
            let git_dir = current.join(".git");
            if git_dir.exists() {
                return Ok(Some(current));
            }

            if !current.pop() {
                // Reached root without finding .git
                return Ok(None);
            }
        }
    }

    /// Detect issue ID from Git branch name
    ///
    /// # Errors
    ///
    /// Returns an error if the repository cannot be opened or branch info cannot be read
    pub fn detect_from_branch(&self, repo_path: &Path) -> Result<Option<IssueId>> {
        let repo = Repository::open(repo_path)
            .with_context(|| format!("Failed to open Git repository at {}", repo_path.display()))?;

        let head = repo.head()?;

        if let Some(branch_name) = head.shorthand() {
            let ids = self.parser.parse(branch_name);
            if !ids.is_empty() {
                return Ok(Some(ids[0].clone()));
            }
        }

        Ok(None)
    }

    /// Detect issue ID from the most recent commit message
    ///
    /// # Errors
    ///
    /// Returns an error if the repository cannot be opened or commit info cannot be read
    pub fn detect_from_commit(&self, repo_path: &Path) -> Result<Option<IssueId>> {
        let repo = Repository::open(repo_path)
            .with_context(|| format!("Failed to open Git repository at {}", repo_path.display()))?;

        let head = repo.head()?;
        let commit = head.peel_to_commit()?;

        if let Some(message) = commit.message() {
            let ids = self.parser.parse(message);
            if !ids.is_empty() {
                return Ok(Some(ids[0].clone()));
            }
        }

        Ok(None)
    }

    /// Detect issue ID from Git, trying branch first then commit
    ///
    /// # Errors
    ///
    /// Returns an error if the repository cannot be accessed
    pub fn detect_from_git(&self, working_dir: &Path) -> Result<Option<IssueId>> {
        // First, find the repository
        let Some(repo_path) = self.find_repo(working_dir)? else {
            return Ok(None);
        };

        // Try branch name first
        if let Ok(Some(id)) = self.detect_from_branch(&repo_path) {
            log::debug!("Detected issue {} from Git branch", id.id);
            return Ok(Some(id));
        }

        // Fall back to commit message
        if let Ok(Some(id)) = self.detect_from_commit(&repo_path) {
            log::debug!("Detected issue {} from Git commit", id.id);
            return Ok(Some(id));
        }

        Ok(None)
    }

    /// Get the current branch name
    ///
    /// # Errors
    ///
    /// Returns an error if the repository cannot be opened
    pub fn get_branch_name(&self, repo_path: &Path) -> Result<Option<String>> {
        let repo = Repository::open(repo_path)
            .with_context(|| format!("Failed to open Git repository at {}", repo_path.display()))?;

        let head = repo.head()?;
        Ok(head.shorthand().map(String::from))
    }

    /// Get recent commit messages
    ///
    /// # Errors
    ///
    /// Returns an error if the repository cannot be opened
    pub fn get_recent_commits(&self, repo_path: &Path, count: usize) -> Result<Vec<String>> {
        let repo = Repository::open(repo_path)
            .with_context(|| format!("Failed to open Git repository at {}", repo_path.display()))?;

        let mut revwalk = repo.revwalk()?;
        revwalk.push_head()?;

        let mut messages = Vec::new();
        for oid in revwalk.take(count).filter_map(std::result::Result::ok) {
            if let Ok(commit) = repo.find_commit(oid) {
                if let Some(msg) = commit.message() {
                    // Take first line only
                    let first_line = msg.lines().next().unwrap_or("").to_string();
                    if !first_line.is_empty() {
                        messages.push(first_line);
                    }
                }
            }
        }

        Ok(messages)
    }

    /// Get list of changed files (staged + unstaged)
    ///
    /// # Errors
    ///
    /// Returns an error if the repository cannot be opened
    pub fn get_changed_files(&self, repo_path: &Path) -> Result<Vec<String>> {
        let repo = Repository::open(repo_path)
            .with_context(|| format!("Failed to open Git repository at {}", repo_path.display()))?;

        let mut files = Vec::new();

        // Get diff between HEAD and working directory
        if let Ok(head) = repo.head() {
            if let Ok(tree) = head.peel_to_tree() {
                if let Ok(diff) = repo.diff_tree_to_workdir_with_index(Some(&tree), None) {
                    for delta in diff.deltas() {
                        if let Some(path) = delta.new_file().path() {
                            files.push(path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }

        Ok(files)
    }
}

impl Default for GitDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Search for a Git repository starting from a path
///
/// # Errors
///
/// Returns an error if the path is invalid
pub fn find_git_repo(start_path: &Path) -> Result<Option<PathBuf>> {
    for entry in WalkDir::new(start_path)
        .max_depth(10)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if entry.file_name() == ".git" && entry.file_type().is_dir() {
            if let Some(parent) = entry.path().parent() {
                return Ok(Some(parent.to_path_buf()));
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_integration() {
        let detector = GitDetector::new();
        let ids = detector.parser.parse("feature/PROJ-123-awesome-feature");

        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].id, "PROJ-123");
    }
}
