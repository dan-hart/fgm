//! Figma image export API endpoints with rate limiting

use super::cache::{CacheKey, CacheTTL};
use super::client::FigmaClient;
use super::types::ImageResponse;
use anyhow::Result;

impl FigmaClient {
    /// Export nodes as images
    ///
    /// Uses rate limiting and caches the resulting URLs.
    ///
    /// # Arguments
    /// * `file_key` - The file key
    /// * `node_ids` - Node IDs to export
    /// * `format` - Export format (png, svg, pdf, jpg)
    /// * `scale` - Scale factor (1-4)
    pub async fn export_images(
        &self,
        file_key: &str,
        node_ids: &[String],
        format: &str,
        scale: u8,
    ) -> Result<ImageResponse> {
        // Create cache key based on all parameters
        let params_hash = CacheKey::hash_export_params(node_ids, format, scale);
        let cache_key = CacheKey::Images(file_key.to_string(), params_hash);

        // Check cache first
        if let Some(cached) = self.cache().get::<ImageResponse>(&cache_key) {
            // Verify cached URLs are not expired (Figma URLs expire after ~30 min)
            if !cached.images.is_empty() {
                return Ok(cached);
            }
        }

        // Build request
        let ids = node_ids.join(",");
        let url = format!(
            "{}/images/{}?ids={}&format={}&scale={}",
            self.base_url(),
            file_key,
            ids,
            format,
            scale
        );

        // Execute with rate limiting
        let images: ImageResponse = self.get_json(&url).await?;

        // Only cache successful responses with images
        if images.err.is_none() && !images.images.is_empty() {
            self.cache().set(&cache_key, &images, CacheTTL::IMAGE_URLS);
        }

        Ok(images)
    }

    /// Get image fill URLs from a file
    pub async fn get_image_fills(&self, file_key: &str) -> Result<ImageResponse> {
        let url = format!("{}/files/{}/images", self.base_url(), file_key);
        let images: ImageResponse = self.get_json(&url).await?;
        Ok(images)
    }

    /// Download an image from a URL
    ///
    /// This downloads from Figma's S3 bucket, not the API, so rate limiting
    /// is less of a concern. We still use the execute_request wrapper for
    /// consistent error handling.
    pub async fn download_image(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.execute_request(|| self.http().get(url)).await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to download image: {}", response.status());
        }

        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}
