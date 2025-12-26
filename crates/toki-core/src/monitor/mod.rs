use anyhow::Result;
use async_trait::async_trait;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

/// Application activity information
#[derive(Debug, Clone)]
pub struct AppActivity {
    pub app_id: String,
    pub app_name: String,
    pub window_title: Option<String>,
    pub is_active: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// System monitor trait for platform-specific implementations
#[async_trait]
pub trait SystemMonitor: Send + Sync {
    /// Start monitoring system activity
    async fn start_monitoring(&mut self) -> Result<()>;

    /// Get current active application
    async fn get_active_app(&self) -> Result<Option<AppActivity>>;

    /// Check if system is idle
    async fn is_idle(&self, threshold_seconds: u32) -> Result<bool>;

    /// Get current idle time in seconds
    async fn get_idle_seconds(&self) -> Result<u32>;

    /// Stop monitoring
    async fn stop_monitoring(&mut self) -> Result<()>;
}

/// Create platform-specific monitor
///
/// # Errors
///
/// Returns an error if the current platform is not supported or if monitor initialization fails
pub fn create_monitor() -> Result<Box<dyn SystemMonitor>> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacOSMonitor::new()?))
    }

    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(linux::LinuxMonitor::new()?))
    }

    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(windows::WindowsMonitor::new()?))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        anyhow::bail!("Unsupported platform")
    }
}
