use anyhow::Result;
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use toki_storage::Database;
use uuid::Uuid;

/// Break state for smart idle detection
/// Distinguishes between quick breaks (water, bathroom) and actual session ends
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakState {
    /// User is actively working (idle < 2 min)
    Active,
    /// Short break - like getting water, bathroom (2-5 min)
    /// Don't interrupt session, just pause activity tracking
    ShortBreak,
    /// Long break - like lunch, meeting (5-30 min)
    /// Mark as break time in session, but don't end session
    LongBreak,
    /// Away - user has left for extended period (> 30 min)
    /// End the current session
    Away,
}

impl BreakState {
    /// Determine break state from idle seconds
    #[must_use]
    pub fn from_idle_seconds(idle_secs: u32) -> Self {
        match idle_secs {
            0..=119 => Self::Active,       // < 2 minutes
            120..=299 => Self::ShortBreak, // 2-5 minutes
            300..=1799 => Self::LongBreak, // 5-30 minutes
            _ => Self::Away,               // > 30 minutes
        }
    }

    /// Check if this state should pause activity tracking
    #[must_use]
    pub const fn should_pause_tracking(&self) -> bool {
        !matches!(self, Self::Active)
    }

    /// Check if this state should end the session
    #[must_use]
    pub const fn should_end_session(&self) -> bool {
        matches!(self, Self::Away)
    }

    /// Check if this is a countable break (for break time tracking)
    #[must_use]
    pub const fn is_break(&self) -> bool {
        matches!(self, Self::ShortBreak | Self::LongBreak)
    }

    /// Get human-readable description
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Active => "Working",
            Self::ShortBreak => "Short break",
            Self::LongBreak => "On break",
            Self::Away => "Away",
        }
    }
}

/// Manages session lifecycle based on work hours and idle detection
pub struct SessionManager {
    database: Arc<Database>,
    work_start_hour: u32,
    work_end_hour: u32,
    idle_threshold_mins: u32,
    /// Configurable thresholds for break states
    short_break_threshold_secs: u32,
    long_break_threshold_secs: u32,
    away_threshold_secs: u32,
}

impl SessionManager {
    /// Create a new session manager with default settings
    ///
    /// Default thresholds:
    /// - Short break: 2 minutes idle
    /// - Long break: 5 minutes idle
    /// - Away: 30 minutes idle
    #[must_use]
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            work_start_hour: 9,  // 09:00
            work_end_hour: 18,   // 18:00
            idle_threshold_mins: 15,
            short_break_threshold_secs: 120,  // 2 minutes
            long_break_threshold_secs: 300,   // 5 minutes
            away_threshold_secs: 1800,        // 30 minutes
        }
    }

    /// Create session manager with custom work hours
    #[must_use]
    pub fn with_work_hours(
        database: Arc<Database>,
        start_hour: u32,
        end_hour: u32,
        idle_threshold_mins: u32,
    ) -> Self {
        Self {
            database,
            work_start_hour: start_hour,
            work_end_hour: end_hour,
            idle_threshold_mins,
            short_break_threshold_secs: 120,
            long_break_threshold_secs: 300,
            away_threshold_secs: 1800,
        }
    }

    /// Configure break thresholds (in seconds)
    #[must_use]
    pub fn with_break_thresholds(
        mut self,
        short_break_secs: u32,
        long_break_secs: u32,
        away_secs: u32,
    ) -> Self {
        self.short_break_threshold_secs = short_break_secs;
        self.long_break_threshold_secs = long_break_secs;
        self.away_threshold_secs = away_secs;
        self
    }

    /// Determine the current break state based on idle time
    #[must_use]
    pub fn get_break_state(&self, idle_secs: u32) -> BreakState {
        if idle_secs < self.short_break_threshold_secs {
            BreakState::Active
        } else if idle_secs < self.long_break_threshold_secs {
            BreakState::ShortBreak
        } else if idle_secs < self.away_threshold_secs {
            BreakState::LongBreak
        } else {
            BreakState::Away
        }
    }

    /// Check if we should track activity based on break state
    #[must_use]
    pub fn should_track_activity(&self, idle_secs: u32) -> bool {
        let state = self.get_break_state(idle_secs);
        !state.should_pause_tracking()
    }

    /// Check if we should end session based on break state (smart version)
    #[must_use]
    pub fn should_end_session_smart(&self, idle_secs: u32, now: DateTime<Utc>) -> bool {
        let hour = now.hour();
        let outside_work_hours = hour < self.work_start_hour || hour >= self.work_end_hour;
        let break_state = self.get_break_state(idle_secs);
        
        outside_work_hours || break_state.should_end_session()
    }

    /// Check if a session should be started
    ///
    /// Session starts if:
    /// - Current time is within work hours
    /// - No ongoing session exists
    #[must_use]
    pub fn should_start_session(&self, now: DateTime<Utc>) -> bool {
        let hour = now.hour();
        hour >= self.work_start_hour && hour < self.work_end_hour
    }

    /// Check if a session should be ended
    ///
    /// Session ends if:
    /// - Current time is outside work hours, OR
    /// - System has been idle for longer than threshold
    ///
    /// # Arguments
    ///
    /// * `idle_secs` - Number of seconds the system has been idle
    /// * `now` - Current timestamp
    #[must_use]
    pub fn should_end_session(&self, idle_secs: u32, now: DateTime<Utc>) -> bool {
        let hour = now.hour();
        let outside_work_hours = hour < self.work_start_hour || hour >= self.work_end_hour;
        let exceeded_idle_threshold = idle_secs >= self.idle_threshold_mins * 60;

        outside_work_hours || exceeded_idle_threshold
    }

    /// Create a new session
    ///
    /// # Errors
    ///
    /// Returns an error if database operation fails
    pub fn create_session(&self) -> Result<Uuid> {
        let session_id = self.database.create_session(Utc::now())?;
        log::info!("Created new session: {session_id}");
        Ok(session_id)
    }

    /// Finalize the current session
    ///
    /// # Errors
    ///
    /// Returns an error if database operation fails
    pub fn finalize_session(&self, session_id: Uuid) -> Result<()> {
        self.database.finalize_session(session_id, Utc::now())?;
        log::info!("Finalized session: {session_id}");
        Ok(())
    }

    /// Get the current active session, if any
    ///
    /// # Errors
    ///
    /// Returns an error if database query fails
    pub fn get_current_session(&self) -> Result<Option<Uuid>> {
        Ok(self.database.get_current_session()?.map(|s| s.id))
    }

    /// Update session statistics
    ///
    /// # Errors
    ///
    /// Returns an error if database operation fails
    pub fn update_session_stats(
        &self,
        session_id: Uuid,
        active_secs: u32,
        idle_secs: u32,
        interruptions: u32,
    ) -> Result<()> {
        // To update stats, we first need to fetch the current state of categories and work_items
        let spans = self.database.get_activity_spans_by_session(session_id)?;

        let mut categories = std::collections::HashSet::new();
        let mut work_item_ids = std::collections::HashSet::new();

        for span in spans {
            categories.insert(span.category);
            if let Some(id) = span.work_item_id {
                work_item_ids.insert(id);
            }
        }

        let cat_vec: Vec<String> = categories.into_iter().collect();
        let work_vec: Vec<Uuid> = work_item_ids.into_iter().collect();

        self.database.update_session_stats(
            session_id,
            active_secs,
            idle_secs,
            interruptions,
            &cat_vec,
            &work_vec,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_should_start_session_during_work_hours() {
        let db = Arc::new(Database::new(None).unwrap());
        let manager = SessionManager::new(db);

        // 10:00 UTC - should start
        let time = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        assert!(manager.should_start_session(time));

        // 17:59 UTC - should start
        let time = Utc.with_ymd_and_hms(2024, 1, 1, 17, 59, 0).unwrap();
        assert!(manager.should_start_session(time));
    }

    #[test]
    fn test_should_not_start_session_outside_work_hours() {
        let db = Arc::new(Database::new(None).unwrap());
        let manager = SessionManager::new(db);

        // 08:00 UTC - before work hours
        let time = Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0).unwrap();
        assert!(!manager.should_start_session(time));

        // 18:00 UTC - after work hours
        let time = Utc.with_ymd_and_hms(2024, 1, 1, 18, 0, 0).unwrap();
        assert!(!manager.should_start_session(time));
    }

    #[test]
    fn test_should_end_session_when_idle() {
        let db = Arc::new(Database::new(None).unwrap());
        let manager = SessionManager::new(db);

        let time = Utc.with_ymd_and_hms(2024, 1, 1, 14, 0, 0).unwrap();

        // 10 minutes idle - should not end
        assert!(!manager.should_end_session(600, time));

        // 15 minutes idle - should end
        assert!(manager.should_end_session(900, time));

        // 20 minutes idle - should end
        assert!(manager.should_end_session(1200, time));
    }

    #[test]
    fn test_should_end_session_outside_work_hours() {
        let db = Arc::new(Database::new(None).unwrap());
        let manager = SessionManager::new(db);

        // Even with no idle time, should end outside work hours
        let time = Utc.with_ymd_and_hms(2024, 1, 1, 19, 0, 0).unwrap();
        assert!(manager.should_end_session(0, time));
    }
}
