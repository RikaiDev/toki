/// Privacy settings command handlers
use anyhow::Result;
use toki_storage::Database;

/// Privacy action types
pub enum PrivacyActionType {
    Pause,
    Resume,
    ListExcluded,
    Exclude { app: String },
}

pub fn handle_privacy_command(action: Option<PrivacyActionType>) -> Result<()> {
    let db = Database::new(None)?;
    let mut settings = db.get_settings()?;

    match action {
        Some(PrivacyActionType::Pause) => {
            settings.pause_tracking = true;
            db.update_settings(&settings)?;
            println!("Tracking paused");
        }
        Some(PrivacyActionType::Resume) => {
            settings.pause_tracking = false;
            db.update_settings(&settings)?;
            println!("Tracking resumed");
        }
        Some(PrivacyActionType::ListExcluded) => {
            println!("Excluded applications:");
            for app in &settings.excluded_apps {
                println!("  - {app}");
            }
        }
        Some(PrivacyActionType::Exclude { app }) => {
            if settings.excluded_apps.contains(&app) {
                println!("'{app}' already in exclusion list");
            } else {
                settings.excluded_apps.push(app.clone());
                db.update_settings(&settings)?;
                println!("Added '{app}' to exclusion list");
            }
        }
        None => {
            println!("Privacy Settings");
            println!("\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
            println!(
                "Tracking: {}",
                if settings.pause_tracking {
                    "PAUSED"
                } else {
                    "ACTIVE"
                }
            );
            println!(
                "Idle threshold: {} seconds",
                settings.idle_threshold_seconds
            );
            println!("Excluded apps: {}", settings.excluded_apps.len());
        }
    }

    Ok(())
}
