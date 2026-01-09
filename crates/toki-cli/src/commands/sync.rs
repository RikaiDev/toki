/// Time sync command handler
use anyhow::Result;
use toki_integrations::{GitLabClient, PlaneClient, ProjectManagementSystem, TimeEntry};
use toki_storage::Database;

#[allow(clippy::cognitive_complexity)]
#[allow(clippy::too_many_lines)]
pub async fn handle_sync_command(system: String, dry_run: bool, reviewed: bool) -> Result<()> {
    let db = Database::new(None)?;

    let config = db
        .get_integration_config(&system)?
        .ok_or_else(|| anyhow::anyhow!("No configuration found for system: {system}"))?;

    println!("Synchronizing with {}...", config.system_type);

    if dry_run {
        println!("  (Dry run mode - no actual changes will be made)");
    }
    if reviewed {
        println!("  (Syncing only reviewed/confirmed time blocks)");
    }

    let sync_result = match config.system_type.as_str() {
        "plane" => {
            let workspace_slug = config.workspace_slug.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "Workspace not configured. Run: toki config set plane.workspace <slug>"
                )
            })?;

            let client = PlaneClient::new(
                config.api_key.clone(),
                workspace_slug,
                Some(config.api_url.clone()),
            )?;

            let mut time_entries = Vec::new();

            if reviewed {
                // Only sync confirmed time blocks
                let time_blocks = db.get_confirmed_time_blocks()?;

                for block in time_blocks {
                    // Get the first associated issue candidate
                    if let Some(issue_id) = block.work_item_ids.first() {
                        if let Some(issue) = db.get_issue_candidate_by_id(*issue_id)? {
                            // Only sync issues from the target system (plane)
                            if issue.external_system == "plane" {
                                let duration = u32::try_from(
                                    (block.end_time - block.start_time).num_seconds().max(0),
                                )
                                .unwrap_or(u32::MAX);
                                time_entries.push(TimeEntry {
                                    work_item_id: issue.external_id.clone(),
                                    start_time: block.start_time,
                                    duration_seconds: duration,
                                    description: block.description.clone(),
                                    category: block
                                        .tags
                                        .first()
                                        .cloned()
                                        .unwrap_or_else(|| "Development".to_string()),
                                });
                            }
                        }
                    }
                }
            } else {
                // Original behavior - sync all work items with activities
                let work_items = db.get_all_work_items()?;

                for work_item in work_items {
                    let activities = db.get_activities_by_work_item(work_item.id)?;

                    if activities.is_empty() {
                        continue;
                    }

                    for activity in &activities {
                        time_entries.push(TimeEntry {
                            work_item_id: work_item.external_id.clone(),
                            start_time: activity.timestamp,
                            duration_seconds: activity.duration_seconds,
                            description: format!("Auto-tracked by Toki: {}", activity.category),
                            category: activity.category.clone(),
                        });
                    }
                }
            }

            if time_entries.is_empty() {
                if reviewed {
                    println!("No confirmed time blocks to sync.");
                    println!("Run 'toki review' to review and confirm time blocks first.");
                } else {
                    println!("No time entries to sync.");
                }
                return Ok(());
            }

            println!("Found {} time entries to sync", time_entries.len());

            if dry_run {
                use toki_integrations::SyncReport;
                Ok(SyncReport::new(0))
            } else {
                client.batch_sync(time_entries).await
            }
        }
        "notion" => {
            use toki_integrations::{NotionClient, SyncReport};

            let client = NotionClient::new(config.api_key.clone())?;

            // Set time property if configured
            if let Some(time_prop) = &config.workspace_slug {
                client.set_time_property(Some(time_prop.clone())).await;
            }

            // Populate page ID cache from database
            let page_id_map = db.get_notion_page_id_map()?;
            for (external_id, page_id) in &page_id_map {
                client.cache_page_id(external_id, page_id).await;
            }
            log::debug!("Loaded {} page ID mappings from database", page_id_map.len());

            let mut time_entries = Vec::new();

            if reviewed {
                // Only sync confirmed time blocks
                let time_blocks = db.get_confirmed_time_blocks()?;

                for block in time_blocks {
                    // Get the first associated issue candidate
                    if let Some(issue_id) = block.work_item_ids.first() {
                        if let Some(issue) = db.get_issue_candidate_by_id(*issue_id)? {
                            // Only sync Notion issues with source_page_id
                            if issue.external_system == "notion" && issue.source_page_id.is_some() {
                                let duration = u32::try_from(
                                    (block.end_time - block.start_time).num_seconds().max(0),
                                )
                                .unwrap_or(u32::MAX);
                                time_entries.push(TimeEntry {
                                    work_item_id: issue.external_id.clone(),
                                    start_time: block.start_time,
                                    duration_seconds: duration,
                                    description: block.description.clone(),
                                    category: block
                                        .tags
                                        .first()
                                        .cloned()
                                        .unwrap_or_else(|| "Development".to_string()),
                                });
                            }
                        }
                    }
                }
            } else {
                // Sync all work items with activities
                let work_items = db.get_all_work_items()?;

                for work_item in work_items {
                    // Only sync notion work items
                    if work_item.external_system != "notion" {
                        continue;
                    }

                    let activities = db.get_activities_by_work_item(work_item.id)?;

                    if activities.is_empty() {
                        continue;
                    }

                    for activity in &activities {
                        time_entries.push(TimeEntry {
                            work_item_id: work_item.external_id.clone(),
                            start_time: activity.timestamp,
                            duration_seconds: activity.duration_seconds,
                            description: format!("Auto-tracked by Toki: {}", activity.category),
                            category: activity.category.clone(),
                        });
                    }
                }
            }

            if time_entries.is_empty() {
                if reviewed {
                    println!("No confirmed time blocks to sync.");
                    println!("Run 'toki review' to review and confirm time blocks first.");
                } else {
                    println!("No Notion time entries to sync.");
                }
                return Ok(());
            }

            println!("Found {} time entries to sync to Notion", time_entries.len());

            if dry_run {
                for entry in &time_entries {
                    println!("  {} - {}s", entry.work_item_id, entry.duration_seconds);
                }
                Ok(SyncReport::new(0))
            } else {
                client.batch_sync(time_entries).await
            }
        }
        "gitlab" => {
            use toki_integrations::SyncReport;

            let project = config.project_id.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "GitLab project not configured. Run: toki config set gitlab.project <project>"
                )
            })?;

            let client = if config.api_url.is_empty() {
                GitLabClient::new(&config.api_key, &project)?
            } else {
                GitLabClient::with_base_url(&config.api_key, &project, &config.api_url)?
            };

            let mut time_entries = Vec::new();

            if reviewed {
                // Only sync confirmed time blocks
                let time_blocks = db.get_confirmed_time_blocks()?;

                for block in time_blocks {
                    // Get the first associated issue candidate
                    if let Some(issue_id) = block.work_item_ids.first() {
                        if let Some(issue) = db.get_issue_candidate_by_id(*issue_id)? {
                            // Only sync GitLab issues
                            if issue.external_system == "gitlab" {
                                let duration = u32::try_from(
                                    (block.end_time - block.start_time).num_seconds().max(0),
                                )
                                .unwrap_or(u32::MAX);
                                time_entries.push(TimeEntry {
                                    work_item_id: issue.external_id.clone(),
                                    start_time: block.start_time,
                                    duration_seconds: duration,
                                    description: block.description.clone(),
                                    category: block
                                        .tags
                                        .first()
                                        .cloned()
                                        .unwrap_or_else(|| "Development".to_string()),
                                });
                            }
                        }
                    }
                }
            } else {
                // Sync all work items with activities
                let work_items = db.get_all_work_items()?;

                for work_item in work_items {
                    // Only sync gitlab work items
                    if work_item.external_system != "gitlab" {
                        continue;
                    }

                    let activities = db.get_activities_by_work_item(work_item.id)?;

                    if activities.is_empty() {
                        continue;
                    }

                    for activity in &activities {
                        time_entries.push(TimeEntry {
                            work_item_id: work_item.external_id.clone(),
                            start_time: activity.timestamp,
                            duration_seconds: activity.duration_seconds,
                            description: format!("Auto-tracked by Toki: {}", activity.category),
                            category: activity.category.clone(),
                        });
                    }
                }
            }

            if time_entries.is_empty() {
                if reviewed {
                    println!("No confirmed time blocks to sync to GitLab.");
                    println!("Run 'toki review' to review and confirm time blocks first.");
                } else {
                    println!("No GitLab time entries to sync.");
                }
                return Ok(());
            }

            println!("Found {} time entries to sync to GitLab", time_entries.len());

            if dry_run {
                for entry in &time_entries {
                    let duration = format_duration(entry.duration_seconds);
                    println!(
                        "  Issue #{} - {} - {}",
                        entry.work_item_id, duration, entry.description
                    );
                }
                Ok(SyncReport::new(0))
            } else {
                client.batch_sync(time_entries).await
            }
        }
        _ => {
            anyhow::bail!("Unsupported PM system: {}", config.system_type);
        }
    }?;

    if !dry_run {
        println!("Sync complete!");
        println!("  Success: {}", sync_result.successful);
        println!("  Failed: {}", sync_result.failed);
        if !sync_result.errors.is_empty() {
            println!("\nErrors:");
            for error in sync_result.errors {
                println!("  - {error}");
            }
        }
    }

    Ok(())
}

/// Format duration in human-readable form (e.g., "1h 30m")
fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    match (hours, minutes) {
        (0, 0) => "0m".to_string(),
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}
