/// Learn command handler - teach toki to classify activities (deprecated, use auto-inference)
use anyhow::Result;
use clap::Subcommand;
use toki_storage::{ClassificationRule, Database, PatternType};

use super::helpers::truncate_str;

#[derive(Subcommand, Debug)]
pub enum LearnAction {
    /// Add a classification rule
    Add {
        /// Pattern to match (domain, keyword, etc.)
        pattern: String,
        /// Category to assign (Break, Research, Coding, etc.)
        category: String,
        /// Pattern type: domain, `window_title`, `bundle_id`, `url_path`
        #[arg(short = 't', long, default_value = "window_title")]
        pattern_type: String,
    },
    /// List all learned rules
    List,
    /// Delete a learned rule by ID or pattern
    Delete {
        /// Rule ID or pattern to delete
        identifier: String,
    },
}

pub fn handle_learn_command(action: LearnAction) -> Result<()> {
    let db = Database::new(None)?;

    match action {
        LearnAction::Add {
            pattern,
            category,
            pattern_type,
        } => {
            let pt: PatternType = pattern_type
                .parse()
                .map_err(|e: String| anyhow::anyhow!(e))?;

            // Check if rule already exists
            if let Some(existing) = db.find_rule_by_pattern(&pattern, &pt)? {
                println!(
                    "Rule already exists: '{}' -> '{}' (hits: {})",
                    existing.pattern, existing.category, existing.hit_count
                );
                println!("Delete it first with: toki learn delete {}", existing.id);
                return Ok(());
            }

            let rule = ClassificationRule::from_correction(pattern.clone(), pt, category.clone());
            db.save_classification_rule(&rule)?;

            println!("Learned: '{pattern}' -> '{category}'");
            println!("This rule will be applied from now on.");
            println!("\nTo test, restart the daemon: toki stop && toki start");
        }

        LearnAction::List => {
            let rules = db.get_classification_rules()?;

            if rules.is_empty() {
                println!("No learned rules yet.");
                println!("\nTeach toki with:");
                println!("  toki learn add \"instagram\" Break --type domain");
                println!("  toki learn add \"Cake\" Research --type window_title");
                return Ok(());
            }

            println!("Learned classification rules:\n");
            println!(
                "{:<36} {:<20} {:<15} {:<12} HITS",
                "ID", "PATTERN", "TYPE", "CATEGORY"
            );
            println!("{}", "-".repeat(95));

            for rule in rules {
                let short_id = &rule.id.to_string()[..8];
                println!(
                    "{:<36} {:<20} {:<15} {:<12} {}",
                    short_id,
                    truncate_str(&rule.pattern, 18),
                    format!("{:?}", rule.pattern_type),
                    rule.category,
                    rule.hit_count
                );
            }

            println!("\nTo delete a rule: toki learn delete <ID or pattern>");
        }

        LearnAction::Delete { identifier } => {
            let rules = db.get_classification_rules()?;

            // Try to find by ID prefix or pattern
            let to_delete = rules.iter().find(|r| {
                r.id.to_string().starts_with(&identifier)
                    || r.pattern.to_lowercase() == identifier.to_lowercase()
            });

            if let Some(rule) = to_delete {
                db.delete_classification_rule(rule.id)?;
                println!("Deleted rule: '{}' -> '{}'", rule.pattern, rule.category);
            } else {
                println!("Rule not found: {identifier}");
                println!("Use 'toki learn list' to see all rules.");
            }
        }
    }

    Ok(())
}
