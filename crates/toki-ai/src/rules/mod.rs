#[cfg(test)]
mod tests;

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Rule for categorizing applications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub pattern: String,
    pub category: String,
    pub confidence: f32,
}

/// Rule-based classification engine
pub struct RuleEngine {
    rules: Vec<Rule>,
    compiled_patterns: HashMap<String, Regex>,
}

impl RuleEngine {
    /// Create a new rule engine
    #[must_use]
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            compiled_patterns: HashMap::new(),
        }
    }

    /// Add a rule to the engine
    ///
    /// # Errors
    ///
    /// Returns an error if the regex pattern compilation fails
    pub fn add_rule(&mut self, rule: Rule) -> Result<()> {
        let regex = Regex::new(&rule.pattern)?;
        self.compiled_patterns.insert(rule.pattern.clone(), regex);
        self.rules.push(rule);
        Ok(())
    }

    /// Load default rules
    ///
    /// # Errors
    ///
    /// Returns an error if any default rule regex pattern fails to compile
    pub fn load_default_rules(&mut self) -> Result<()> {
        let default_rules = vec![
            Rule {
                pattern: String::from("(?i)(vscode|cursor|intellij|pycharm)"),
                category: String::from("Coding"),
                confidence: 0.95,
            },
            Rule {
                pattern: String::from("(?i)(terminal|iterm)"),
                category: String::from("Terminal"),
                confidence: 0.90,
            },
            Rule {
                pattern: String::from("(?i)(chrome|firefox|safari)"),
                category: String::from("Browser"),
                confidence: 0.85,
            },
        ];

        for rule in default_rules {
            self.add_rule(rule)?;
        }

        log::info!("Loaded {} default rules", self.rules.len());
        Ok(())
    }

    /// Classify an application
    #[must_use]
    pub fn classify(&self, app_id: &str) -> Option<(String, f32)> {
        for rule in &self.rules {
            if let Some(regex) = self.compiled_patterns.get(&rule.pattern) {
                if regex.is_match(app_id) {
                    return Some((rule.category.clone(), rule.confidence));
                }
            }
        }
        None
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}
