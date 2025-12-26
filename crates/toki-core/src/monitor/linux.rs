use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::sync::{Arc, Mutex};

use super::{AppActivity, SystemMonitor};

pub struct LinuxMonitor {
    last_event_time: Arc<Mutex<std::time::Instant>>,
}

impl LinuxMonitor {
    /// Create a new Linux monitor
    ///
    /// # Errors
    ///
    /// Currently always succeeds, but returns `Result` for future compatibility
    pub fn new() -> Result<Self> {
        Ok(Self {
            last_event_time: Arc::new(Mutex::new(std::time::Instant::now())),
        })
    }
}

#[async_trait]
impl SystemMonitor for LinuxMonitor {
    async fn start_monitoring(&mut self) -> Result<()> {
        log::info!("Started Linux activity monitoring (placeholder)");
        Ok(())
    }

    async fn get_active_app(&self) -> Result<Option<AppActivity>> {
        // Placeholder implementation
        Ok(Some(AppActivity {
            app_id: String::from("linux.placeholder"),
            app_name: String::from("Placeholder"),
            window_title: None,
            is_active: true,
            timestamp: Utc::now(),
        }))
    }

    async fn is_idle(&self, threshold_seconds: u32) -> Result<bool> {
        let idle_secs = self.get_idle_seconds().await?;
        Ok(idle_secs > threshold_seconds)
    }

    async fn get_idle_seconds(&self) -> Result<u32> {
        // TODO: Implement proper idle detection using X11/Wayland APIs
        let last_event = *self.last_event_time.lock().unwrap();
        #[allow(clippy::cast_possible_truncation)]
        let elapsed = last_event.elapsed().as_secs() as u32;
        Ok(elapsed)
    }

    async fn stop_monitoring(&mut self) -> Result<()> {
        log::info!("Stopped Linux activity monitoring");
        Ok(())
    }
}
