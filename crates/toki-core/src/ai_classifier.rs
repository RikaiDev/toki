use anyhow::Result;
use lru::LruCache;
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
    pub fn to_prompt_text(&self) -> String {
        let mut text = String::new();
        text.push_str(&format!("App: {}\n", self.app_id));
        if let Some(title) = &self.window_title {
            text.push_str(&format!("Window Title: {}\n", title));
        }
        if let Some(branch) = &self.git_branch {
            text.push_str(&format!("Git Branch: {}\n", branch));
        }
        if let Some(project) = &self.project_name {
            text.push_str(&format!("Project: {}\n", project));
        }
        text
    }
}

/// Semantic classifier using AI with caching
pub struct AiClassifier {
    ai_service: Arc<AiService>,
    cache: RwLock<LruCache<ContextSnapshot, ClassificationResponse>>,
}

impl AiClassifier {
    pub fn new(ai_service: Arc<AiService>, cache_size: usize) -> Self {
        let cache_size = NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(100).unwrap());
        Self {
            ai_service,
            cache: RwLock::new(LruCache::new(cache_size)),
        }
    }

    /// Classify the context using AI, with caching
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
