use toki_storage::Settings;

/// Privacy filter for controlling what gets tracked
pub struct PrivacyFilter {
    settings: Settings,
}

impl PrivacyFilter {
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    /// Check if tracking is currently paused
    #[must_use]
    pub fn is_tracking_paused(&self) -> bool {
        self.settings.pause_tracking
    }

    /// Check if an application should be excluded from tracking
    #[must_use]
    pub fn should_exclude_app(&self, app_id: &str) -> bool {
        self.settings
            .excluded_apps
            .iter()
            .any(|excluded| app_id.contains(excluded) || excluded.contains(app_id))
    }

    /// Get idle threshold in seconds
    #[must_use]
    pub fn idle_threshold(&self) -> u32 {
        self.settings.idle_threshold_seconds
    }

    /// Update settings
    pub fn update_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }
}
