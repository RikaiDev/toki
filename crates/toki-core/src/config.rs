use anyhow::Result;
use std::path::PathBuf;

pub fn get_data_dir() -> Result<PathBuf> {
    let mut path =
        dirs::data_local_dir().ok_or_else(|| anyhow::anyhow!("Failed to get local data dir"))?;
    path.push("toki");
    Ok(path)
}
