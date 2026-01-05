use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::ai_provider::AiProviderTrait;

/// Anthropic Claude API Provider
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }
}

#[async_trait]
impl AiProviderTrait for AnthropicProvider {
    fn model_name(&self) -> &str {
        &self.model
    }

    async fn generate(&self, prompt: &str) -> Result<String> {
        let url = "https://api.anthropic.com/v1/messages";

        let body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": [{
                "role": "user",
                "content": prompt
            }]
        });

        let response = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Anthropic")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error: {}", error_text);
        }

        let json: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse Anthropic response")?;

        // Extract text from: content[0].text
        json["content"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .context("Failed to extract text from Anthropic response")
    }
}
