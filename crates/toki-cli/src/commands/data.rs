/// Data management command handlers (export, delete)
use anyhow::Result;
use chrono::{Duration, Utc};
use std::fmt::Write;
use toki_storage::Database;

use super::helpers::escape_csv;

pub fn handle_data_export(format: &str, output: Option<String>) -> Result<()> {
    let db = Database::new(None)?;
    let end = Utc::now();
    let start = end - Duration::days(365);

    let output_path = output.unwrap_or_else(|| format!("toki_export.{format}"));

    match format {
        "json" => {
            let activities = db.get_activities(start, end)?;
            let json = serde_json::to_string_pretty(&activities)?;
            std::fs::write(&output_path, json)?;
            println!("Exported {} activities to {output_path}", activities.len());
        }
        "csv" => {
            // Export activity spans (more detailed data)
            let spans = db.get_activity_spans(start, end)?;
            let mut csv_content = String::from(
                "id,app_bundle_id,category,start_time,end_time,duration_seconds,work_item_id,session_id\n",
            );

            for span in &spans {
                let _ = writeln!(
                    csv_content,
                    "{},{},{},{},{},{},{},{}",
                    span.id,
                    escape_csv(&span.app_bundle_id),
                    escape_csv(&span.category),
                    span.start_time.to_rfc3339(),
                    span.end_time.map(|t| t.to_rfc3339()).unwrap_or_default(),
                    span.duration_seconds,
                    span.work_item_id
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                    span.session_id.map(|id| id.to_string()).unwrap_or_default(),
                );
            }

            std::fs::write(&output_path, csv_content)?;
            println!("Exported {} activity spans to {output_path}", spans.len());
        }
        _ => {
            println!("Unknown format: {format}. Use 'json' or 'csv'");
        }
    }

    Ok(())
}

pub fn handle_data_delete(period: &str) -> Result<()> {
    let db = Database::new(None)?;

    let (start, end) = match period {
        "today" => {
            let start = Utc::now()
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc();
            let end = Utc::now();
            (start, end)
        }
        "week" => {
            let end = Utc::now();
            let start = end - Duration::days(7);
            (start, end)
        }
        "all" => {
            let end = Utc::now();
            let start = Utc::now() - Duration::days(3650); // 10 years
            (start, end)
        }
        _ => {
            println!("Unknown period: {period}. Use 'today', 'week', or 'all'");
            return Ok(());
        }
    };

    let deleted = db.delete_activities(start, end)?;
    println!("Deleted {deleted} activities");

    Ok(())
}
