/// Configuration management command handlers
use anyhow::Result;
use toki_storage::{Database, IntegrationConfig};

pub fn handle_config_get(key: &str) -> Result<()> {
    let db = Database::new(None)?;
    let value = get_config_value(&db, key)?;
    match value {
        Some(v) => println!("{key} = {v}"),
        None => println!("{key} is not set"),
    }
    Ok(())
}

pub fn handle_config_set(key: &str, value: &str) -> Result<()> {
    let db = Database::new(None)?;
    set_config_value(&db, key, value)?;
    println!("Set {key} = {value}");
    Ok(())
}

pub fn handle_config_list() -> Result<()> {
    let db = Database::new(None)?;

    println!("Configuration:");
    println!("\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");

    // List Plane integration config
    if let Ok(Some(config)) = db.get_integration_config("plane") {
        println!("\n[plane]");
        println!("  api_url = {}", config.api_url);
        println!(
            "  api_key = {}***",
            &config.api_key.chars().take(8).collect::<String>()
        );
        if let Some(ws) = config.workspace_slug {
            println!("  workspace = {ws}");
        }
    }

    // List Notion integration config
    if let Ok(Some(config)) = db.get_integration_config("notion") {
        println!("\n[notion]");
        if !config.api_key.is_empty() {
            println!(
                "  api_key = {}***",
                &config.api_key.chars().take(8).collect::<String>()
            );
        }
        if let Some(db_id) = &config.project_id {
            println!("  database_id = {db_id}");
        }
        // time_property is stored in workspace_slug for Notion
        if let Some(tp) = &config.workspace_slug {
            println!("  time_property = {tp}");
        }
    }

    // List GitHub integration config
    if let Ok(Some(config)) = db.get_integration_config("github") {
        println!("\n[github]");
        if !config.api_key.is_empty() {
            println!(
                "  token = {}***",
                &config.api_key.chars().take(8).collect::<String>()
            );
        }
    }

    // List GitLab integration config
    if let Ok(Some(config)) = db.get_integration_config("gitlab") {
        println!("\n[gitlab]");
        if !config.api_key.is_empty() {
            println!(
                "  token = {}***",
                &config.api_key.chars().take(8).collect::<String>()
            );
        }
        if !config.api_url.is_empty() {
            println!("  api_url = {}", config.api_url);
        }
    }

    // List settings
    let settings = db.get_settings()?;
    println!("\n[settings]");
    println!(
        "  idle_threshold_seconds = {}",
        settings.idle_threshold_seconds
    );
    println!(
        "  work_item_tracking = {}",
        settings.enable_work_item_tracking
    );
    println!("  capture_window_title = {}", settings.capture_window_title);

    Ok(())
}

fn get_config_value(db: &Database, key: &str) -> Result<Option<String>> {
    let parts: Vec<&str> = key.split('.').collect();

    if parts.len() != 2 {
        anyhow::bail!("Invalid key format. Use: <section>.<key> (e.g., plane.api_key)");
    }

    let section = parts[0];
    let field = parts[1];

    match section {
        "plane" | "github" | "gitlab" | "jira" | "notion" => {
            if let Some(config) = db.get_integration_config(section)? {
                let value = match field {
                    "api_url" => Some(config.api_url),
                    "api_key" | "token" => Some(config.api_key),
                    "workspace" | "workspace_slug" => config.workspace_slug.clone(),
                    "project" | "project_id" | "database_id" => config.project_id.clone(),
                    // For Notion, time_property is stored in workspace_slug field
                    "time_property" if section == "notion" => config.workspace_slug.clone(),
                    _ => None,
                };
                Ok(value)
            } else {
                Ok(None)
            }
        }
        "settings" => {
            let settings = db.get_settings()?;
            let value = match field {
                "idle_threshold" | "idle_threshold_seconds" => {
                    Some(settings.idle_threshold_seconds.to_string())
                }
                "work_item_tracking" | "enable_work_item_tracking" => {
                    Some(settings.enable_work_item_tracking.to_string())
                }
                "capture_window_title" => Some(settings.capture_window_title.to_string()),
                _ => None,
            };
            Ok(value)
        }
        _ => anyhow::bail!(
            "Unknown section: {section}. Valid sections: plane, github, gitlab, jira, notion, settings"
        ),
    }
}

fn set_config_value(db: &Database, key: &str, value: &str) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();

    if parts.len() != 2 {
        anyhow::bail!("Invalid key format. Use: <section>.<key> (e.g., plane.api_key)");
    }

    let section = parts[0];
    let field = parts[1];

    match section {
        "plane" | "jira" => {
            let mut config = db.get_integration_config(section)?.unwrap_or_else(|| {
                IntegrationConfig::new(section.to_string(), String::new(), String::new())
            });

            match field {
                "api_url" => config.api_url = value.to_string(),
                "api_key" | "token" => config.api_key = value.to_string(),
                "workspace" | "workspace_slug" => config.workspace_slug = Some(value.to_string()),
                "project" | "project_id" => config.project_id = Some(value.to_string()),
                _ => anyhow::bail!(
                    "Unknown field: {field}. Valid fields: api_url, api_key, workspace, project"
                ),
            }

            config.updated_at = chrono::Utc::now();
            db.upsert_integration_config(&config)?;
        }
        "github" => {
            let mut config = db.get_integration_config(section)?.unwrap_or_else(|| {
                IntegrationConfig::new(section.to_string(), String::new(), String::new())
            });

            match field {
                "token" | "api_key" => config.api_key = value.to_string(),
                _ => anyhow::bail!(
                    "Unknown field: {field}. Valid fields: token"
                ),
            }

            config.updated_at = chrono::Utc::now();
            db.upsert_integration_config(&config)?;
        }
        "gitlab" => {
            let mut config = db.get_integration_config(section)?.unwrap_or_else(|| {
                IntegrationConfig::new(section.to_string(), String::new(), String::new())
            });

            match field {
                "token" | "api_key" => config.api_key = value.to_string(),
                "api_url" | "url" => config.api_url = value.to_string(),
                _ => anyhow::bail!(
                    "Unknown field: {field}. Valid fields: token, api_url"
                ),
            }

            config.updated_at = chrono::Utc::now();
            db.upsert_integration_config(&config)?;
        }
        "notion" => {
            let mut config = db.get_integration_config(section)?.unwrap_or_else(|| {
                IntegrationConfig::new(section.to_string(), String::new(), String::new())
            });

            match field {
                "api_key" => config.api_key = value.to_string(),
                "database_id" => config.project_id = Some(value.to_string()),
                // Store time_property in workspace_slug field (reused for Notion)
                "time_property" => config.workspace_slug = Some(value.to_string()),
                _ => anyhow::bail!(
                    "Unknown field: {field}. Valid fields: api_key, database_id, time_property"
                ),
            }

            config.updated_at = chrono::Utc::now();
            db.upsert_integration_config(&config)?;
        }
        "settings" => {
            let mut settings = db.get_settings()?;

            match field {
                "idle_threshold" | "idle_threshold_seconds" => {
                    settings.idle_threshold_seconds = value
                        .parse()
                        .map_err(|_| anyhow::anyhow!("Invalid number"))?;
                }
                "work_item_tracking" | "enable_work_item_tracking" => {
                    settings.enable_work_item_tracking = value == "true" || value == "1";
                }
                "capture_window_title" => {
                    settings.capture_window_title = value == "true" || value == "1";
                }
                _ => anyhow::bail!("Unknown field: {field}"),
            }

            db.update_settings(&settings)?;
        }
        _ => anyhow::bail!(
            "Unknown section: {section}. Valid sections: plane, github, gitlab, jira, notion, settings"
        ),
    }

    Ok(())
}
