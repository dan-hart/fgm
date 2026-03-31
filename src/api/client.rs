//! Figma API client with caching and rate limiting

use super::cache::{create_shared_cache, CacheStats, FigmaCache};
use super::rate_limit::{RateLimitTelemetry, RateLimiter, RequestClass};
use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Response, StatusCode};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

const FIGMA_API_BASE: &str = "https://api.figma.com/v1";
const DEFAULT_API_CONCURRENCY: usize = 3;
const DEFAULT_DOWNLOAD_CONCURRENCY: usize = 10;

/// Figma API client with integrated caching and rate limiting
pub struct FigmaClient {
    client: Client,
    #[allow(dead_code)]
    token: String,
    cache: Arc<FigmaCache>,
    rate_limiter: Arc<Mutex<RateLimiter>>,
    inflight_requests: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    api_semaphore: Arc<Semaphore>,
    download_semaphore: Arc<Semaphore>,
    download_parallelism: usize,
    stale_while_revalidate: bool,
}

impl Clone for FigmaClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            token: self.token.clone(),
            cache: self.cache.clone(),
            rate_limiter: self.rate_limiter.clone(),
            inflight_requests: self.inflight_requests.clone(),
            api_semaphore: self.api_semaphore.clone(),
            download_semaphore: self.download_semaphore.clone(),
            download_parallelism: self.download_parallelism,
            stale_while_revalidate: self.stale_while_revalidate,
        }
    }
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

        let user_agent = format!("fgm-cli/{}", env!("CARGO_PKG_VERSION"));
        let client = Client::builder()
            .default_headers(headers)
            .user_agent(user_agent)
            .build()?;

        Ok(Self {
            client,
            token,
            cache,
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new())),
            inflight_requests: Arc::new(Mutex::new(HashMap::new())),
            api_semaphore: Arc::new(Semaphore::new(DEFAULT_API_CONCURRENCY)),
            download_semaphore: Arc::new(Semaphore::new(DEFAULT_DOWNLOAD_CONCURRENCY)),
            download_parallelism: DEFAULT_DOWNLOAD_CONCURRENCY,
            stale_while_revalidate: true,
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
        let response = self
            .execute_request(RequestClass::Other, || self.client.get(&url))
            .await?;
        Ok(response.status().is_success())
    }

    /// Run a closure under an in-flight coalescing key (singleflight behavior).
    pub async fn run_singleflight<T, F, Fut>(&self, key: String, op: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let lock = {
            let mut inflight = self.inflight_requests.lock().await;
            inflight
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        let _guard = lock.lock().await;
        let result = op().await;

        let mut inflight = self.inflight_requests.lock().await;
        if let Some(existing) = inflight.get(&key) {
            if Arc::ptr_eq(existing, &lock) && Arc::strong_count(existing) <= 2 {
                inflight.remove(&key);
            }
        }

        result
    }

    pub fn stale_while_revalidate_enabled(&self) -> bool {
        self.stale_while_revalidate
    }

    pub fn download_parallelism(&self) -> usize {
        self.download_parallelism
    }

    fn classify_url(url: &str) -> RequestClass {
        if url.contains("/images/") {
            return RequestClass::Images;
        }
        if url.contains("/nodes?") {
            return RequestClass::Nodes;
        }
        if url.contains("/teams/") || url.contains("/projects/") {
            return RequestClass::Team;
        }
        if url.contains("/files/") || url.ends_with("/me") {
            return RequestClass::Metadata;
        }
        RequestClass::Other
    }

    /// Execute an HTTP request with rate limit handling and retries
    ///
    /// This method wraps requests with:
    /// - Proactive throttling when approaching rate limits
    /// - Automatic retry with exponential backoff on HTTP 429
    /// - Rate limit header parsing
    pub async fn execute_request<F>(&self, class: RequestClass, request_fn: F) -> Result<Response>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        loop {
            let _permit = match class {
                RequestClass::Download => self
                    .download_semaphore
                    .acquire()
                    .await
                    .map_err(|_| anyhow::anyhow!("download semaphore closed"))?,
                _ => self
                    .api_semaphore
                    .acquire()
                    .await
                    .map_err(|_| anyhow::anyhow!("api semaphore closed"))?,
            };

            // Check if we should proactively throttle
            {
                let mut limiter = self.rate_limiter.lock().await;
                limiter.record_request_attempt(class);
                limiter.proactive_delay(class).await;
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
                    limiter.record_rate_limited_response();
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
        let class = Self::classify_url(url);
        let response = self.execute_request(class, || self.client.get(url)).await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, error_text);
        }

        let result = response.json().await?;
        Ok(result)
    }

    /// Retrieve cumulative rate-limit telemetry for this client instance.
    pub async fn rate_limit_telemetry(&self) -> RateLimitTelemetry {
        let limiter = self.rate_limiter.lock().await;
        limiter.telemetry_snapshot()
    }
}
