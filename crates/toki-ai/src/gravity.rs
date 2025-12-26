use std::sync::{Arc, Mutex};
use anyhow::Result;
use toki_storage::Database;
use uuid::Uuid;
use crate::embedding::EmbeddingService;

/// Calculates semantic gravity (relevance) between activities and project context
pub struct GravityCalculator {
    embedding_service: Mutex<EmbeddingService>,
    database: Arc<Database>,
}

/// Relevance score classification
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RelevanceStatus {
    /// Highly relevant to current project (Focus)
    Focus,
    /// Somewhat relevant, potentially research or distraction (Drift)
    Drift,
    /// Not relevant (Break)
    Break,
}

impl RelevanceStatus {
    /// Convert score to status
    #[must_use] pub fn from_score(score: f32) -> Self {
        if score >= 0.6 {
            Self::Focus
        } else if score >= 0.3 {
            Self::Drift
        } else {
            Self::Break
        }
    }
}

impl GravityCalculator {
    pub fn new(database: Arc<Database>) -> Result<Self> {
        Ok(Self {
            embedding_service: Mutex::new(EmbeddingService::new()?),
            database,
        })
    }

    /// Calculate gravity score for a specific text against a project context
    /// Returns a score between 0.0 and 1.0
    pub fn calculate_gravity(&self, text: &str, project_id: Uuid) -> Result<f32> {
        // 1. Get project context vector (cached or compute)
        let context_vector = self.get_project_vector(project_id)?;
        
        if context_vector.is_empty() {
            // No context available yet, assume neutral relevance
            return Ok(0.5);
        }

        // 2. Compute vector for the text
        let text_vector = {
            let mut service = self.embedding_service.lock()
                .map_err(|e| anyhow::anyhow!("Failed to lock embedding service: {e}"))?;
            service.generate_embedding(text)?
        };

        // 3. Calculate similarity
        Ok(EmbeddingService::cosine_similarity(&context_vector, &text_vector))
    }

    /// Get or compute the project context vector
    fn get_project_vector(&self, project_id: Uuid) -> Result<Vec<f32>> {
        // 1. Try to fetch stored embedding from DB
        if let Ok(Some(embedding)) = self.database.get_project_embedding(project_id) {
            return Ok(embedding);
        }

        // 2. If not found, return empty for now
        // In a full implementation, we would:
        // - Fetch recent signals for the project (git commits, files, etc.)
        // - Generate embedding for the combined text
        // - Save it to DB
        
        // For now, let's assume we need to compute it manually or it doesn't exist
        Ok(Vec::new())
    }

    /// Calculate gravity for a window title against a set of context signals
    pub fn calculate_context_gravity(
        &self, 
        window_title: &str, 
        context_text: &str
    ) -> Result<f32> {
        if context_text.trim().is_empty() {
            return Ok(0.5);
        }

        let mut service = self.embedding_service.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock embedding service: {e}"))?;
        
        let context_vec = service.generate_embedding(context_text)?;
        let window_vec = service.generate_embedding(window_title)?;

        Ok(EmbeddingService::cosine_similarity(&context_vec, &window_vec))
    }
}
