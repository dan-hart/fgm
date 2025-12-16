use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// Figma token (fallback when keychain unavailable)
    /// WARNING: Stored in plaintext - prefer keychain storage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figma_token: Option<String>,
    #[serde(default)]
    pub defaults: DefaultsConfig,
    #[serde(default)]
    pub export: ExportConfig,
    #[serde(default)]
    pub tokens: TokensConfig,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DefaultsConfig {
    pub team_id: Option<String>,
    #[serde(default = "default_output_format")]
    pub output_format: String,
    #[serde(default = "default_image_protocol")]
    pub image_protocol: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExportConfig {
    #[serde(default = "default_format")]
    pub default_format: String,
    #[serde(default = "default_scale")]
    pub default_scale: u8,
    pub output_dir: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TokensConfig {
    #[serde(default = "default_css_prefix")]
    pub css_prefix: String,
    #[serde(default = "default_swift_prefix")]
    pub swift_prefix: String,
}

fn default_output_format() -> String {
    "table".to_string()
}

fn default_image_protocol() -> String {
    "auto".to_string()
}

fn default_format() -> String {
    "png".to_string()
}

fn default_scale() -> u8 {
    2
}

fn default_css_prefix() -> String {
    "--figma-".to_string()
}

fn default_swift_prefix() -> String {
    "Figma".to_string()
}

impl Config {
    /// Get the config directory path
    pub fn config_dir() -> Option<PathBuf> {
        ProjectDirs::from("", "", "fgm").map(|dirs| dirs.config_dir().to_path_buf())
    }

    /// Get the config file path
    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|dir| dir.join("config.toml"))
    }

    /// Load config from file, returning default if not found
    pub fn load() -> Result<Self> {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Ok(Self::default()),
        };

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir().ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
        fs::create_dir_all(&dir)?;

        let path = dir.join("config.toml");
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Get token from config file (fallback storage)
    pub fn get_token(&self) -> Option<&str> {
        self.figma_token.as_deref()
    }

    /// Store token in config file (fallback storage)
    /// WARNING: This stores the token in plaintext
    pub fn set_token(&mut self, token: &str) {
        self.figma_token = Some(token.to_string());
    }

    /// Remove token from config file
    pub fn remove_token(&mut self) {
        self.figma_token = None;
    }

    /// Check if config file has a token
    pub fn has_token(&self) -> bool {
        self.figma_token.is_some()
    }
}
