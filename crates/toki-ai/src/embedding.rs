use anyhow::{Result, Context};
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

/// Service for generating text embeddings and calculating similarity
pub struct EmbeddingService {
    model: TextEmbedding,
}

impl EmbeddingService {
    /// Create a new embedding service with Multilingual-E5-Small model
    /// This will download the model on first run (~100MB)
    /// Multilingual-E5-Small has much better Chinese support than `AllMiniLML6V2`
    ///
    /// # Errors
    ///
    /// Returns an error if the model fails to load or download.
    pub fn new() -> Result<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::MultilingualE5Small)
                .with_show_download_progress(true),
        )?;

        Ok(Self { model })
    }

    /// Generate embedding vector for a given text
    /// 
    /// # Errors
    /// Returns error if model inference fails
    pub fn generate_embedding(&mut self, text: &str) -> Result<Vec<f32>> {
        // fastembed supports batch processing, but we just need one
        let documents = vec![text];
        let embeddings = self.model.embed(documents, None)?;
        
        embeddings
            .into_iter()
            .next()
            .context("Failed to generate embedding")
    }

    /// Calculate Cosine Similarity between two vectors
    /// Returns a score between -1.0 and 1.0 (usually 0.0-1.0 for text)
    #[must_use] pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![1.0, 0.0, 0.0];
        let v3 = vec![0.0, 1.0, 0.0];
        
        assert!((EmbeddingService::cosine_similarity(&v1, &v2) - 1.0).abs() < f32::EPSILON);
        assert!((EmbeddingService::cosine_similarity(&v1, &v3)).abs() < f32::EPSILON);
    }
}
