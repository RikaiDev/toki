//! AI Configuration database operations

use anyhow::Result;
use rusqlite::Connection;

use crate::models::{AiConfig, AiProvider};

/// Get AI configuration from database
pub fn get_ai_config(conn: &Connection) -> Result<AiConfig> {
    let result = conn.query_row(
        "SELECT provider, model, api_key, base_url, enabled FROM ai_config WHERE id = 1",
        [],
        |row| {
            let provider_str: String = row.get(0)?;
            let model: Option<String> = row.get(1)?;
            let api_key: Option<String> = row.get(2)?;
            let base_url: Option<String> = row.get(3)?;
            let enabled: bool = row.get(4)?;

            Ok((provider_str, model, api_key, base_url, enabled))
        },
    );

    match result {
        Ok((provider_str, model, api_key, base_url, enabled)) => {
            let provider = AiProvider::parse_provider(&provider_str).unwrap_or_default();
            Ok(AiConfig {
                provider,
                model,
                api_key,
                base_url,
                enabled,
            })
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            // Return default config if none exists
            Ok(AiConfig::default())
        }
        Err(e) => Err(e.into()),
    }
}

/// Save AI configuration to database
pub fn save_ai_config(conn: &Connection, config: &AiConfig) -> Result<()> {
    conn.execute(
        "INSERT INTO ai_config (id, provider, model, api_key, base_url, enabled, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
            provider = excluded.provider,
            model = excluded.model,
            api_key = excluded.api_key,
            base_url = excluded.base_url,
            enabled = excluded.enabled,
            updated_at = excluded.updated_at",
        rusqlite::params![
            config.provider.to_string(),
            config.model,
            config.api_key,
            config.base_url,
            config.enabled,
        ],
    )?;

    Ok(())
}

/// Update a specific AI config field
pub fn update_ai_config_field(conn: &Connection, key: &str, value: Option<&str>) -> Result<()> {
    // Ensure we have a row to update
    conn.execute(
        "INSERT OR IGNORE INTO ai_config (id, provider, enabled) VALUES (1, 'google', 1)",
        [],
    )?;

    match key {
        "provider" => {
            if let Some(v) = value {
                conn.execute(
                    "UPDATE ai_config SET provider = ?1, updated_at = datetime('now') WHERE id = 1",
                    [v],
                )?;
            }
        }
        "model" => {
            conn.execute(
                "UPDATE ai_config SET model = ?1, updated_at = datetime('now') WHERE id = 1",
                [value],
            )?;
        }
        "api_key" => {
            conn.execute(
                "UPDATE ai_config SET api_key = ?1, updated_at = datetime('now') WHERE id = 1",
                [value],
            )?;
        }
        "base_url" => {
            conn.execute(
                "UPDATE ai_config SET base_url = ?1, updated_at = datetime('now') WHERE id = 1",
                [value],
            )?;
        }
        "enabled" => {
            let enabled = value.is_some_and(|v| v == "true" || v == "1");
            conn.execute(
                "UPDATE ai_config SET enabled = ?1, updated_at = datetime('now') WHERE id = 1",
                [enabled],
            )?;
        }
        _ => {
            anyhow::bail!("Unknown AI config key: {key}");
        }
    }

    Ok(())
}
