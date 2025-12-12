//! Figma API client with caching and rate limiting

use super::cache::{create_shared_cache, FigmaCache, CacheStats};
use super::rate_limit::RateLimiter;
use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Response, StatusCode};
use std::sync::Arc;
use tokio::sync::Mutex;

const FIGMA_API_BASE: &str = "https://api.figma.com/v1";

/// Figma API client with integrated caching and rate limiting
pub struct FigmaClient {
    client: Client,
    #[allow(dead_code)]
    token: String,
    cache: Arc<FigmaCache>,
    rate_limiter: Arc<Mutex<RateLimiter>>,
}

impl FigmaClient {
    /// Create a new Figma client with the given access token
    ///
    /// Uses a shared cache with disk persistence by default.
    pub fn new(token: String) -> Result<Self> {
        let cache = create_shared_cache();
        Self::with_cache(token, cache)
    }

    /// Create a new Figma client with a custom cache
    pub fn with_cache(token: String, cache: Arc<FigmaCache>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        // Figma uses X-Figma-Token header, not Bearer auth
        headers.insert("X-Figma-Token", HeaderValue::from_str(&token)?);

        let client = Client::builder()
            .default_headers(headers)
            .user_agent("fgm-cli/0.1.0")
            .build()?;

        Ok(Self {
            client,
            token,
            cache,
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new())),
        })
    }

    /// Create a client without caching (memory only, no persistence)
    pub fn without_cache(token: String) -> Result<Self> {
        let cache = Arc::new(FigmaCache::memory_only());
        Self::with_cache(token, cache)
    }

    /// Get the base URL for the API
    pub fn base_url(&self) -> &str {
        FIGMA_API_BASE
    }

    /// Get a reference to the underlying HTTP client
    pub fn http(&self) -> &Client {
        &self.client
    }

    /// Get a reference to the cache
    pub fn cache(&self) -> &Arc<FigmaCache> {
        &self.cache
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Clear all cached data
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Check if the token is valid by making a test request
    pub async fn validate_token(&self) -> Result<bool> {
        let url = format!("{}/me", FIGMA_API_BASE);
        let response = self.execute_request(|| self.client.get(&url)).await?;
        Ok(response.status().is_success())
    }

    /// Execute an HTTP request with rate limit handling and retries
    ///
    /// This method wraps requests with:
    /// - Proactive throttling when approaching rate limits
    /// - Automatic retry with exponential backoff on HTTP 429
    /// - Rate limit header parsing
    pub async fn execute_request<F>(&self, request_fn: F) -> Result<Response>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        loop {
            // Check if we should proactively throttle
            {
                let limiter = self.rate_limiter.lock().await;
                limiter.proactive_delay().await;
            }

            // Execute the request
            let response = request_fn().send().await?;

            // Parse rate limit headers
            {
                let mut limiter = self.rate_limiter.lock().await;
                limiter.parse_headers(&response);
            }

            // Check for rate limiting
            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                let should_retry = {
                    let mut limiter = self.rate_limiter.lock().await;
                    limiter.wait_and_retry().await
                };

                if should_retry {
                    continue;
                } else {
                    anyhow::bail!(
                        "Rate limit exceeded after maximum retries. Please wait before trying again."
                    );
                }
            }

            // Reset rate limiter on success
            {
                let mut limiter = self.rate_limiter.lock().await;
                limiter.reset();
            }

            return Ok(response);
        }
    }

    /// Execute a GET request and parse JSON response with rate limiting
    pub async fn get_json<T>(&self, url: &str) -> Result<T>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let response = self.execute_request(|| self.client.get(url)).await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, error_text);
        }

        let result = response.json().await?;
        Ok(result)
    }
}
