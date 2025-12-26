use anyhow::Result;
use std::sync::Arc;
use toki_storage::{Category, ClassificationRule, Database};

/// Classification result with metadata
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    pub category: String,
    pub matched_rule_id: Option<uuid::Uuid>,
    pub source: ClassificationSource,
}

/// Where the classification came from
#[derive(Debug, Clone, PartialEq)]
pub enum ClassificationSource {
    /// Matched a user-defined rule (learned from corrections)
    UserRule,
    /// Matched a built-in regex pattern
    BuiltInPattern,
    /// No match, using default
    Default,
}

/// Classifier for categorizing applications based on rules
/// Priority: User rules > Built-in regex patterns > Default
pub struct Classifier {
    categories: Vec<Category>,
    user_rules: Vec<ClassificationRule>,
    database: Option<Arc<Database>>,
}

impl Classifier {
    /// Create a new classifier from database categories
    ///
    /// # Errors
    ///
    /// Returns an error if database query for categories fails
    pub fn from_database(db: &Database) -> Result<Self> {
        let categories = db.get_categories()?;
        let user_rules = db.get_classification_rules().unwrap_or_default();
        log::info!(
            "Loaded {} categories, {} user rules",
            categories.len(),
            user_rules.len()
        );
        Ok(Self {
            categories,
            user_rules,
            database: None,
        })
    }

    /// Create classifier with database reference for recording hits
    pub fn from_database_arc(db: Arc<Database>) -> Result<Self> {
        let categories = db.get_categories()?;
        let user_rules = db.get_classification_rules().unwrap_or_default();
        log::info!(
            "Loaded {} categories, {} user rules",
            categories.len(),
            user_rules.len()
        );
        Ok(Self {
            categories,
            user_rules,
            database: Some(db),
        })
    }

    /// Reload user rules from database
    pub fn reload_rules(&mut self) -> Result<()> {
        if let Some(db) = &self.database {
            self.user_rules = db.get_classification_rules()?;
            log::info!("Reloaded {} user rules", self.user_rules.len());
        }
        Ok(())
    }

    /// Classify an application based on its bundle ID
    #[must_use]
    pub fn classify(&self, app_id: &str) -> String {
        self.classify_with_context(app_id, None)
    }

    /// Classify with full result metadata
    #[must_use] pub fn classify_full(&self, app_id: &str, window_title: Option<&str>) -> ClassificationResult {
        // 1. Check user rules first (highest priority)
        for rule in &self.user_rules {
            if rule.matches(window_title, app_id) {
                log::debug!(
                    "Matched user rule '{}' -> '{}' (hits: {})",
                    rule.pattern,
                    rule.category,
                    rule.hit_count
                );

                // Record hit asynchronously
                if let Some(db) = &self.database {
                    if let Err(e) = db.record_rule_hit(rule.id) {
                        log::warn!("Failed to record rule hit: {e}");
                    }
                }

                return ClassificationResult {
                    category: rule.category.clone(),
                    matched_rule_id: Some(rule.id),
                    source: ClassificationSource::UserRule,
                };
            }
        }

        // 2. Check built-in patterns (window title first, then bundle ID)
        if let Some(title) = window_title {
            for category in &self.categories {
                if let Ok(re) = regex::Regex::new(&category.pattern) {
                    if re.is_match(title) {
                        log::debug!(
                            "Classified by window title '{}' as '{}'",
                            title,
                            category.name
                        );
                        return ClassificationResult {
                            category: category.name.clone(),
                            matched_rule_id: None,
                            source: ClassificationSource::BuiltInPattern,
                        };
                    }
                }
            }
        }

        // Check bundle ID
        for category in &self.categories {
            if let Ok(re) = regex::Regex::new(&category.pattern) {
                if re.is_match(app_id) {
                    log::debug!("Classified '{}' as '{}'", app_id, category.name);
                    return ClassificationResult {
                        category: category.name.clone(),
                        matched_rule_id: None,
                        source: ClassificationSource::BuiltInPattern,
                    };
                }
            }
        }

        // 3. Default category
        log::debug!("'{app_id}' not matched, using 'Uncategorized'");
        ClassificationResult {
            category: String::from("Uncategorized"),
            matched_rule_id: None,
            source: ClassificationSource::Default,
        }
    }

    /// Classify an application based on bundle ID and optional window title
    /// This allows detecting CLI tools running inside terminals
    #[must_use]
    pub fn classify_with_context(&self, app_id: &str, window_title: Option<&str>) -> String {
        self.classify_full(app_id, window_title).category
    }

    /// Add a user correction and create a new rule
    ///
    /// # Errors
    ///
    /// Returns an error if database operation fails
    pub fn add_correction(
        &mut self,
        pattern: String,
        pattern_type: toki_storage::PatternType,
        category: String,
    ) -> Result<ClassificationRule> {
        let rule = ClassificationRule::from_correction(pattern, pattern_type, category);

        if let Some(db) = &self.database {
            db.save_classification_rule(&rule)?;
        }

        self.user_rules.insert(0, rule.clone()); // Insert at front (highest priority)
        log::info!(
            "Added user rule: '{}' ({:?}) -> '{}'",
            rule.pattern,
            rule.pattern_type,
            rule.category
        );

        Ok(rule)
    }
}
