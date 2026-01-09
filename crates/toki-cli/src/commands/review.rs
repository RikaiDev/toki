/// Review command handler - AI-powered daily activity review
use anyhow::Result;
use chrono::{NaiveDate, TimeZone, Utc};
use std::sync::Arc;
use toki_ai::{ActivitySegment, ActivitySignals, SmartIssueMatcher, SuggestedIssue, TimeAnalyzer};
use toki_storage::{Database, TimeBlock};

/// Handle the review command - show daily activity summary with AI suggestions
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::too_many_lines)]
pub fn handle_review_command(
    date: Option<String>,
    verbose: bool,
    confirm_all: bool,
) -> Result<()> {
    let db = Arc::new(Database::new(None)?);

    // Parse date or use today
    let target_date = if let Some(date_str) = date {
        NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("Invalid date format. Use YYYY-MM-DD"))?
    } else {
        Utc::now().date_naive()
    };

    // Get time range for the date
    let start = target_date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid time"))?;
    let end = target_date
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| anyhow::anyhow!("Invalid time"))?;

    let start_utc = Utc.from_utc_datetime(&start);
    let end_utc = Utc.from_utc_datetime(&end);

    // Fetch activity spans for the day
    let spans = db.get_activity_spans(start_utc, end_utc)?;

    if spans.is_empty() {
        println!("No activity recorded for {target_date}");
        return Ok(());
    }

    // Convert ActivitySpan to ActivitySegment for AI analysis
    let segments: Vec<ActivitySegment> = spans
        .iter()
        .filter_map(|span| {
            let end_time = span.end_time?;
            Some(ActivitySegment {
                start_time: span.start_time,
                end_time,
                project_name: None, // Would need to look up project
                category: span.category.clone(),
                edited_files: span
                    .context
                    .as_ref()
                    .map(|c| c.edited_files.clone())
                    .unwrap_or_default(),
                git_commits: span
                    .context
                    .as_ref()
                    .map(|c| c.git_commits.clone())
                    .unwrap_or_default(),
                git_branch: span.context.as_ref().and_then(|c| c.git_branch.clone()),
                browser_urls: span
                    .context
                    .as_ref()
                    .map(|c| c.browser_urls.clone())
                    .unwrap_or_default(),
            })
        })
        .collect();

    // Analyze with AI
    let analyzer = TimeAnalyzer::new();
    let mut summary = analyzer.generate_daily_summary(target_date, &segments);

    // Compute Gravity/Relevance for unclassified or generic activities
    // This is the "Quiet Tech" magic: infer relevance without rules
    if let Ok(gravity_calc) = toki_ai::GravityCalculator::new(db.clone()) {
        // We need to compute gravity for each segment against its likely project
        // For simplicity in this phase, we'll just check against the most active project of the day
        let top_project_id = if let Ok(projects) = db
            .get_project_time_for_date(&target_date.format("%Y-%m-%d").to_string())
        {
            projects.first().map(|(p, _)| p.id) // Correctly extract project ID
        } else {
            None
        };

        if let Some(pid) = top_project_id {
            // Check unclassified segments
            // Note: Ideally we would update the summary structure to include relevance info
            // For now, we will just log it or print it during the review
            // In a full implementation, this would re-classify segments in the summary
            println!("\n(AI Context Gravity initialized. Project context: {pid})");

            for block in &mut summary.suggested_blocks {
                // Check relevance of the block description
                if let Ok(score) = gravity_calc.calculate_gravity(&block.suggested_description, pid)
                {
                    let status = toki_ai::RelevanceStatus::from_score(score);

                    // If score is low but it was classified as work, flag it
                    if status == toki_ai::RelevanceStatus::Break && block.confidence > 0.7 {
                        block.reasoning.push(format!(
                            "Warning: Low semantic relevance to project (Gravity: {score:.2})"
                        ));
                    } else if status == toki_ai::RelevanceStatus::Focus {
                        block
                            .reasoning
                            .push(format!("Confirmed high relevance (Gravity: {score:.2})"));
                        // Boost confidence
                        block.confidence = (block.confidence + 0.2).min(1.0);
                    }
                }
            }
        }
    }

    // Display header
    println!("\n{}", "=".repeat(60));
    println!("Daily Activity Review: {target_date}");
    println!("{}", "=".repeat(60));

    // Total time
    let total_hours = summary.total_active_seconds / 3600;
    let total_mins = (summary.total_active_seconds % 3600) / 60;
    println!("\nTotal active time: {total_hours}h {total_mins}m");

    // Classified vs unclassified
    let classified_pct = if summary.total_active_seconds > 0 {
        (f64::from(summary.classified_seconds) / f64::from(summary.total_active_seconds)) * 100.0
    } else {
        0.0
    };
    println!(
        "Classified: {}m ({:.0}%), Unclassified: {}m",
        summary.classified_seconds / 60,
        classified_pct,
        summary.unclassified_seconds / 60
    );

    // Project breakdown from project_time table (accurate multi-window tracking)
    let date_str = target_date.format("%Y-%m-%d").to_string();
    let project_times = db.get_project_time_for_date(&date_str)?;

    if !project_times.is_empty() {
        let total_project_secs: u32 = project_times.iter().map(|(_, s)| *s).sum();
        let total_h = total_project_secs / 3600;
        let total_m = (total_project_secs % 3600) / 60;
        println!("\nProject breakdown (total: {total_h}h {total_m}m):");
        for (project, seconds) in &project_times {
            let hours = seconds / 3600;
            let mins = (seconds % 3600) / 60;
            #[allow(clippy::cast_precision_loss)]
            let pct = if total_project_secs > 0 {
                (f64::from(*seconds) / f64::from(total_project_secs)) * 100.0
            } else {
                0.0
            };
            println!("  - {}: {}h {}m ({:.0}%)", project.name, hours, mins, pct);
        }
    } else if !summary.project_breakdown.is_empty() {
        // Fallback to AI-analyzed breakdown if no project_time data
        println!("\nProject breakdown:");
        let mut projects: Vec<_> = summary.project_breakdown.iter().collect();
        projects.sort_by(|a, b| b.1.cmp(a.1));
        for (project, seconds) in projects.iter().take(5) {
            let hours = *seconds / 3600;
            let mins = (*seconds % 3600) / 60;
            println!("  - {project}: {hours}h {mins}m");
        }
    }

    // AI Suggested time blocks - enhanced with SmartIssueMatcher
    if !summary.suggested_blocks.is_empty() {
        println!("\n{}", "-".repeat(60));
        println!("AI Suggested Time Blocks:");
        println!("{}", "-".repeat(60));

        // Try to initialize SmartIssueMatcher for AI-based issue matching
        let smart_matcher = SmartIssueMatcher::new(db.clone()).ok();

        // Get the top project for smart matching
        let date_str_for_matching = target_date.format("%Y-%m-%d").to_string();
        let top_project_for_matching = db
            .get_project_time_for_date(&date_str_for_matching)
            .ok()
            .and_then(|projects| projects.first().map(|(p, _)| p.clone()));

        for (i, block) in summary.suggested_blocks.iter_mut().enumerate() {
            let start = block.start_time.format("%H:%M");
            let end = block.end_time.format("%H:%M");
            let duration_mins = block.duration_seconds / 60;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let confidence_pct = (block.confidence.clamp(0.0, 1.0) * 100.0) as u32;

            println!(
                "\n{}. {} - {} ({}m) - {}% confidence",
                i + 1,
                start,
                end,
                duration_mins,
                confidence_pct
            );
            println!("   {}", block.suggested_description);

            // If no issues detected, try SmartIssueMatcher
            if block.suggested_issues.is_empty() {
                if let (Some(matcher), Some(project)) =
                    (&smart_matcher, &top_project_for_matching)
                {
                    // Check if project has issue candidates
                    if let Ok(candidates) = db.get_active_issue_candidates(project.id) {
                        if !candidates.is_empty() {
                            // Collect context from segments within this block's time range
                            let block_segments: Vec<_> = segments
                                .iter()
                                .filter(|s| {
                                    s.start_time >= block.start_time
                                        && s.end_time <= block.end_time
                                })
                                .collect();

                            // Build ActivitySignals - use block description as fallback context
                            let signals = ActivitySignals {
                                git_branch: block_segments
                                    .iter()
                                    .find_map(|s| s.git_branch.clone()),
                                recent_commits: block_segments
                                    .iter()
                                    .flat_map(|s| s.git_commits.clone())
                                    .take(5)
                                    .collect(),
                                edited_files: block_segments
                                    .iter()
                                    .flat_map(|s| s.edited_files.clone())
                                    .take(10)
                                    .collect(),
                                browser_urls: block_segments
                                    .iter()
                                    .flat_map(|s| s.browser_urls.clone())
                                    .take(5)
                                    .collect(),
                                // Use block description as window title context for semantic matching
                                window_titles: vec![block.suggested_description.clone()],
                            };

                            // Find matches using AI
                            if let Ok(matches) =
                                matcher.find_best_matches(&signals, project.id, 3)
                            {
                                for m in matches {
                                    block.suggested_issues.push(SuggestedIssue {
                                        issue_id: m.issue_id.clone(),
                                        confidence: m.confidence,
                                        reason: SmartIssueMatcher::format_reasons(&m.match_reasons),
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Display suggested issues
            if block.suggested_issues.is_empty() {
                println!("   No issue matches (general development time)");
                if top_project_for_matching
                    .as_ref()
                    .is_none_or(|p| p.pm_project_id.is_none())
                {
                    println!(
                        "   Tip: Link project to Plane.so with 'toki project link' then 'toki issue-sync'"
                    );
                }
            } else {
                println!("   AI Suggested Issues:");
                for issue in &block.suggested_issues {
                    let conf_level = if issue.confidence >= 0.8 {
                        "[HIGH]"
                    } else if issue.confidence >= 0.5 {
                        "[MED] "
                    } else {
                        "[LOW] "
                    };
                    println!(
                        "     {} {} - {:.0}% - {}",
                        conf_level,
                        issue.issue_id,
                        issue.confidence * 100.0,
                        issue.reason
                    );
                }
            }

            if verbose {
                println!("   Reasoning:");
                for reason in &block.reasoning {
                    println!("     - {reason}");
                }
            }
        }
    }

    // Save time blocks if --confirm-all is set
    if confirm_all && !summary.suggested_blocks.is_empty() {
        println!("\n{}", "-".repeat(60));
        println!("Saving {} time blocks...", summary.suggested_blocks.len());

        let mut saved_count = 0;
        for suggested in &summary.suggested_blocks {
            // Convert suggested issues to issue candidate UUIDs
            let work_item_ids: Vec<_> = suggested
                .suggested_issues
                .iter()
                .filter_map(|si| {
                    // Try to find the issue candidate by external ID (supports all systems)
                    db.get_issue_candidate_by_external_id(&si.issue_id)
                        .ok()
                        .flatten()
                        .map(|ic| ic.id)
                })
                .collect();

            // Use the ai_suggested constructor, then mark as confirmed
            let mut time_block = TimeBlock::ai_suggested(
                suggested.start_time,
                suggested.end_time,
                suggested.suggested_description.clone(),
                work_item_ids,
                suggested.confidence,
            );
            time_block.confirmed = true; // Mark as confirmed since user used --confirm-all

            if let Err(e) = db.save_time_block(&time_block) {
                log::error!("Failed to save time block: {e}");
            } else {
                saved_count += 1;
            }
        }

        println!("Saved {saved_count} confirmed time blocks.");
        println!("\nTo sync to Plane.so:");
        println!("  toki sync plane --reviewed");
    } else {
        println!("\n{}", "=".repeat(60));
        println!("To confirm and save all blocks:");
        println!("  toki review --confirm-all");
        println!("\nTo sync confirmed blocks to Plane.so:");
        println!("  toki sync plane --reviewed");
        println!("{}", "=".repeat(60));
    }

    Ok(())
}
