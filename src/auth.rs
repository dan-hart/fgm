use anyhow::{anyhow, Result};
use keyring::Entry;
use std::env;

const SERVICE_NAME: &str = "fgm";
const USERNAME: &str = "figma_token";

/// Get the Figma token from environment variable or keychain
pub fn get_token() -> Result<String> {
    // First check environment variable
    if let Ok(token) = env::var("FIGMA_TOKEN") {
        return Ok(token);
    }

    // Then check keychain
    let entry = Entry::new(SERVICE_NAME, USERNAME)?;
    entry
        .get_password()
        .map_err(|e| anyhow!("No token found: {}", e))
}

/// Store the token in the system keychain
pub fn store_token(token: &str) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, USERNAME)?;
    entry
        .set_password(token)
        .map_err(|e| anyhow!("Failed to store token: {}", e))
}

/// Remove the token from the system keychain
pub fn remove_token() -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, USERNAME)?;
    entry
        .delete_credential()
        .map_err(|e| anyhow!("Failed to remove token: {}", e))
}

/// Check if a token exists
pub fn has_token() -> bool {
    get_token().is_ok()
}
