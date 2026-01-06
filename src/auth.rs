use anyhow::{anyhow, Context, Result};
use keyring::Entry;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::output;

use crate::config::Config;

const SERVICE_NAME: &str = "fgm";
const USERNAME: &str = "figma_token";
static KEYCHAIN_ENABLED: AtomicBool = AtomicBool::new(true);

/// Token source information for debugging
#[derive(Debug, Clone, PartialEq)]
pub enum TokenSource {
    Environment,
    Keychain,
    ConfigFile,
}

impl std::fmt::Display for TokenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenSource::Environment => write!(f, "environment variable (FIGMA_TOKEN)"),
            TokenSource::Keychain => write!(f, "system keychain"),
            TokenSource::ConfigFile => write!(f, "config file"),
        }
    }
}

/// Result of token retrieval with source information
pub struct TokenResult {
    pub token: String,
    pub source: TokenSource,
}

/// Enable or disable keychain access (to avoid OS prompts)
pub fn set_keychain_enabled(enabled: bool) {
    KEYCHAIN_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_keychain_enabled() -> bool {
    KEYCHAIN_ENABLED.load(Ordering::Relaxed)
}

/// Get the Figma token from environment variable, keychain, or config file
pub fn get_token() -> Result<String> {
    get_token_with_source().map(|r| r.token)
}

/// Get the Figma token along with its source
pub fn get_token_with_source() -> Result<TokenResult> {
    // 1. First check environment variable
    if let Ok(token) = env::var("FIGMA_TOKEN") {
        if !token.is_empty() {
            return Ok(TokenResult {
                token,
                source: TokenSource::Environment,
            });
        }
    }

    // 2. Then check config file (default storage)
    match get_token_from_config() {
        Ok(token) => {
            return Ok(TokenResult {
                token,
                source: TokenSource::ConfigFile,
            });
        }
        Err(_) => {}
    }

    // 3. Finally check keychain (unless disabled)
    if is_keychain_enabled() {
        match get_token_from_keychain() {
            Ok(token) => {
                return Ok(TokenResult {
                    token,
                    source: TokenSource::Keychain,
                });
            }
            Err(e) => {
                // Log keychain error only in verbose mode
                output::print_verbose(&format!("Note: Keychain access failed ({}).", e));
            }
        }
    }

    Err(anyhow!(
        "No Figma token found. Set FIGMA_TOKEN environment variable or run 'fgm auth login'"
    ))
}

/// Get token specifically from keychain
pub fn get_token_from_keychain() -> Result<String> {
    if !is_keychain_enabled() {
        return Err(anyhow!("Keychain access disabled"));
    }
    let entry = Entry::new(SERVICE_NAME, USERNAME)
        .context("Failed to create keychain entry - keychain may not be available")?;

    entry
        .get_password()
        .map_err(|e| match e {
            keyring::Error::NoEntry => anyhow!("No token stored in keychain"),
            keyring::Error::Ambiguous(_) => anyhow!("Multiple keychain entries found - please run 'fgm auth logout' and re-login"),
            keyring::Error::PlatformFailure(ref msg) => anyhow!("Keychain platform error: {}", msg),
            keyring::Error::NoStorageAccess(ref msg) => anyhow!("Keychain access denied: {}", msg),
            _ => anyhow!("Keychain error: {}", e),
        })
}

/// Get token from config file
pub fn get_token_from_config() -> Result<String> {
    let config = Config::load().context("Failed to load config file")?;
    config
        .get_token()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("No token stored in config file"))
}

/// Store the token in the system keychain
pub fn store_token(token: &str) -> Result<()> {
    store_token_in_keychain(token)
}

/// Store token in keychain
pub fn store_token_in_keychain(token: &str) -> Result<()> {
    if !is_keychain_enabled() {
        return Err(anyhow!("Keychain access disabled"));
    }
    let entry = Entry::new(SERVICE_NAME, USERNAME)
        .context("Failed to create keychain entry")?;

    entry
        .set_password(token)
        .map_err(|e| match e {
            keyring::Error::PlatformFailure(ref msg) => {
                anyhow!("Keychain platform error: {}. Try 'fgm auth login' to store in config file instead.", msg)
            }
            keyring::Error::NoStorageAccess(ref msg) => {
                anyhow!("Keychain access denied: {}. Check your system keychain settings.", msg)
            }
            _ => anyhow!("Failed to store token in keychain: {}", e),
        })
}

/// Store token in config file (fallback, less secure)
pub fn store_token_in_config(token: &str) -> Result<()> {
    let mut config = Config::load().context("Failed to load config")?;
    config.set_token(token);
    config.save().context("Failed to save config file")?;
    Ok(())
}

/// Remove the token from the system keychain
pub fn remove_token() -> Result<()> {
    let mut removed_any = false;
    let mut errors = Vec::new();

    // Try to remove from keychain
    if is_keychain_enabled() {
        match remove_token_from_keychain() {
            Ok(_) => removed_any = true,
            Err(e) => errors.push(format!("keychain: {}", e)),
        }
    }

    // Also remove from config file if present
    match remove_token_from_config() {
        Ok(_) => removed_any = true,
        Err(e) => errors.push(format!("config: {}", e)),
    }

    if removed_any {
        Ok(())
    } else if errors.is_empty() {
        Err(anyhow!("No token was stored"))
    } else {
        Err(anyhow!("Failed to remove token: {}", errors.join(", ")))
    }
}

/// Remove token from keychain
pub fn remove_token_from_keychain() -> Result<()> {
    if !is_keychain_enabled() {
        return Err(anyhow!("Keychain access disabled"));
    }
    let entry = Entry::new(SERVICE_NAME, USERNAME)
        .context("Failed to create keychain entry")?;

    entry
        .delete_credential()
        .map_err(|e| match e {
            keyring::Error::NoEntry => anyhow!("No token in keychain"),
            _ => anyhow!("Failed to remove from keychain: {}", e),
        })
}

/// Remove token from config file
pub fn remove_token_from_config() -> Result<()> {
    let mut config = Config::load().context("Failed to load config")?;
    if config.has_token() {
        config.remove_token();
        config.save().context("Failed to save config")?;
        Ok(())
    } else {
        Err(anyhow!("No token in config file"))
    }
}

/// Check if a token exists (in any storage)
pub fn has_token() -> bool {
    get_token().is_ok()
}

/// Check if keychain is accessible (for diagnostics)
pub fn test_keychain_access() -> Result<()> {
    if !is_keychain_enabled() {
        return Err(anyhow!("Keychain access disabled"));
    }
    let entry = Entry::new(SERVICE_NAME, "test_access")
        .context("Failed to create test keychain entry")?;

    // Try to set and immediately delete a test value
    entry
        .set_password("test")
        .context("Failed to write to keychain")?;

    entry
        .delete_credential()
        .context("Failed to delete from keychain")?;

    Ok(())
}

/// Get keychain service and username for debugging
pub fn get_keychain_info() -> (&'static str, &'static str) {
    (SERVICE_NAME, USERNAME)
}
