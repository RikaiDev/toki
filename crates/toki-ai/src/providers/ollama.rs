use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::ai_provider::AiProviderTrait;

/// Local Ollama Provider
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(base_url: Option<&str>, model: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url
                .unwrap_or("http://localhost:11434")
                .trim_end_matches('/')
                .to_string(),
            model: model.to_string(),
        }
    }
}

#[async_trait]
impl AiProviderTrait for OllamaProvider {
    fn model_name(&self) -> &str {
        &self.model
    }

    async fn generate(&self, prompt: &str) -> Result<String> {
        let url = format!("{}/api/chat", self.base_url);

        let body = json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": prompt
            }],
            "stream": false
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Ollama")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Ollama API error: {}", error_text);
        }

        let json: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse Ollama response")?;

        // Extract text from: message.content
        json["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .context("Failed to extract text from Ollama response")
    }

    async fn is_available(&self) -> bool {
        // Check if Ollama is running by hitting /api/tags or /
        let url = format!("{}/api/tags", self.base_url);
        self.client.get(&url).send().await.is_ok()
    }
}
