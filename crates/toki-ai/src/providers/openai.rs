use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::ai_provider::AiProviderTrait;

/// OpenAI API Provider (also compatible with other OpenAI-compatible APIs)
pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAiProvider {
    pub fn new(api_key: &str, model: &str, base_url: Option<&str>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            base_url: base_url
                .unwrap_or("https://api.openai.com/v1")
                .trim_end_matches('/')
                .to_string(),
        }
    }
}

#[async_trait]
impl AiProviderTrait for OpenAiProvider {
    fn model_name(&self) -> &str {
        &self.model
    }

    async fn generate(&self, prompt: &str) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);

        let body = json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": prompt
            }]
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .context("Failed to send request to OpenAI")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error: {}", error_text);
        }

        let json: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse OpenAI response")?;

        // Extract text from: choices[0].message.content
        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .context("Failed to extract text from OpenAI response")
    }
}
