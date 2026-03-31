//! Rate limiting and retry logic for Figma API
//!
//! Provides exponential backoff with jitter for handling HTTP 429 responses.

use reqwest::{Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

/// API request class for endpoint-aware throttling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequestClass {
    Metadata,
    Nodes,
    Images,
    Team,
    Other,
    Download,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestClassCounts {
    pub metadata: u64,
    pub nodes: u64,
    pub images: u64,
    pub team: u64,
    pub other: u64,
    pub download: u64,
}

/// Rate limit state tracking and retry logic
pub struct RateLimiter {
    /// Current retry count
    retry_count: u32,
    /// Maximum retries before giving up
    max_retries: u32,
    /// Base delay for exponential backoff (milliseconds)
    base_delay_ms: u64,
    /// Maximum delay cap (milliseconds)
    max_delay_ms: u64,
    /// Remaining requests (from X-RateLimit-Remaining header)
    remaining: Option<u32>,
    /// Retry-After value (seconds) from last 429 response
    retry_after: Option<u64>,
    /// Total HTTP requests attempted through this limiter
    total_requests: u64,
    /// Total retries performed after 429 responses
    total_retries: u64,
    /// Total 429 responses observed
    total_rate_limited_responses: u64,
    /// Total proactive throttles performed
    total_proactive_throttles: u64,
    /// Total milliseconds slept for proactive throttling
    total_proactive_wait_ms: u64,
    /// Per-class request counters
    class_counts: RequestClassCounts,
    /// Token bucket capacity
    bucket_capacity: f64,
    /// Current available tokens
    available_tokens: f64,
    /// Tokens refilled per second
    bucket_refill_per_sec: f64,
    /// Last refill timestamp
    last_refill: std::time::Instant,
    /// Remaining threshold for proactive slowdown
    proactive_remaining_threshold: u32,
}

/// Information parsed from rate limit headers
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    /// Remaining requests before rate limit
    pub remaining: Option<u32>,
    /// Seconds to wait before retrying (from Retry-After header)
    pub retry_after: Option<u64>,
    /// Whether we hit a rate limit (HTTP 429)
    pub is_rate_limited: bool,
}

/// Cumulative telemetry for rate-limit behavior.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimitTelemetry {
    pub total_requests: u64,
    pub total_retries: u64,
    pub total_rate_limited_responses: u64,
    pub total_proactive_throttles: u64,
    pub total_proactive_wait_ms: u64,
    pub remaining: Option<u32>,
    pub retry_after: Option<u64>,
    pub request_class_counts: RequestClassCounts,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            retry_count: 0,
            max_retries: 5,
            base_delay_ms: 1000,   // 1 second
            max_delay_ms: 120_000, // 2 minutes max
            remaining: None,
            retry_after: None,
            total_requests: 0,
            total_retries: 0,
            total_rate_limited_responses: 0,
            total_proactive_throttles: 0,
            total_proactive_wait_ms: 0,
            class_counts: RequestClassCounts::default(),
            bucket_capacity: 8.0,
            available_tokens: 8.0,
            bucket_refill_per_sec: 4.0,
            last_refill: std::time::Instant::now(),
            proactive_remaining_threshold: 12,
        }
    }
}

impl RateLimiter {
    /// Create a new rate limiter with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a rate limiter with custom settings
    pub fn with_config(max_retries: u32, base_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            retry_count: 0,
            max_retries,
            base_delay_ms,
            max_delay_ms,
            remaining: None,
            retry_after: None,
            total_requests: 0,
            total_retries: 0,
            total_rate_limited_responses: 0,
            total_proactive_throttles: 0,
            total_proactive_wait_ms: 0,
            class_counts: RequestClassCounts::default(),
            bucket_capacity: 8.0,
            available_tokens: 8.0,
            bucket_refill_per_sec: 4.0,
            last_refill: std::time::Instant::now(),
            proactive_remaining_threshold: 12,
        }
    }

    /// Create a rate limiter with explicit token-bucket parameters.
    pub fn with_budget(
        max_retries: u32,
        base_delay_ms: u64,
        max_delay_ms: u64,
        bucket_capacity: f64,
        bucket_refill_per_sec: f64,
    ) -> Self {
        let mut limiter = Self::with_config(max_retries, base_delay_ms, max_delay_ms);
        limiter.bucket_capacity = bucket_capacity.max(1.0);
        limiter.available_tokens = limiter.bucket_capacity;
        limiter.bucket_refill_per_sec = bucket_refill_per_sec.max(0.1);
        limiter
    }

    /// Record a request attempt before dispatching HTTP.
    pub fn record_request_attempt(&mut self, class: RequestClass) {
        self.total_requests = self.total_requests.saturating_add(1);
        match class {
            RequestClass::Metadata => {
                self.class_counts.metadata = self.class_counts.metadata.saturating_add(1)
            }
            RequestClass::Nodes => {
                self.class_counts.nodes = self.class_counts.nodes.saturating_add(1)
            }
            RequestClass::Images => {
                self.class_counts.images = self.class_counts.images.saturating_add(1)
            }
            RequestClass::Team => self.class_counts.team = self.class_counts.team.saturating_add(1),
            RequestClass::Other => {
                self.class_counts.other = self.class_counts.other.saturating_add(1)
            }
            RequestClass::Download => {
                self.class_counts.download = self.class_counts.download.saturating_add(1)
            }
        }
    }

    /// Record a 429 rate-limited response.
    pub fn record_rate_limited_response(&mut self) {
        self.total_rate_limited_responses = self.total_rate_limited_responses.saturating_add(1);
    }

    /// Parse rate limit headers from response
    ///
    /// Looks for:
    /// - `X-RateLimit-Remaining`: Number of requests remaining
    /// - `Retry-After`: Seconds to wait before retrying
    pub fn parse_headers(&mut self, response: &Response) -> RateLimitInfo {
        // Parse X-RateLimit-Remaining header (Figma may or may not provide this)
        self.remaining = response
            .headers()
            .get("X-RateLimit-Remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());

        // Parse Retry-After header (standard HTTP for 429 responses)
        self.retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());

        let is_rate_limited = response.status() == StatusCode::TOO_MANY_REQUESTS;

        RateLimitInfo {
            remaining: self.remaining,
            retry_after: self.retry_after,
            is_rate_limited,
        }
    }

    /// Check if we're approaching rate limits (proactive throttling)
    pub fn should_throttle(&self) -> bool {
        matches!(self.remaining, Some(remaining) if remaining < self.proactive_remaining_threshold)
    }

    /// Calculate delay with exponential backoff and jitter
    ///
    /// Formula: base_delay * 2^retry_count + random_jitter
    /// Jitter is 0-25% of the calculated delay to prevent thundering herd
    pub fn calculate_delay(&self) -> Duration {
        // Use Retry-After if provided by server
        if let Some(retry_after) = self.retry_after {
            return Duration::from_secs(retry_after);
        }

        // Exponential backoff: base_delay * 2^retry_count
        let exp_delay = self
            .base_delay_ms
            .saturating_mul(2u64.pow(self.retry_count));
        let capped_delay = exp_delay.min(self.max_delay_ms);

        // Add jitter (0-25% of delay) to prevent thundering herd
        let jitter = (rand::random::<f64>() * 0.25 * capped_delay as f64) as u64;

        Duration::from_millis(capped_delay + jitter)
    }

    /// Wait before retry, returns false if max retries exceeded
    pub async fn wait_and_retry(&mut self) -> bool {
        if self.retry_count >= self.max_retries {
            return false;
        }

        let delay = self.calculate_delay();
        eprintln!(
            "Rate limited. Waiting {:?} before retry ({}/{})...",
            delay,
            self.retry_count + 1,
            self.max_retries
        );

        sleep(delay).await;
        self.retry_count += 1;
        self.total_retries = self.total_retries.saturating_add(1);
        true
    }

    /// Reset retry count after successful request
    pub fn reset(&mut self) {
        self.retry_count = 0;
        self.retry_after = None;
    }

    fn class_token_cost(class: RequestClass) -> f64 {
        match class {
            RequestClass::Metadata => 1.0,
            RequestClass::Nodes => 1.5,
            RequestClass::Images => 2.0,
            RequestClass::Team => 1.25,
            RequestClass::Other => 1.0,
            RequestClass::Download => 0.25,
        }
    }

    fn refill_bucket(&mut self) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        if elapsed <= 0.0 {
            return;
        }

        let replenished = elapsed * self.bucket_refill_per_sec;
        self.available_tokens = (self.available_tokens + replenished).min(self.bucket_capacity);
        self.last_refill = now;
    }

    /// Proactive delay based on token budget and remaining-header pressure.
    pub async fn proactive_delay(&mut self, class: RequestClass) {
        self.refill_bucket();

        let cost = Self::class_token_cost(class);
        if self.available_tokens < cost {
            let deficit = cost - self.available_tokens;
            let delay_secs = deficit / self.bucket_refill_per_sec;
            let delay = Duration::from_secs_f64(delay_secs.max(0.05));
            self.total_proactive_throttles = self.total_proactive_throttles.saturating_add(1);
            self.total_proactive_wait_ms = self
                .total_proactive_wait_ms
                .saturating_add(delay.as_millis() as u64);
            sleep(delay).await;
            self.refill_bucket();
        }

        self.available_tokens = (self.available_tokens - cost).max(0.0);

        if self.should_throttle() && !matches!(class, RequestClass::Download) {
            let delay = Duration::from_millis(350);
            self.total_proactive_throttles = self.total_proactive_throttles.saturating_add(1);
            self.total_proactive_wait_ms = self
                .total_proactive_wait_ms
                .saturating_add(delay.as_millis() as u64);
            sleep(delay).await;
        }
    }

    /// Snapshot cumulative telemetry for external reporting.
    pub fn telemetry_snapshot(&self) -> RateLimitTelemetry {
        RateLimitTelemetry {
            total_requests: self.total_requests,
            total_retries: self.total_retries,
            total_rate_limited_responses: self.total_rate_limited_responses,
            total_proactive_throttles: self.total_proactive_throttles,
            total_proactive_wait_ms: self.total_proactive_wait_ms,
            remaining: self.remaining,
            retry_after: self.retry_after,
            request_class_counts: self.class_counts.clone(),
        }
    }

    /// Get current retry count
    pub fn retry_count(&self) -> u32 {
        self.retry_count
    }

    /// Check if the response indicates a rate limit error
    pub fn is_rate_limit_response(response: &Response) -> bool {
        response.status() == StatusCode::TOO_MANY_REQUESTS
    }

    /// Check if an error message indicates rate limiting (legacy fallback)
    pub fn is_rate_limit_error(error_msg: &str) -> bool {
        error_msg.to_lowercase().contains("rate limit")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff() {
        let mut limiter = RateLimiter::new();

        // First retry should use base delay
        limiter.retry_count = 0;
        let d1 = limiter.calculate_delay();
        assert!(d1.as_millis() >= 1000 && d1.as_millis() <= 1250);

        // Second retry should roughly double
        limiter.retry_count = 1;
        let d2 = limiter.calculate_delay();
        assert!(d2.as_millis() >= 2000 && d2.as_millis() <= 2500);

        // Third retry
        limiter.retry_count = 2;
        let d3 = limiter.calculate_delay();
        assert!(d3.as_millis() >= 4000 && d3.as_millis() <= 5000);
    }

    #[test]
    fn test_max_delay_cap() {
        let limiter = RateLimiter::with_config(10, 1000, 5000);

        // High retry count should still be capped
        let mut limiter_high = limiter;
        limiter_high.retry_count = 10;
        let delay = limiter_high.calculate_delay();
        assert!(delay.as_millis() <= 6250); // 5000 + 25% jitter max
    }

    #[test]
    fn test_retry_after_override() {
        let mut limiter = RateLimiter::new();
        limiter.retry_after = Some(30);

        let delay = limiter.calculate_delay();
        assert_eq!(delay.as_secs(), 30);
    }

    #[test]
    fn test_should_throttle() {
        let mut limiter = RateLimiter::new();

        limiter.remaining = Some(5);
        assert!(limiter.should_throttle());

        limiter.remaining = Some(15);
        assert!(!limiter.should_throttle());

        limiter.remaining = None;
        assert!(!limiter.should_throttle());
    }

    #[test]
    fn test_telemetry_snapshot_counts_requests_and_retries() {
        let mut limiter = RateLimiter::new();
        limiter.record_request_attempt(RequestClass::Metadata);
        limiter.record_request_attempt(RequestClass::Images);
        limiter.record_rate_limited_response();
        limiter.total_retries = 3;
        limiter.total_proactive_throttles = 2;

        let snapshot = limiter.telemetry_snapshot();
        assert_eq!(snapshot.total_requests, 2);
        assert_eq!(snapshot.total_retries, 3);
        assert_eq!(snapshot.total_rate_limited_responses, 1);
        assert_eq!(snapshot.total_proactive_throttles, 2);
        assert_eq!(snapshot.request_class_counts.metadata, 1);
        assert_eq!(snapshot.request_class_counts.images, 1);
    }

    #[test]
    fn test_request_class_counters() {
        let mut limiter = RateLimiter::new();
        limiter.record_request_attempt(RequestClass::Metadata);
        limiter.record_request_attempt(RequestClass::Nodes);
        limiter.record_request_attempt(RequestClass::Nodes);
        limiter.record_request_attempt(RequestClass::Download);

        let snapshot = limiter.telemetry_snapshot();
        assert_eq!(snapshot.request_class_counts.metadata, 1);
        assert_eq!(snapshot.request_class_counts.nodes, 2);
        assert_eq!(snapshot.request_class_counts.download, 1);
    }

    #[tokio::test]
    async fn test_token_bucket_waits_when_depleted() {
        let mut limiter = RateLimiter::with_budget(1, 1000, 2000, 1.0, 100.0);
        limiter.available_tokens = 0.0;
        limiter.proactive_delay(RequestClass::Images).await;
        let snapshot = limiter.telemetry_snapshot();
        assert!(snapshot.total_proactive_throttles >= 1);
        assert!(snapshot.total_proactive_wait_ms > 0);
    }
}
