use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;
use tokio::fs;
use tokio::task;

#[derive(Debug, Deserialize)]
struct StorageData {
    #[serde(rename = "windowsState")]
    windows_state: Option<WindowsState>,
    // Fallback for other versions/configs
    #[serde(rename = "openedPathsList")]
    opened_paths_list: Option<OpenedPathsList>,
}

#[derive(Debug, Deserialize)]
struct WindowsState {
    #[serde(rename = "lastActiveWindow")]
    last_active_window: Option<WindowStateEntry>,
    #[serde(rename = "openedWindows")]
    opened_windows: Option<Vec<WindowStateEntry>>,
}

#[derive(Debug, Deserialize)]
struct WindowStateEntry {
    folder: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenedPathsList {
    entries: Vec<PathEntry>,
}

#[derive(Debug, Deserialize)]
struct PathEntry {
    #[serde(rename = "folderUri")]
    folder_uri: Option<String>,
}

/// Get candidate paths for VSCode/Cursor storage.json files.
fn get_candidate_storage_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if cfg!(target_os = "macos") {
        if let Some(home) = dirs::home_dir() {
            // Cursor (Priority)
            paths.push(
                home.join("Library/Application Support/Cursor/User/globalStorage/storage.json"),
            );
            // VSCode (Modern)
            paths.push(
                home.join("Library/Application Support/Code/User/globalStorage/storage.json"),
            );
            // VSCode (Legacy)
            paths.push(home.join("Library/Application Support/Code/storage.json"));
        }
    } else {
        // TODO: Add paths for Linux and Windows
    }
    paths
}

/// Extract project name from window title.
/// Cursor/VSCode window title formats:
/// - "filename.rs — toki — Cursor" (workspace name is second-to-last)
/// - "toki — Cursor" (just workspace name)
/// - "filename.rs - toki - Visual Studio Code"
/// - "filename.rs — toki" (no app name)
fn extract_project_from_title(title: &str) -> Option<String> {
    // Split by common separators: — (em dash), - (hyphen), –– (en dash)
    let parts: Vec<&str> = title
        .split(&['\u{2014}', '-', '\u{2013}'][..])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    // Identify app names to skip
    let app_names = ["cursor", "visual studio code", "code", "vscode"];

    match parts.len() {
        0 | 1 => None,
        2 => {
            // Could be "workspace - App" or "filename - workspace"
            let first = parts[0];
            let second = parts[1];

            // If second part is an app name, first is workspace
            if app_names.iter().any(|app| second.eq_ignore_ascii_case(app)) {
                return Some(first.to_string());
            }

            // If first part has a file extension, second is likely the workspace
            if first.contains('.')
                && first
                    .split('.')
                    .next_back()
                    .is_some_and(|ext| ext.len() <= 4)
            {
                return Some(second.to_string());
            }

            // Default: assume first is workspace
            Some(first.to_string())
        }
        _ => {
            // "file - workspace - App" or more parts
            // Find the workspace by skipping file (first with extension) and app (last if known)
            let last = parts[parts.len() - 1];
            let skip_last = app_names.iter().any(|app| last.eq_ignore_ascii_case(app));

            let end_idx = if skip_last {
                parts.len() - 1
            } else {
                parts.len()
            };

            // Return the last non-app part that's not a filename
            for i in (0..end_idx).rev() {
                let part = parts[i];
                // Skip parts that look like filenames (have common extensions)
                if part.contains('.') {
                    let ext = part.split('.').next_back().unwrap_or("");
                    if [
                        "rs", "ts", "js", "py", "go", "java", "cpp", "c", "h", "md", "json",
                        "toml", "yaml", "yml",
                    ]
                    .contains(&ext)
                    {
                        continue;
                    }
                }
                return Some(part.to_string());
            }

            // Fallback to second-to-last
            Some(parts[end_idx.saturating_sub(1)].to_string())
        }
    }
}

/// Get the most recently used workspace path from `VSCode` or Cursor.
///
/// # Errors
///
/// Returns an error if filesystem operations fail.
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::too_many_lines)]
pub async fn get_last_workspace(window_title: Option<&str>) -> Result<Option<PathBuf>> {
    let candidates = get_candidate_storage_paths();
    let mut valid_paths = Vec::new();

    // Check which files exist and get their modification time
    for path in candidates {
        if fs::try_exists(&path).await? {
            if let Ok(metadata) = fs::metadata(&path).await {
                if let Ok(modified) = metadata.modified() {
                    valid_paths.push((path, modified));
                }
            }
        }
    }

    // Sort by modification time, newest first
    valid_paths.sort_by(|a, b| b.1.cmp(&a.1));

    // Try to extract project name from window title
    let project_name = window_title.and_then(extract_project_from_title);
    log::debug!("Window title: {window_title:?}, extracted project: {project_name:?}");

    // Attempt to match project name if provided
    if let Some(ref project) = project_name {
        for (storage_path, _) in &valid_paths {
            if let Ok(content) = fs::read_to_string(storage_path).await {
                let content_clone = content.clone();
                let parse_result = task::spawn_blocking(move || {
                    serde_json::from_str::<StorageData>(&content_clone)
                })
                .await?;

                if let Ok(data) = parse_result {
                    // Collect all candidate URIs
                    let mut workspace_uris = Vec::new();

                    // 1. Windows State (Newer VSCode/Cursor)
                    if let Some(ws) = data.windows_state {
                        if let Some(last) = ws.last_active_window {
                            if let Some(folder) = last.folder {
                                workspace_uris.push(folder);
                            }
                        }
                        if let Some(opened) = ws.opened_windows {
                            for win in opened {
                                if let Some(folder) = win.folder {
                                    workspace_uris.push(folder);
                                }
                            }
                        }
                    }

                    // 2. Opened Paths List (Legacy)
                    if let Some(list) = data.opened_paths_list {
                        for entry in list.entries {
                            if let Some(uri) = entry.folder_uri {
                                workspace_uris.push(uri);
                            }
                        }
                    }

                    // Find exact match first, then fuzzy match
                    for uri in &workspace_uris {
                        let uri_clone = uri.clone();
                        let path_res =
                            task::spawn_blocking(move || PathBuf::from_url(&uri_clone)).await?;

                        if let Ok(path) = path_res {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                // Exact match (case-insensitive)
                                if name.eq_ignore_ascii_case(project) {
                                    log::debug!("Matched workspace: {}", path.display());
                                    return Ok(Some(path));
                                }
                            }
                        }
                    }

                    // Fuzzy match: project name contains or is contained by folder name
                    for uri in workspace_uris {
                        let uri_clone = uri.clone();
                        let path_res =
                            task::spawn_blocking(move || PathBuf::from_url(&uri_clone)).await?;

                        if let Ok(path) = path_res {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                let name_lower = name.to_lowercase();
                                let project_lower = project.to_lowercase();
                                if name_lower.contains(&project_lower)
                                    || project_lower.contains(&name_lower)
                                {
                                    log::debug!("Fuzzy matched workspace: {}", path.display());
                                    return Ok(Some(path));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: use lastActiveWindow from storage.json
    for (storage_path, _) in valid_paths {
        let content = fs::read_to_string(&storage_path).await?;
        let parse_result =
            task::spawn_blocking(move || serde_json::from_str::<StorageData>(&content)).await?;

        let Ok(data) = parse_result else {
            continue;
        };

        if let Some(ws) = data.windows_state {
            if let Some(last) = ws.last_active_window {
                if let Some(uri) = last.folder {
                    let uri_clone = uri.clone();
                    let path_res =
                        task::spawn_blocking(move || PathBuf::from_url(&uri_clone)).await?;
                    if let Ok(path) = path_res {
                        log::debug!("Fallback to lastActiveWindow: {}", path.display());
                        return Ok(Some(path));
                    }
                }
            }
        }

        if let Some(list) = data.opened_paths_list {
            if let Some(first_entry) = list.entries.first() {
                if let Some(uri) = &first_entry.folder_uri {
                    let uri_clone = uri.clone();
                    let path_res =
                        task::spawn_blocking(move || PathBuf::from_url(&uri_clone)).await?;
                    if let Ok(path) = path_res {
                        return Ok(Some(path));
                    }
                }
            }
        }
    }
    Ok(None)
}

// Helper trait to convert file URI to PathBuf
trait FromUrl {
    fn from_url(url: &str) -> Result<Self>
    where
        Self: Sized;
}

impl FromUrl for PathBuf {
    fn from_url(url: &str) -> Result<Self> {
        if let Ok(path) = url::Url::parse(url)?.to_file_path() {
            Ok(path)
        } else {
            Err(anyhow::anyhow!("Invalid file URI"))
        }
    }
}
