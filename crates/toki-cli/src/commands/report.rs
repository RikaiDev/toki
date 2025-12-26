/// Report and categories command handlers
use anyhow::Result;
use chrono::{Duration, Utc};
use tabled::{Table, Tabled};
use toki_ai::InsightsGenerator;
use toki_storage::Database;

#[derive(Tabled)]
struct CategoryStats {
    #[tabled(rename = "Category")]
    category: String,
    #[tabled(rename = "Time (minutes)")]
    time_minutes: u32,
    #[tabled(rename = "Percentage")]
    percentage: String,
}

pub fn handle_report_command(period: String) -> Result<()> {
    let db = Database::new(None)?;

    let (start, end) = match period.as_str() {
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
        "month" => {
            let end = Utc::now();
            let start = end - Duration::days(30);
            (start, end)
        }
        _ => {
            println!("Unknown period: {period}. Use 'today', 'week', or 'month'");
            return Ok(());
        }
    };

    // Use activity_spans for more accurate data
    let spans = db.get_activity_spans(start, end)?;

    if spans.is_empty() {
        println!("No activities recorded for period: {period}");
        return Ok(());
    }

    let category_time = InsightsGenerator::time_per_category_from_spans(&spans);
    let total_time: u32 = category_time.values().sum();

    println!("\nTime Tracking Report: {period}");
    println!("\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");

    let mut stats: Vec<CategoryStats> = category_time
        .into_iter()
        .map(|(category, seconds)| {
            let minutes = seconds / 60;
            let percentage = if total_time > 0 {
                format!(
                    "{:.1}%",
                    (f64::from(seconds) / f64::from(total_time)) * 100.0
                )
            } else {
                String::from("0%")
            };
            CategoryStats {
                category,
                time_minutes: minutes,
                percentage,
            }
        })
        .collect();

    stats.sort_by(|a, b| b.time_minutes.cmp(&a.time_minutes));

    let table = Table::new(stats).to_string();
    println!("\n{table}");

    println!("\nTotal tracked time: {} minutes", total_time / 60);

    Ok(())
}

pub fn handle_categories_command() -> Result<()> {
    let db = Database::new(None)?;
    let categories = db.get_categories()?;

    println!("Category Rules");
    println!("\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    for category in categories {
        println!("\n{}", category.name);
        println!("  Pattern: {}", category.pattern);
        if let Some(desc) = category.description {
            println!("  Description: {desc}");
        }
    }

    Ok(())
}
