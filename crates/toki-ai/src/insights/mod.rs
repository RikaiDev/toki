#[cfg(test)]
mod tests;

use std::collections::HashMap;
use toki_storage::{Activity, ActivitySpan};

/// Generate insights from activity data
pub struct InsightsGenerator;

impl InsightsGenerator {
    /// Calculate time spent per category from Activity records
    #[must_use]
    pub fn time_per_category(activities: &[Activity]) -> HashMap<String, u32> {
        let mut category_time: HashMap<String, u32> = HashMap::new();

        for activity in activities {
            *category_time.entry(activity.category.clone()).or_insert(0) +=
                activity.duration_seconds;
        }

        category_time
    }

    /// Calculate time spent per category from `ActivitySpan` records
    #[must_use]
    pub fn time_per_category_from_spans(spans: &[ActivitySpan]) -> HashMap<String, u32> {
        let mut category_time: HashMap<String, u32> = HashMap::new();

        for span in spans {
            *category_time.entry(span.category.clone()).or_insert(0) += span.duration_seconds;
        }

        category_time
    }

    /// Calculate total active time
    #[must_use]
    pub fn total_active_time(activities: &[Activity]) -> u32 {
        activities
            .iter()
            .filter(|a| a.is_active)
            .map(|a| a.duration_seconds)
            .sum()
    }

    /// Calculate total time from spans
    #[must_use]
    pub fn total_time_from_spans(spans: &[ActivitySpan]) -> u32 {
        spans.iter().map(|s| s.duration_seconds).sum()
    }

    /// Find most used applications
    #[must_use]
    pub fn top_applications(activities: &[Activity], limit: usize) -> Vec<(String, u32)> {
        let mut app_time: HashMap<String, u32> = HashMap::new();

        for activity in activities {
            *app_time.entry(activity.app_bundle_id.clone()).or_insert(0) +=
                activity.duration_seconds;
        }

        let mut sorted: Vec<_> = app_time.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);
        sorted
    }

    /// Find most used applications from spans
    #[must_use]
    pub fn top_applications_from_spans(spans: &[ActivitySpan], limit: usize) -> Vec<(String, u32)> {
        let mut app_time: HashMap<String, u32> = HashMap::new();

        for span in spans {
            *app_time.entry(span.app_bundle_id.clone()).or_insert(0) += span.duration_seconds;
        }

        let mut sorted: Vec<_> = app_time.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);
        sorted
    }
}
