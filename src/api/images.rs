use super::client::FigmaClient;
use super::types::ImageResponse;
use anyhow::Result;

impl FigmaClient {
    /// Export nodes as images
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
        let ids = node_ids.join(",");
        let url = format!(
            "{}/images/{}?ids={}&format={}&scale={}",
            self.base_url(),
            file_key,
            ids,
            format,
            scale
        );
        let response = self.http().get(&url).send().await?;
        let images: ImageResponse = response.json().await?;
        Ok(images)
    }

    /// Get image fill URLs from a file
    pub async fn get_image_fills(&self, file_key: &str) -> Result<ImageResponse> {
        let url = format!("{}/files/{}/images", self.base_url(), file_key);
        let response = self.http().get(&url).send().await?;
        let images: ImageResponse = response.json().await?;
        Ok(images)
    }

    /// Download an image from a URL
    pub async fn download_image(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.http().get(url).send().await?;
        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}
