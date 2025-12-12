//! Rate limiting and retry logic for Figma API
//!
//! Provides exponential backoff with jitter for handling HTTP 429 responses.

use reqwest::{Response, StatusCode};
use std::time::Duration;
use tokio::time::sleep;

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

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            retry_count: 0,
            max_retries: 5,
            base_delay_ms: 1000,   // 1 second
            max_delay_ms: 120_000, // 2 minutes max
            remaining: None,
            retry_after: None,
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
        }
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
        matches!(self.remaining, Some(remaining) if remaining < 10)
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
        let exp_delay = self.base_delay_ms.saturating_mul(2u64.pow(self.retry_count));
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
        true
    }

    /// Reset retry count after successful request
    pub fn reset(&mut self) {
        self.retry_count = 0;
        self.retry_after = None;
    }

    /// Proactive delay when approaching limits
    pub async fn proactive_delay(&self) {
        if self.should_throttle() {
            let delay = Duration::from_millis(500);
            sleep(delay).await;
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
}
