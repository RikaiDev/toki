use anyhow::Result;
use std::path::PathBuf;

/// Get the local data directory for toki.
///
/// # Errors
///
/// Returns an error if the local data directory cannot be determined.
pub fn get_data_dir() -> Result<PathBuf> {
    let mut path =
        dirs::data_local_dir().ok_or_else(|| anyhow::anyhow!("Failed to get local data dir"))?;
    path.push("toki");
    Ok(path)
}
