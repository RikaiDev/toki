use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::ai_provider::AiProviderTrait;

/// Google GenAI (Gemini) Provider
pub struct GoogleGenAiProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl GoogleGenAiProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }
}

#[async_trait]
impl AiProviderTrait for GoogleGenAiProvider {
    fn model_name(&self) -> &str {
        &self.model
    }

    async fn generate(&self, prompt: &str) -> Result<String> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let body = json!({
            "contents": [{
                "parts": [{
                    "text": prompt
                }]
            }]
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Google AI")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Google AI API error: {}", error_text);
        }

        let json: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse Google AI response")?;

        // Extract text from: candidates[0].content.parts[0].text
        json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .context("Failed to extract text from Google AI response")
    }

    async fn is_available(&self) -> bool {
        // Quick check if we can reach the API (using a minimal model list call or similar would be better,
        // but for now we just assume if we have a key it's "available" configuration-wise,
        // or we could try a tiny generation.)
        // For simplicity/performance, we assume true if initialized, 
        // fallback logic in service will handle actual failures.
        true
    }
}
