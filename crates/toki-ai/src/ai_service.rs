use anyhow::{Context, Result};
use toki_storage::models::{AiConfig, Complexity, IssueCandidate};
use toki_storage::IssueTimeStats;

use crate::ai_provider::{create_provider, AiProviderTrait};

/// Unified AI Service
///
/// Handles interaction with configured AI provider and provides
/// domain-specific functions like time estimation and complexity analysis.
pub struct AiService {
    provider: Box<dyn AiProviderTrait>,
    config: AiConfig,
}

impl AiService {
    /// Create a new AI service from configuration
    pub fn new(config: AiConfig) -> Result<Self> {
        let provider = create_provider(&config)?;
        Ok(Self { provider, config })
    }

    /// Check if AI service is available/online
    pub async fn is_available(&self) -> bool {
        self.config.enabled && self.provider.is_available().await
    }

    /// Get the model name in use
    pub fn model_name(&self) -> &str {
        self.provider.model_name()
    }

    /// Estimate time for an issue using RAG (Retrieval Augmented Generation)
    ///
    /// # Arguments
    /// * `issue` - The issue to estimate
    /// * `similar_stats` - Historical stats of similar issues (for context)
    pub async fn estimate_time_rag(
        &self,
        issue: &IssueCandidate,
        similar_stats: &[IssueTimeStats],
    ) -> Result<u32> {
        let prompt = self.build_estimation_prompt(issue, similar_stats);
        let response = self.provider.generate(&prompt).await?;
        
        // Parse the response to extract seconds
        // Expected format: JSON or just a number, but LLMs are chatty.
        // We'll ask for JSON format in the prompt.
        self.parse_estimation_response(&response)
    }

    /// Analyze complexity of a task
    pub async fn analyze_complexity(&self, title: &str, description: &str) -> Result<Complexity> {
        let prompt = format!(
            "Analyze the complexity of the following software engineering task.\n\
             Task: {}\n\
             Description: {}\n\
             \n\
             Classify into one of: Trivial, Simple, Moderate, Complex, Epic.\n\
             Return ONLY the category name.",
            title, description
        );

        let response = self.provider.generate(&prompt).await?;
        let text = response.trim().to_lowercase();

        if text.contains("trivial") { Ok(Complexity::Trivial) }
        else if text.contains("simple") { Ok(Complexity::Simple) }
        else if text.contains("moderate") { Ok(Complexity::Moderate) }
        else if text.contains("complex") { Ok(Complexity::Complex) }
        else if text.contains("epic") { Ok(Complexity::Epic) }
        else {
            // Default fallback
            Ok(Complexity::Moderate)
        }
    }

    fn build_estimation_prompt(&self, issue: &IssueCandidate, similar_stats: &[IssueTimeStats]) -> String {
        let mut context = String::new();
        if !similar_stats.is_empty() {
            context.push_str("Here are some similar resolved tasks and their actual time spent:\n");
            for (i, stat) in similar_stats.iter().enumerate().take(3) {
                 // Note: We don't have titles for similar stats easily here unless passed, 
                 // but TimeEstimator usually fetches them.
                 // For now, let's just use the ID and time.
                 let hours = stat.total_seconds as f32 / 3600.0;
                 context.push_str(&format!("{}. {} took {:.1} hours\n", i+1, stat.issue_id, hours));
            }
            context.push_str("\n");
        }

        format!(
            "You are a senior software engineer project manager.\n\
             Estimate the time required to complete the following task in SECONDS.\n\
             \n\
             Task Title: {}\n\
             Description: {}\n\
             \n\
             {}\
             Consider complexity, testing, and potential blockers.\n\
             \n\
             Return a JSON object with this exact format:\n\
             {{\n  \"estimated_seconds\": 3600,\n  \"reasoning\": \"...\"\n }}\n\
             Do not include markdown formatting like ```json.",
            issue.title,
            issue.description.as_deref().unwrap_or("No description"),
            context
        )
    }

    fn parse_estimation_response(&self, response: &str) -> Result<u32> {
        // Clean up response if it contains markdown code blocks
        let clean = response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```");

        let json: serde_json::Value = serde_json::from_str(clean)
            .context(format!("Failed to parse JSON from AI response: {}", response))?;

        json["estimated_seconds"]
            .as_u64()
            .map(|s| s as u32)
            .context("JSON missing estimated_seconds field")
    }
}
