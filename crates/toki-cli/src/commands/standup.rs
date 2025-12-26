//! Standup report generation commands
//!
//! Generates standup reports from tracked activity.
//!
//! Usage:
//! ```bash
//! # Generate standup for today
//! toki standup
//!
//! # Custom format
//! toki standup --format slack
//! toki standup --format discord
//! toki standup --format markdown
//! toki standup --format json
//! ```

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use toki_ai::{StandupFormat, StandupGenerator};
use toki_storage::Database;

/// Generate and output a standup report
///
/// # Errors
///
/// Returns an error if database access or report generation fails
pub fn handle_standup_command(format: &str, date: Option<&str>) -> Result<()> {
    let db = Arc::new(Database::new(None).context("Failed to open database")?);
    let generator = StandupGenerator::new(db);

    // Parse optional date
    let parsed_date = if let Some(date_str) = date {
        Some(
            NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                .context("Invalid date format. Use YYYY-MM-DD")?,
        )
    } else {
        None
    };

    let report = generator.generate(parsed_date)?;
    let standup_format = StandupFormat::parse(format);

    println!("{}", report.format(standup_format));
    Ok(())
}
