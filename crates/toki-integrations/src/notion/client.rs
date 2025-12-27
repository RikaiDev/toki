//! Notion API client implementation
//!
//! This module provides the HTTP client for interacting with the Notion API.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use super::schema::{PropertyMapping, PropertyMappingConfig, NOTION_API_VERSION, NOTION_BASE_URL, RATE_LIMIT_INTERVAL_MS, TIME_CONVENTIONS};
use super::types::{
    NotionBlock, NotionDatabase, NotionIssueCandidateData, NotionPage,
    NotionPaginatedResponse, NotionPropertyUpdate, NotionPropertyValue,
    PropertyUpdateValue, RichTextInput, SearchResponse, SelectInput, TextContent,
    UpdatePageRequest,
};

// ============================================================================
// Rate Limiter
// ============================================================================

/// Simple rate limiter for Notion API
pub(crate) struct RateLimiter {
    last_request: Mutex<Instant>,
    interval: Duration,
}

impl RateLimiter {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            last_request: Mutex::new(
                Instant::now()
                    .checked_sub(Duration::from_millis(interval_ms))
                    .unwrap_or_else(Instant::now),
            ),
            interval: Duration::from_millis(interval_ms),
        }
    }

    pub async fn wait(&self) {
        let mut last = self.last_request.lock().await;
        let elapsed = last.elapsed();
        if elapsed < self.interval {
            tokio::time::sleep(self.interval - elapsed).await;
        }
        *last = Instant::now();
    }
}

// ============================================================================
// Notion Client
// ============================================================================

/// Notion API client
pub struct NotionClient {
    api_key: String,
    client: reqwest::Client,
    rate_limiter: Arc<RateLimiter>,
    /// Mapping from `external_id` (short ID) to full page ID
    page_id_cache: Arc<Mutex<HashMap<String, String>>>,
    /// Configured time property name (overrides auto-detection)
    time_property_override: Arc<Mutex<Option<String>>>,
}

impl NotionClient {
    /// Create a new Notion client
    ///
    /// # Arguments
    /// * `api_key` - Notion integration token (starts with `secret_`)
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created
    pub fn new(api_key: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            api_key,
            client,
            rate_limiter: Arc::new(RateLimiter::new(RATE_LIMIT_INTERVAL_MS)),
            page_id_cache: Arc::new(Mutex::new(HashMap::new())),
            time_property_override: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a new Notion client with a configured time property
    ///
    /// # Arguments
    /// * `api_key` - Notion integration token
    /// * `time_property` - Name of the property to use for time tracking
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created
    pub fn with_time_property(api_key: String, time_property: String) -> Result<Self> {
        let mut client = Self::new(api_key)?;
        client.time_property_override = Arc::new(Mutex::new(Some(time_property)));
        Ok(client)
    }

    /// Set the time property name for time tracking
    pub async fn set_time_property(&self, property_name: Option<String>) {
        let mut guard = self.time_property_override.lock().await;
        *guard = property_name;
    }

    /// Get the configured time property name
    pub async fn get_time_property(&self) -> Option<String> {
        let guard = self.time_property_override.lock().await;
        guard.clone()
    }

    /// Make an authenticated GET request with rate limiting
    pub(crate) async fn get<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        self.rate_limiter.wait().await;

        log::debug!("GET {url}");

        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .send()
            .await
            .context("Failed to send request to Notion API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Notion API error ({status}): {body}");
        }

        response
            .json()
            .await
            .context("Failed to parse Notion API response")
    }

    /// Make an authenticated POST request with rate limiting
    pub(crate) async fn post<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        self.rate_limiter.wait().await;

        log::debug!("POST {url}");

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("Failed to send request to Notion API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Notion API error ({status}): {body}");
        }

        response
            .json()
            .await
            .context("Failed to parse Notion API response")
    }

    /// Make an authenticated PATCH request with rate limiting
    pub(crate) async fn patch<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        self.rate_limiter.wait().await;

        log::debug!("PATCH {url}");

        let response = self
            .client
            .patch(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("Failed to send request to Notion API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Notion API error ({status}): {body}");
        }

        response
            .json()
            .await
            .context("Failed to parse Notion API response")
    }

    // ========================================================================
    // Database APIs
    // ========================================================================

    /// Get a database by ID
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or database not found
    pub async fn get_database(&self, database_id: &str) -> Result<NotionDatabase> {
        let clean_id = Self::clean_id(database_id);
        let url = format!("{NOTION_BASE_URL}/databases/{clean_id}");
        self.get(&url).await
    }

    /// Query pages in a database
    ///
    /// # Arguments
    /// * `database_id` - Database UUID
    /// * `cursor` - Optional pagination cursor
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn query_database(
        &self,
        database_id: &str,
        cursor: Option<&str>,
    ) -> Result<NotionPaginatedResponse<NotionPage>> {
        let clean_id = Self::clean_id(database_id);
        let url = format!("{NOTION_BASE_URL}/databases/{clean_id}/query");

        let mut body = serde_json::json!({});
        if let Some(c) = cursor {
            body["start_cursor"] = serde_json::Value::String(c.to_string());
        }

        self.post(&url, &body).await
    }

    /// Query all pages in a database (handles pagination automatically)
    ///
    /// # Errors
    ///
    /// Returns an error if any API request fails
    pub async fn query_database_all(&self, database_id: &str) -> Result<Vec<NotionPage>> {
        let mut all_pages = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let response = self
                .query_database(database_id, cursor.as_deref())
                .await?;
            all_pages.extend(response.results);

            if response.has_more {
                cursor = response.next_cursor;
            } else {
                break;
            }
        }

        Ok(all_pages)
    }

    /// List databases the integration has access to (handles pagination)
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn list_databases(&self) -> Result<Vec<NotionDatabase>> {
        let url = format!("{NOTION_BASE_URL}/search");
        let mut all_databases = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut body = serde_json::json!({
                "filter": {
                    "value": "database",
                    "property": "object"
                }
            });

            if let Some(ref c) = cursor {
                body["start_cursor"] = serde_json::Value::String(c.clone());
            }

            let response: SearchResponse = self.post(&url, &body).await?;
            all_databases.extend(response.results);

            if response.has_more {
                cursor = response.next_cursor;
            } else {
                break;
            }
        }

        Ok(all_databases)
    }

    // ========================================================================
    // Page APIs
    // ========================================================================

    /// Get a page by ID
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or page not found
    pub async fn get_page(&self, page_id: &str) -> Result<NotionPage> {
        let clean_id = Self::clean_id(page_id);
        let url = format!("{NOTION_BASE_URL}/pages/{clean_id}");
        self.get(&url).await
    }

    /// Get blocks (content) of a page
    ///
    /// # Arguments
    /// * `page_id` - Page UUID
    /// * `cursor` - Optional pagination cursor
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn get_page_blocks(
        &self,
        page_id: &str,
        cursor: Option<&str>,
    ) -> Result<NotionPaginatedResponse<NotionBlock>> {
        let clean_id = Self::clean_id(page_id);
        let mut url = format!("{NOTION_BASE_URL}/blocks/{clean_id}/children");
        if let Some(c) = cursor {
            url = format!("{url}?start_cursor={c}");
        }
        self.get(&url).await
    }

    /// Get all blocks from a page (handles pagination)
    ///
    /// # Arguments
    /// * `page_id` - Page UUID
    /// * `max_depth` - Maximum recursion depth for nested blocks (default: 2)
    ///
    /// # Errors
    ///
    /// Returns an error if any API request fails
    pub async fn get_page_blocks_all(
        &self,
        page_id: &str,
        max_depth: usize,
    ) -> Result<Vec<NotionBlock>> {
        self.get_blocks_recursive(page_id, 0, max_depth).await
    }

    /// Recursively get blocks with depth limit
    #[async_recursion::async_recursion]
    async fn get_blocks_recursive(
        &self,
        block_id: &str,
        current_depth: usize,
        max_depth: usize,
    ) -> Result<Vec<NotionBlock>> {
        if current_depth >= max_depth {
            return Ok(Vec::new());
        }

        let mut all_blocks = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let response = self.get_page_blocks(block_id, cursor.as_deref()).await?;

            for block in response.results {
                let has_children = block.has_children;
                let block_id_clone = block.id.clone();
                all_blocks.push(block);

                // Recursively fetch children if needed
                if has_children && current_depth + 1 < max_depth {
                    let children = self
                        .get_blocks_recursive(&block_id_clone, current_depth + 1, max_depth)
                        .await?;
                    all_blocks.extend(children);
                }
            }

            if response.has_more {
                cursor = response.next_cursor;
            } else {
                break;
            }
        }

        Ok(all_blocks)
    }

    /// Update a page property
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails
    pub async fn update_page_property(
        &self,
        page_id: &str,
        property_name: &str,
        value: NotionPropertyUpdate,
    ) -> Result<NotionPage> {
        let clean_id = Self::clean_id(page_id);
        let url = format!("{NOTION_BASE_URL}/pages/{clean_id}");

        let update_value = match value {
            NotionPropertyUpdate::Number(n) => PropertyUpdateValue::Number { number: n },
            NotionPropertyUpdate::Text(s) => PropertyUpdateValue::RichText {
                rich_text: vec![RichTextInput {
                    text: TextContent { content: s },
                }],
            },
            NotionPropertyUpdate::Title(s) => PropertyUpdateValue::Title {
                title: vec![RichTextInput {
                    text: TextContent { content: s },
                }],
            },
            NotionPropertyUpdate::Checkbox(b) => PropertyUpdateValue::Checkbox { checkbox: b },
            NotionPropertyUpdate::Select(s) => PropertyUpdateValue::Select {
                select: SelectInput { name: s },
            },
            NotionPropertyUpdate::Status(s) => PropertyUpdateValue::Status {
                status: SelectInput { name: s },
            },
        };

        let mut properties = HashMap::new();
        properties.insert(property_name.to_string(), update_value);

        let request = UpdatePageRequest { properties };
        self.patch(&url, &request).await
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Clean a Notion ID (remove dashes if present, ensure proper format)
    pub(crate) fn clean_id(id: &str) -> String {
        // Notion IDs can be with or without dashes
        // The API accepts both formats, but we normalize for consistency
        id.replace('-', "")
    }

    /// Generate external ID from database and page IDs
    ///
    /// Format: `{db_prefix_4}-{page_prefix_8}`
    /// Example: `ab12-f7e3c2a1`
    #[must_use]
    pub fn generate_external_id(database_id: &str, page_id: &str) -> String {
        let db_clean = Self::clean_id(database_id);
        let page_clean = Self::clean_id(page_id);

        let db_prefix = if db_clean.len() >= 4 {
            &db_clean[..4]
        } else {
            &db_clean
        };

        let page_prefix = if page_clean.len() >= 8 {
            &page_clean[..8]
        } else {
            &page_clean
        };

        format!("{db_prefix}-{page_prefix}")
    }

    /// Cache a page ID mapping for later lookup
    pub async fn cache_page_id(&self, external_id: &str, full_page_id: &str) {
        let mut cache = self.page_id_cache.lock().await;
        cache.insert(external_id.to_string(), full_page_id.to_string());
    }

    /// Look up a full page ID from external ID
    pub async fn get_cached_page_id(&self, external_id: &str) -> Option<String> {
        let cache = self.page_id_cache.lock().await;
        cache.get(external_id).cloned()
    }

    /// Detect time property from database schema
    ///
    /// Returns the configured time property if set, otherwise auto-detects
    /// using `TIME_CONVENTIONS`.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be fetched
    pub async fn detect_time_property(&self, database_id: &str) -> Result<Option<String>> {
        // Check configured override first
        if let Some(prop) = self.get_time_property().await {
            return Ok(Some(prop));
        }

        // Auto-detect from database schema
        let database = self.get_database(database_id).await?;
        let mapping = database.detect_property_mapping(None);
        Ok(mapping.time)
    }

    /// Extract text content from blocks
    #[must_use]
    pub fn blocks_to_text(blocks: &[NotionBlock]) -> String {
        blocks
            .iter()
            .map(NotionBlock::as_plain_text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Convert a Notion page to issue candidate data
    ///
    /// # Arguments
    /// * `page` - The Notion page to convert
    /// * `database_id` - The database ID (for `external_id` generation)
    /// * `mapping` - Property mapping for extracting fields
    /// * `description_text` - Optional pre-fetched description from blocks
    #[must_use]
    pub fn page_to_issue_candidate(
        page: &NotionPage,
        database_id: &str,
        mapping: &PropertyMapping,
        description_text: Option<String>,
    ) -> NotionIssueCandidateData {
        // Generate external ID
        let external_id = Self::generate_external_id(database_id, &page.id);

        // Extract title from mapped property
        let title = mapping
            .title
            .as_ref()
            .and_then(|prop_name| page.properties.get(prop_name))
            .and_then(NotionPropertyValue::as_plain_text)
            .unwrap_or_else(|| "Untitled".to_string());

        // Extract status from mapped property
        let status = mapping
            .status
            .as_ref()
            .and_then(|prop_name| page.properties.get(prop_name))
            .and_then(NotionPropertyValue::as_select_name)
            .unwrap_or_else(|| "Unknown".to_string());

        // Extract description from mapped property or use provided block text
        let description = mapping
            .description
            .as_ref()
            .and_then(|prop_name| page.properties.get(prop_name))
            .and_then(NotionPropertyValue::as_plain_text)
            .or(description_text);

        // Extract labels from multi-select if available
        let labels = page
            .properties
            .values()
            .find(|p| p.value_type == "multi_select")
            .and_then(|p| p.multi_select.as_ref())
            .map(|opts| opts.iter().map(|o| o.name.clone()).collect())
            .unwrap_or_default();

        NotionIssueCandidateData {
            external_id,
            external_system: "notion".to_string(),
            title,
            description,
            status,
            database_id: database_id.to_string(),
            page_id: page.id.clone(),
            labels,
        }
    }

    /// Sync pages from a Notion database to issue candidates
    ///
    /// This is a helper method that:
    /// 1. Queries all pages in the database
    /// 2. Detects property mapping
    /// 3. Converts pages to issue candidate data
    /// 4. Optionally fetches block content for descriptions
    ///
    /// # Arguments
    /// * `database_id` - The Notion database ID
    /// * `config` - Optional property mapping configuration
    /// * `fetch_blocks` - Whether to fetch page blocks for descriptions (slower but more complete)
    ///
    /// # Errors
    ///
    /// Returns an error if API calls fail
    pub async fn fetch_database_as_issues(
        &self,
        database_id: &str,
        config: Option<&PropertyMappingConfig>,
        fetch_blocks: bool,
    ) -> Result<Vec<NotionIssueCandidateData>> {
        // Get database schema and detect property mapping
        let database = self.get_database(database_id).await?;
        let mapping = database.detect_property_mapping(config);

        log::debug!(
            "Database '{}' schema: {}",
            database.title_plain_text(),
            database.describe_schema()
        );
        log::debug!(
            "Detected mapping - title: {:?}, status: {:?}, time: {:?}",
            mapping.title,
            mapping.status,
            mapping.time
        );

        // Query all pages
        let pages = self.query_database_all(database_id).await?;
        log::info!("Fetched {} pages from Notion database", pages.len());

        let mut candidates = Vec::with_capacity(pages.len());

        for page in &pages {
            // Skip archived pages
            if page.archived {
                continue;
            }

            // Fetch block content if requested
            let description_text = if fetch_blocks {
                match self.get_page_blocks_all(&page.id, 2).await {
                    Ok(blocks) => {
                        let text = Self::blocks_to_text(&blocks);
                        if text.is_empty() {
                            None
                        } else {
                            Some(text)
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to fetch blocks for page {}: {e}", page.id);
                        None
                    }
                }
            } else {
                None
            };

            // Convert to issue candidate
            let candidate = Self::page_to_issue_candidate(page, database_id, &mapping, description_text);

            // Cache the page ID for later lookups
            self.cache_page_id(&candidate.external_id, &page.id).await;

            candidates.push(candidate);
        }

        Ok(candidates)
    }

    /// Add time entry using configured or auto-detected time property
    ///
    /// This is used by the trait implementation.
    pub(crate) async fn add_time_entry_internal(
        &self,
        work_item_id: &str,
        duration_seconds: u32,
    ) -> Result<()> {
        // Get page ID from cache
        let page_id = self
            .get_cached_page_id(work_item_id)
            .await
            .ok_or_else(|| {
                anyhow::anyhow!("Page ID not found in cache for: {work_item_id}")
            })?;

        let page = self.get_page(&page_id).await?;

        // Determine the time property to use
        let configured_prop = self.get_time_property().await;

        let property_name = if let Some(ref prop_name) = configured_prop {
            // Verify the configured property exists and is a number type
            if let Some(prop) = page.properties.get(prop_name) {
                if prop.value_type == "number" {
                    prop_name.clone()
                } else {
                    anyhow::bail!(
                        "Configured time property '{}' is not a number type (found: {})",
                        prop_name,
                        prop.value_type
                    );
                }
            } else {
                anyhow::bail!(
                    "Configured time property '{}' not found in page. Available properties: {}",
                    prop_name,
                    page.properties.keys().cloned().collect::<Vec<_>>().join(", ")
                );
            }
        } else {
            // Auto-detect using TIME_CONVENTIONS
            let detected = TIME_CONVENTIONS.iter().find_map(|conv| {
                page.properties.iter().find_map(|(name, prop)| {
                    if prop.value_type == "number" && name.eq_ignore_ascii_case(conv) {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
            });

            detected.ok_or_else(|| {
                let available_numbers: Vec<_> = page
                    .properties
                    .iter()
                    .filter(|(_, p)| p.value_type == "number")
                    .map(|(n, _)| n.as_str())
                    .collect();

                if available_numbers.is_empty() {
                    anyhow::anyhow!(
                        "No number properties found in page. Add a number property for time tracking."
                    )
                } else {
                    anyhow::anyhow!(
                        "No time tracking property found. Available number properties: {}. \
                         Use `toki config set notion.time_property <name>` to configure.",
                        available_numbers.join(", ")
                    )
                }
            })?
        };

        // Get current value and add new time
        let current_value = page
            .properties
            .get(&property_name)
            .and_then(NotionPropertyValue::as_number)
            .unwrap_or(0.0);

        let hours_to_add = f64::from(duration_seconds) / 3600.0;
        let new_value = current_value + hours_to_add;

        log::debug!(
            "Updating time on Notion page {work_item_id} (property '{property_name}'): {current_value:.2} + {hours_to_add:.2} = {new_value:.2} hours"
        );

        self.update_page_property(&page_id, &property_name, NotionPropertyUpdate::Number(new_value))
            .await?;

        log::info!(
            "Time entry added to Notion: {work_item_id} (+{hours_to_add:.2}h, total: {new_value:.2}h)"
        );
        Ok(())
    }
}
