//! HTTP utilities for API integrations.

use anyhow::Result;

/// Extension trait for reqwest::Response to handle common error patterns.
#[async_trait::async_trait]
pub trait ResponseExt {
    /// Ensure the response status is successful, returning an error with details if not.
    ///
    /// # Errors
    ///
    /// Returns an error if the response status is not successful (2xx),
    /// including the status code and response body in the error message.
    async fn ensure_success(self, api_name: &str) -> Result<Self>
    where
        Self: Sized;
}

#[async_trait::async_trait]
impl ResponseExt for reqwest::Response {
    async fn ensure_success(self, api_name: &str) -> Result<Self> {
        if !self.status().is_success() {
            let status = self.status();
            let error_text = self.text().await.unwrap_or_default();
            anyhow::bail!("{api_name} API error ({status}): {error_text}");
        }
        Ok(self)
    }
}
