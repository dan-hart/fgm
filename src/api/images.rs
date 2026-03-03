//! Figma image export API endpoints with rate limiting

use super::cache::{CacheEntryFreshness, CacheKey, CacheTTL};
use super::client::FigmaClient;
use super::rate_limit::RequestClass;
use super::types::ImageResponse;
use anyhow::Result;

fn canonical_node_ids(node_ids: &[String]) -> Vec<String> {
    let mut ids: Vec<String> = node_ids
        .iter()
        .map(|id| {
            let mut normalized = id.trim().to_string();
            if normalized.contains("%3A") || normalized.contains("%3a") {
                normalized = normalized.replace("%3A", ":").replace("%3a", ":");
            }
            if normalized.contains('-') && !normalized.contains(':') {
                normalized = normalized.replace('-', ":");
            }
            normalized
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

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
        scale: f32,
    ) -> Result<ImageResponse> {
        let canonical_ids = canonical_node_ids(node_ids);

        // Create cache key based on all parameters
        let params_hash = CacheKey::hash_export_params(&canonical_ids, format, scale);
        let cache_key = CacheKey::Images(file_key.to_string(), params_hash);

        // Check cache first
        if let Some((cached, freshness)) = self.cache().get_with_freshness::<ImageResponse>(&cache_key)
        {
            if !cached.images.is_empty() {
                match freshness {
                    CacheEntryFreshness::Fresh => return Ok(cached),
                    CacheEntryFreshness::Stale if self.stale_while_revalidate_enabled() => {
                        let client = self.clone();
                        let refresh_key = cache_key.clone();
                        let refresh_ids = canonical_ids.clone();
                        let refresh_file_key = file_key.to_string();
                        let refresh_format = format.to_string();
                        tokio::spawn(async move {
                            let ids = refresh_ids.join(",");
                            let url = format!(
                                "{}/images/{}?ids={}&format={}&scale={}",
                                client.base_url(),
                                refresh_file_key,
                                ids,
                                refresh_format,
                                scale
                            );
                            if let Ok(fresh) = client.get_json::<ImageResponse>(&url).await {
                                if fresh.err.is_none() && !fresh.images.is_empty() {
                                    client.cache().set(&refresh_key, &fresh, CacheTTL::IMAGE_URLS);
                                }
                            }
                        });
                        return Ok(cached);
                    }
                    CacheEntryFreshness::Stale => {}
                }
            }
        }
        let singleflight_key = format!("api:{}", cache_key.as_string());
        self.run_singleflight(singleflight_key, || async move {
            if let Some(cached) = self.cache().get::<ImageResponse>(&cache_key) {
                if !cached.images.is_empty() {
                    return Ok(cached);
                }
            }

            // Build request
            let ids = canonical_ids.join(",");
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
        })
        .await
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
        let response = self
            .execute_request(RequestClass::Download, || self.http().get(url))
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to download image: {}", response.status());
        }

        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}
