use std::io::Read;
use std::path::PathBuf;

/// Daemon control for process management
pub struct DaemonControl {
    pid_file: PathBuf,
}

impl DaemonControl {
    /// Create a new daemon control instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            pid_file: Self::default_pid_path(),
        }
    }

    /// Get default PID file path
    #[must_use]
    pub fn default_pid_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/tmp"));
        PathBuf::from(home).join(".toki/toki.pid")
    }

    /// Read PID from file
    fn read_pid(&self) -> anyhow::Result<u32> {
        let mut file = std::fs::File::open(&self.pid_file)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        contents.trim().parse::<u32>().map_err(Into::into)
    }

    /// Get PID of running daemon
    pub fn get_pid(&self) -> anyhow::Result<Option<u32>> {
        if !self.pid_file.exists() {
            return Ok(None);
        }
        Ok(Some(self.read_pid()?))
    }

    /// Remove PID file
    pub fn remove_pid(&self) -> anyhow::Result<()> {
        if self.pid_file.exists() {
            std::fs::remove_file(&self.pid_file)?;
        }
        Ok(())
    }
}

impl Default for DaemonControl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {}
