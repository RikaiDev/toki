use anyhow::Result;
use lru::LruCache;
use std::fmt::Write;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::RwLock;
use toki_ai::AiService;
use toki_ai::ai_service::ClassificationResponse;

/// Snapshot of the current context for classification
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ContextSnapshot {
    pub app_id: String,
    pub window_title: Option<String>,
    pub git_branch: Option<String>,
    pub project_name: Option<String>,
}

impl ContextSnapshot {
    #[must_use] pub fn to_prompt_text(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "App: {}", self.app_id);
        if let Some(title) = &self.window_title {
            let _ = writeln!(text, "Window Title: {title}");
        }
        if let Some(branch) = &self.git_branch {
            let _ = writeln!(text, "Git Branch: {branch}");
        }
        if let Some(project) = &self.project_name {
            let _ = writeln!(text, "Project: {project}");
        }
        text
    }
}

const DEFAULT_CACHE_SIZE: NonZeroUsize = match NonZeroUsize::new(100) {
    Some(v) => v,
    None => unreachable!(),
};

/// Semantic classifier using AI with caching
pub struct AiClassifier {
    ai_service: Arc<AiService>,
    cache: RwLock<LruCache<ContextSnapshot, ClassificationResponse>>,
}

impl AiClassifier {
    #[must_use] pub fn new(ai_service: Arc<AiService>, cache_size: usize) -> Self {
        let cache_size = NonZeroUsize::new(cache_size).unwrap_or(DEFAULT_CACHE_SIZE);
        Self {
            ai_service,
            cache: RwLock::new(LruCache::new(cache_size)),
        }
    }

    /// Classify the context using AI, with caching
    ///
    /// # Errors
    ///
    /// Returns an error if the AI service fails to classify the context.
    pub async fn classify(&self, context: ContextSnapshot) -> Result<ClassificationResponse> {
        // 1. Check cache
        {
            let mut cache = self.cache.write().await;
            if let Some(cached) = cache.get(&context) {
                // Cloning here requires ClassificationResponse to be Clone, 
                // but it's defined in another crate. We'll reconstruct it or impl Clone there.
                // For now, let's assume we can clone the data out.
                return Ok(ClassificationResponse {
                    category: cached.category.clone(),
                    description: cached.description.clone(),
                    tags: cached.tags.clone(),
                });
            }
        }

        // 2. Cache miss - call AI
        let prompt_text = context.to_prompt_text();
        let result = self.ai_service.classify_context(&prompt_text).await?;

        // 3. Update cache
        {
            let mut cache = self.cache.write().await;
            // We need to store a copy, so we need to clone the result
             cache.put(context, ClassificationResponse {
                category: result.category.clone(),
                description: result.description.clone(),
                tags: result.tags.clone(),
            });
        }

        Ok(result)
    }
}
