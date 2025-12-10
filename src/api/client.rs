use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;

const FIGMA_API_BASE: &str = "https://api.figma.com/v1";

/// Figma API client
pub struct FigmaClient {
    client: Client,
    #[allow(dead_code)]
    token: String,
}

impl FigmaClient {
    /// Create a new Figma client with the given access token
    pub fn new(token: String) -> Result<Self> {
        let mut headers = HeaderMap::new();
        // Figma uses X-Figma-Token header, not Bearer auth
        headers.insert(
            "X-Figma-Token",
            HeaderValue::from_str(&token)?,
        );

        let client = Client::builder()
            .default_headers(headers)
            .user_agent("fgm-cli/0.1.0")
            .build()?;

        Ok(Self { client, token })
    }

    /// Get the base URL for the API
    pub fn base_url(&self) -> &str {
        FIGMA_API_BASE
    }

    /// Get a reference to the underlying HTTP client
    pub fn http(&self) -> &Client {
        &self.client
    }

    /// Check if the token is valid by making a test request
    pub async fn validate_token(&self) -> Result<bool> {
        let url = format!("{}/me", FIGMA_API_BASE);
        let response = self.client.get(&url).send().await?;
        Ok(response.status().is_success())
    }
}
