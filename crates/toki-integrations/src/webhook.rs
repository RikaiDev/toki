//! Plane.so Webhook handling
//!
//! This module provides types and utilities for handling Plane.so webhooks.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Plane.so webhook event types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaneEventType {
    /// Work item created
    #[serde(rename = "issue.created")]
    IssueCreated,
    /// Work item updated
    #[serde(rename = "issue.updated")]
    IssueUpdated,
    /// Work item deleted
    #[serde(rename = "issue.deleted")]
    IssueDeleted,
    /// Work item assigned
    #[serde(rename = "issue.activity.created")]
    IssueActivityCreated,
    /// Comment added
    #[serde(rename = "comment.created")]
    CommentCreated,
    /// Comment updated
    #[serde(rename = "comment.updated")]
    CommentUpdated,
    /// Cycle created
    #[serde(rename = "cycle.created")]
    CycleCreated,
    /// Module created
    #[serde(rename = "module.created")]
    ModuleCreated,
    /// Project created
    #[serde(rename = "project.created")]
    ProjectCreated,
    /// Unknown event type
    #[serde(other)]
    Unknown,
}

/// Plane.so webhook payload
#[derive(Debug, Clone, Deserialize)]
pub struct PlaneWebhookPayload {
    /// Event type
    pub event: PlaneEventType,
    /// Webhook action (e.g., "POST")
    #[serde(default)]
    pub action: Option<String>,
    /// Event data
    pub data: serde_json::Value,
    /// Workspace ID
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
    /// Project ID
    #[serde(default)]
    pub project_id: Option<Uuid>,
    /// Timestamp
    #[serde(default)]
    pub timestamp: Option<String>,
}

/// Work item data from webhook
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookWorkItem {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub sequence_id: i64,
    pub project: Uuid,
    #[serde(default)]
    pub state: Option<Uuid>,
    #[serde(default)]
    pub assignees: Vec<Uuid>,
    #[serde(default)]
    pub priority: Option<String>,
}

/// Webhook handler result
#[derive(Debug, Clone)]
pub struct WebhookResult {
    pub success: bool,
    pub message: String,
    pub work_item_id: Option<String>,
}

impl WebhookResult {
    /// Create a successful result
    #[must_use]
    pub fn success(message: impl Into<String>, work_item_id: Option<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            work_item_id,
        }
    }

    /// Create a failure result
    #[must_use]
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            work_item_id: None,
        }
    }
}

/// Verify webhook signature using HMAC-SHA256
///
/// # Arguments
/// * `payload` - Raw request body
/// * `signature` - Signature from `X-Plane-Signature` header
/// * `secret` - Webhook secret
///
/// # Returns
/// `true` if signature is valid
#[must_use]
pub fn verify_webhook_signature(payload: &[u8], signature: &str, secret: &str) -> bool {
    use std::fmt::Write;

    // Plane uses HMAC-SHA256
    let key = hmac_sha256::HMAC::mac(payload, secret.as_bytes());

    // Convert to hex string
    let mut computed_signature = String::with_capacity(64);
    for byte in key {
        let _ = write!(computed_signature, "{byte:02x}");
    }

    // Constant-time comparison to prevent timing attacks
    constant_time_compare(&computed_signature, signature)
}

/// Constant-time string comparison
fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

/// Process a webhook payload and extract relevant information
///
/// # Errors
///
/// Returns an error if the payload cannot be parsed
#[must_use]
pub fn process_webhook(payload: &PlaneWebhookPayload) -> WebhookResult {
    match payload.event {
        PlaneEventType::IssueCreated => {
            if let Ok(work_item) = serde_json::from_value::<WebhookWorkItem>(payload.data.clone()) {
                WebhookResult::success(
                    format!("Work item created: {}", work_item.name),
                    Some(format!("{}", work_item.id)),
                )
            } else {
                WebhookResult::failure("Failed to parse work item data")
            }
        }
        PlaneEventType::IssueUpdated => {
            if let Ok(work_item) = serde_json::from_value::<WebhookWorkItem>(payload.data.clone()) {
                WebhookResult::success(
                    format!("Work item updated: {}", work_item.name),
                    Some(format!("{}", work_item.id)),
                )
            } else {
                WebhookResult::failure("Failed to parse work item data")
            }
        }
        PlaneEventType::IssueDeleted => {
            WebhookResult::success("Work item deleted", None)
        }
        PlaneEventType::IssueActivityCreated => {
            WebhookResult::success("Activity created on work item", None)
        }
        _ => WebhookResult::success(format!("Received event: {:?}", payload.event), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_webhook_signature() {
        let payload = b"test payload";
        let secret = "test-secret";

        // Generate expected signature
        let key = hmac_sha256::HMAC::mac(payload, secret.as_bytes());
        let mut signature = String::with_capacity(64);
        for byte in key {
            use std::fmt::Write;
            let _ = write!(signature, "{byte:02x}");
        }

        assert!(verify_webhook_signature(payload, &signature, secret));
        assert!(!verify_webhook_signature(payload, "invalid", secret));
    }

    #[test]
    fn test_constant_time_compare() {
        assert!(constant_time_compare("abc", "abc"));
        assert!(!constant_time_compare("abc", "abd"));
        assert!(!constant_time_compare("abc", "ab"));
    }

    #[test]
    fn test_process_webhook_issue_created() {
        let payload = PlaneWebhookPayload {
            event: PlaneEventType::IssueCreated,
            action: Some("POST".to_string()),
            data: serde_json::json!({
                "id": "00000000-0000-0000-0000-000000000001",
                "name": "Test Issue",
                "sequence_id": 123,
                "project": "00000000-0000-0000-0000-000000000002"
            }),
            workspace_id: None,
            project_id: None,
            timestamp: None,
        };

        let result = process_webhook(&payload);
        assert!(result.success);
        assert!(result.message.contains("Test Issue"));
    }
}

