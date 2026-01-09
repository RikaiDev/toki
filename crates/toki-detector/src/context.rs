use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::git::GitDetector;
use crate::ide::vscode;
use crate::parser::{IssueId, IssueIdParser};
use strum_macros::Display;

/// Reference to a detected work item
#[derive(Debug, Clone)]
pub struct WorkItemRef {
    pub issue_id: IssueId,
    pub source: DetectionSource,
}

/// Source of work item detection
#[derive(Debug, Clone, PartialEq, Eq, Display)]
pub enum DetectionSource {
    GitBranch,     // From Git branch name
    GitCommit,     // From Git commit message
    FilePath,      // From file/directory path
    WorkspaceName, // From IDE workspace name (requires window title)
    IdeWorkspace,  // From IDE workspace path
}

/// Work context detector - integrates multiple detection strategies
pub struct WorkContextDetector {
    git_detector: GitDetector,
    parser: IssueIdParser,
}

impl WorkContextDetector {
    /// Create a new work context detector
    #[must_use]
    pub fn new() -> Self {
        Self {
            git_detector: GitDetector::new(),
            parser: IssueIdParser::new(),
        }
    }

    /// Detect work item from multiple sources
    ///
    /// Priority order:
    /// 1. Git branch name
    /// 2. Git commit message
    /// 3. File path patterns
    ///
    /// # Errors
    ///
    /// Returns an error if Git operations fail
    pub async fn detect(
        &self,
        working_dir: Option<&Path>,
        window_title: Option<&str>,
    ) -> Result<Option<WorkItemRef>> {
        // Store IDE workspace path locally to avoid memory leak from Box::leak
        let ide_workspace: Option<PathBuf> = if working_dir.is_none() {
            if let Ok(Some(ide_path)) = vscode::get_last_workspace(window_title).await {
                log::debug!("Got workspace from VSCode: {}", ide_path.display());
                Some(ide_path)
            } else {
                log::debug!("Could not get workspace from VSCode.");
                None
            }
        } else {
            None
        };

        // Use either the provided working_dir or the IDE workspace
        let from_ide = working_dir.is_none() && ide_workspace.is_some();
        let effective_dir: Option<&Path> = working_dir.or(ide_workspace.as_deref());

        // 1-2. Try Git detection (branch then commit)
        if let Some(dir) = effective_dir {
            log::debug!("Detecting from working directory: {}", dir.display());
            if let Ok(Some(issue_id)) = self.git_detector.detect_from_git(dir) {
                let source = if from_ide {
                    DetectionSource::IdeWorkspace
                } else {
                    DetectionSource::GitBranch
                };
                log::debug!("Detected work item {} from Git", issue_id.id);
                return Ok(Some(WorkItemRef { issue_id, source }));
            }
            log::debug!("No work item found from Git.");

            // 3. Try file path detection
            if let Some(issue_id) = self.parser.extract_from_path(dir) {
                log::debug!("Detected work item {} from path", issue_id.id);
                return Ok(Some(WorkItemRef {
                    issue_id,
                    source: DetectionSource::FilePath,
                }));
            }
            log::debug!("No work item found from path.");
        }

        log::debug!("No working directory, cannot detect.");
        Ok(None)
    }

    /// Detect from window title (for IDE workspace detection)
    ///
    /// Useful when privacy settings allow capturing window titles
    #[must_use]
    pub fn detect_from_window_title(&self, window_title: &str) -> Option<WorkItemRef> {
        let ids = self.parser.parse(window_title);
        ids.first().map(|id| WorkItemRef {
            issue_id: id.clone(),
            source: DetectionSource::WorkspaceName,
        })
    }

    /// Get workspace path from IDE (VSCode/Cursor)
    /// Returns the project directory, not the issue ID
    ///
    /// # Errors
    ///
    /// Returns an error if IDE detection fails
    pub async fn get_workspace_path(&self, window_title: Option<&str>) -> Result<Option<PathBuf>> {
        vscode::get_last_workspace(window_title).await
    }

    /// Detect work item from a specific path (e.g., for git branch detection)
    ///
    /// # Errors
    ///
    /// Returns an error if Git operations fail
    pub async fn detect_from_path(&self, path: &Path) -> Result<Option<WorkItemRef>> {
        // Try Git detection
        if let Ok(Some(issue_id)) = self.git_detector.detect_from_git(path) {
            log::debug!("Detected work item {} from Git at {:?}", issue_id.id, path);
            return Ok(Some(WorkItemRef {
                issue_id,
                source: DetectionSource::GitBranch,
            }));
        }

        // Try file path detection
        if let Some(issue_id) = self.parser.extract_from_path(path) {
            log::debug!("Detected work item {} from path {:?}", issue_id.id, path);
            return Ok(Some(WorkItemRef {
                issue_id,
                source: DetectionSource::FilePath,
            }));
        }

        Ok(None)
    }
}

impl Default for WorkContextDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_title_detection() {
        let detector = WorkContextDetector::new();
        let result = detector.detect_from_window_title("VSCode - PROJ-456 Implementation");

        assert!(result.is_some());
        let work_item = result.unwrap();
        assert_eq!(work_item.issue_id.id, "PROJ-456");
        assert_eq!(work_item.source, DetectionSource::WorkspaceName);
    }
}
