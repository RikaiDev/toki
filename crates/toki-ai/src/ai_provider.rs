use anyhow::{Result, Context};
use async_trait::async_trait;
use toki_storage::models::{AiConfig, AiProvider};

use crate::providers::{
    anthropic::AnthropicProvider, google::GoogleGenAiProvider, ollama::OllamaProvider,
    openai::OpenAiProvider,
};

/// Trait for AI providers
#[async_trait]
pub trait AiProviderTrait: Send + Sync {
    /// Generate text response for a given prompt
    async fn generate(&self, prompt: &str) -> Result<String>;

    /// Get the model name being used
    fn model_name(&self) -> &str;

    /// Check if the provider is available
    async fn is_available(&self) -> bool {
        true
    }
}

/// Create a provider instance based on configuration
///
/// # Errors
///
/// Returns an error if the API key is missing for providers that require one
/// (Google, `OpenAI`, Anthropic).
pub fn create_provider(config: &AiConfig) -> Result<Box<dyn AiProviderTrait>> {
    let model = config.effective_model();
    let api_key = config.effective_api_key();
    let base_url = config.effective_base_url();

    match config.provider {
        AiProvider::Google => {
            let api_key = api_key.context("API Key required for Google GenAI")?;
            Ok(Box::new(GoogleGenAiProvider::new(
                &api_key,
                model,
            )))
        }
        AiProvider::OpenAi => {
            let api_key = api_key.context("API Key required for OpenAI")?;
            Ok(Box::new(OpenAiProvider::new(
                &api_key,
                model,
                Some(base_url),
            )))
        }
        AiProvider::Anthropic => {
            let api_key = api_key.context("API Key required for Anthropic")?;
            Ok(Box::new(AnthropicProvider::new(
                &api_key,
                model,
            )))
        }
        AiProvider::Ollama => {
            Ok(Box::new(OllamaProvider::new(
                Some(base_url),
                model,
            )))
        }
    }
}
