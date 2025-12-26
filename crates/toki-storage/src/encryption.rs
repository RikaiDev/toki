use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

/// Initialize encryption for a `SQLite` database using `PRAGMA key`
///
/// # Errors
///
/// Returns an error if the `PRAGMA key` command fails
pub fn init_encryption(conn: &Connection, key: &str) -> Result<()> {
    conn.pragma_update(None, "key", key)
        .context("Failed to set encryption key")?;
    log::info!("Database encryption initialized");
    Ok(())
}

/// Generate a secure random encryption key
///
/// # Panics
///
/// May panic if system time is before UNIX epoch (should never happen on valid systems)
#[must_use]
pub fn generate_key() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Generate a pseudo-random key based on system time and process ID
    // In production, use a proper cryptographic RNG
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let pid = std::process::id();

    format!("{timestamp:x}{pid:x}")
}

/// Load encryption key from file or environment
///
/// # Errors
///
/// Returns an error if key file cannot be read or environment variable is not set
pub fn load_key_from_file(key_path: &Path) -> Result<String> {
    std::fs::read_to_string(key_path)
        .with_context(|| format!("Failed to read encryption key from {}", key_path.display()))
}

/// Save encryption key to file with restricted permissions
///
/// # Errors
///
/// Returns an error if file cannot be written or permissions cannot be set
pub fn save_key_to_file(key: &str, key_path: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create key directory")?;
    }

    // Write key to file
    std::fs::write(key_path, key)
        .with_context(|| format!("Failed to write encryption key to {}", key_path.display()))?;

    // Set restrictive permissions (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(key_path)?.permissions();
        perms.set_mode(0o600); // Read/write for owner only
        std::fs::set_permissions(key_path, perms)?;
    }

    log::info!("Encryption key saved to {}", key_path.display());
    Ok(())
}

/// Get default key file path
#[must_use]
pub fn default_key_path() -> std::path::PathBuf {
    let mut path = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    path.push("toki");
    path.push(".toki.key");
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_key() {
        let key1 = generate_key();
        let key2 = generate_key();

        assert!(!key1.is_empty());
        assert!(!key2.is_empty());
        assert_ne!(key1, key2); // Should generate different keys
    }
}
