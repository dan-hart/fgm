use anyhow::{anyhow, Result};
use url::Url;

/// Parsed Figma URL components
#[derive(Debug, Clone)]
pub struct FigmaUrl {
    /// The file key (required)
    pub file_key: String,
    /// The node ID if specified (optional)
    pub node_id: Option<String>,
    /// The file name from URL (optional)
    pub file_name: Option<String>,
}

impl FigmaUrl {
    /// Parse a Figma URL or file key
    ///
    /// Supports formats:
    /// - `https://www.figma.com/file/abc123/File-Name`
    /// - `https://www.figma.com/file/abc123/File-Name?node-id=123:456`
    /// - `https://www.figma.com/design/abc123/File-Name`
    /// - `https://www.figma.com/design/abc123/File-Name?node-id=123-456`
    /// - `https://figma.com/file/abc123`
    /// - `abc123` (just the file key)
    /// - `abc123:123:456` (file key with node ID)
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();

        // Check if it's a URL
        if input.starts_with("http://") || input.starts_with("https://") {
            return Self::parse_url(input);
        }

        // Check if it's a file key with node ID (format: fileKey:nodeId)
        if let Some((file_key, node_id)) = Self::parse_key_with_node(input) {
            return Ok(Self {
                file_key,
                node_id: Some(node_id),
                file_name: None,
            });
        }

        // Assume it's just a file key
        Ok(Self {
            file_key: input.to_string(),
            node_id: None,
            file_name: None,
        })
    }

    fn parse_url(input: &str) -> Result<Self> {
        let url = Url::parse(input)?;

        // Validate domain
        let host = url.host_str().ok_or_else(|| anyhow!("Invalid URL: no host"))?;
        if !host.contains("figma.com") {
            return Err(anyhow!("Not a Figma URL: {}", host));
        }

        // Parse path segments
        // Formats:
        // /file/abc123/File-Name
        // /design/abc123/File-Name
        // /proto/abc123/File-Name
        let path_segments: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();

        if path_segments.len() < 2 {
            return Err(anyhow!("Invalid Figma URL: missing file key"));
        }

        let file_type = path_segments[0];
        if !["file", "design", "proto", "board"].contains(&file_type) {
            return Err(anyhow!("Invalid Figma URL type: {}", file_type));
        }

        let file_key = path_segments[1].to_string();
        let file_name = path_segments.get(2).map(|s| {
            // URL decode and replace hyphens with spaces
            urlencoding::decode(s)
                .map(|s| s.replace('-', " "))
                .unwrap_or_else(|_| s.to_string())
        });

        // Parse node-id from query params
        let node_id = url.query_pairs().find_map(|(key, value)| {
            if key == "node-id" {
                // Convert URL format (123-456) to API format (123:456)
                Some(value.replace('-', ":"))
            } else {
                None
            }
        });

        Ok(Self {
            file_key,
            node_id,
            file_name,
        })
    }

    fn parse_key_with_node(input: &str) -> Option<(String, String)> {
        // Format: fileKey:nodeId where nodeId contains a colon (e.g., abc123:1:2)
        // We need at least 2 colons for this to be valid
        let parts: Vec<&str> = input.splitn(2, ':').collect();
        if parts.len() == 2 {
            let potential_file_key = parts[0];
            let potential_node_id = parts[1];

            // Node IDs typically look like "123:456" or similar
            // File keys are alphanumeric
            if potential_file_key.chars().all(|c| c.is_alphanumeric())
                && potential_node_id.contains(':')
            {
                return Some((potential_file_key.to_string(), potential_node_id.to_string()));
            }
        }
        None
    }

    /// Check if a string looks like a Figma URL
    pub fn is_figma_url(input: &str) -> bool {
        input.contains("figma.com/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_url() {
        let url = FigmaUrl::parse("https://www.figma.com/file/abc123/My-Design").unwrap();
        assert_eq!(url.file_key, "abc123");
        assert_eq!(url.file_name, Some("My Design".to_string()));
        assert_eq!(url.node_id, None);
    }

    #[test]
    fn test_parse_url_with_node_id() {
        let url = FigmaUrl::parse("https://www.figma.com/design/abc123/Test?node-id=1-234").unwrap();
        assert_eq!(url.file_key, "abc123");
        assert_eq!(url.node_id, Some("1:234".to_string()));
    }

    #[test]
    fn test_parse_file_key() {
        let url = FigmaUrl::parse("abc123xyz").unwrap();
        assert_eq!(url.file_key, "abc123xyz");
        assert_eq!(url.node_id, None);
    }

    #[test]
    fn test_parse_key_with_node() {
        let url = FigmaUrl::parse("abc123:1:234").unwrap();
        assert_eq!(url.file_key, "abc123");
        assert_eq!(url.node_id, Some("1:234".to_string()));
    }
}
