use crate::auth::{
    get_token_from_config, get_token_from_keychain, get_token_with_source,
    get_keychain_info, remove_token, store_token_in_config, store_token_in_keychain,
    test_keychain_access,
};
use crate::cli::AuthCommands;
use crate::config::Config;
use anyhow::Result;
use colored::Colorize;
use std::env;
use std::io::{self, Write};

pub async fn run(command: AuthCommands) -> Result<()> {
    match command {
        AuthCommands::Login => login().await,
        AuthCommands::Logout => logout().await,
        AuthCommands::Status => status().await,
        AuthCommands::Debug => debug().await,
    }
}

async fn login() -> Result<()> {
    println!("{}", "Figma Personal Access Token Setup".bold());
    println!();
    println!("To get a personal access token:");
    println!("  1. Go to https://www.figma.com/developers/api#access-tokens");
    println!("  2. Click 'Get personal access token'");
    println!("  3. Select scopes: file_content:read (required)");
    println!("  4. Copy the generated token");
    println!();

    // Try to open the browser
    if let Err(_) = open::that("https://www.figma.com/developers/api#access-tokens") {
        println!("{}", "Could not open browser automatically.".yellow());
    }

    print!("Paste your token: ");
    io::stdout().flush()?;

    let mut token = String::new();
    io::stdin().read_line(&mut token)?;
    let token = token.trim().to_string();

    if token.is_empty() {
        println!("{}", "No token provided. Aborting.".red());
        return Ok(());
    }

    // Validate the token
    print!("Validating token... ");
    io::stdout().flush()?;

    let client = crate::api::FigmaClient::new(token.clone())?;
    if client.validate_token().await? {
        println!("{}", "valid!".green());

        // Try to store in keychain first
        match store_token_in_keychain(&token) {
            Ok(_) => {
                println!("{}", "Token stored securely in keychain.".green());
            }
            Err(e) => {
                println!("{}", format!("Keychain storage failed: {}", e).yellow());
                println!("Falling back to config file storage...");

                // Fall back to config file
                match store_token_in_config(&token) {
                    Ok(_) => {
                        println!(
                            "{}",
                            "Token stored in config file (~/.config/fgm/config.toml).".green()
                        );
                        println!(
                            "{}",
                            "Warning: Config file storage is less secure than keychain.".yellow()
                        );
                    }
                    Err(e) => {
                        println!("{}: {}", "Failed to store token".red(), e);
                        println!("You can set FIGMA_TOKEN environment variable as an alternative.");
                        return Err(e);
                    }
                }
            }
        }

        // Verify we can retrieve it
        match get_token_with_source() {
            Ok(result) => {
                println!(
                    "{}",
                    format!("Verified: Token accessible from {}.", result.source).green()
                );
            }
            Err(e) => {
                println!(
                    "{}",
                    format!("Warning: Could not verify token retrieval: {}", e).yellow()
                );
                println!("Run 'fgm auth debug' to diagnose the issue.");
            }
        }
    } else {
        println!("{}", "invalid!".red());
        println!("Please check your token and try again.");
    }

    Ok(())
}

async fn logout() -> Result<()> {
    match remove_token() {
        Ok(_) => {
            println!("{}", "Token removed.".green());
        }
        Err(e) => {
            println!("{}: {}", "Failed to remove token".red(), e);
        }
    }
    Ok(())
}

async fn status() -> Result<()> {
    match get_token_with_source() {
        Ok(result) => {
            println!("{}", "Authenticated".green().bold());
            println!("  Source: {}", result.source);

            // Validate and show info
            let client = crate::api::FigmaClient::new(result.token)?;
            if client.validate_token().await? {
                println!("  Token: {}", "valid".green());
            } else {
                println!("  Token: {}", "invalid or expired".red());
            }
        }
        Err(_) => {
            println!("{}", "Not authenticated".red().bold());
            println!("Run 'fgm auth login' to authenticate.");
            println!();
            println!("Tip: Run 'fgm auth debug' to diagnose authentication issues.");
        }
    }
    Ok(())
}

async fn debug() -> Result<()> {
    println!("{}", "Authentication Debug Information".bold());
    println!("{}", "=".repeat(50));
    println!();

    // Get keychain info
    let (service, username) = get_keychain_info();
    println!("{}", "Keychain Configuration:".bold());
    println!("  Service name: {}", service);
    println!("  Account name: {}", username);
    println!();

    // Check environment variable
    println!("{}", "1. Environment Variable (FIGMA_TOKEN)".bold());
    match env::var("FIGMA_TOKEN") {
        Ok(token) if !token.is_empty() => {
            let masked = mask_token(&token);
            println!("  Status: {}", "SET".green());
            println!("  Value:  {}", masked);
        }
        Ok(_) => {
            println!("  Status: {}", "SET BUT EMPTY".yellow());
        }
        Err(_) => {
            println!("  Status: {}", "NOT SET".yellow());
        }
    }
    println!();

    // Check keychain
    println!("{}", "2. System Keychain".bold());

    // First test keychain access
    print!("  Access test: ");
    match test_keychain_access() {
        Ok(_) => {
            println!("{}", "PASSED".green());
        }
        Err(e) => {
            println!("{}", "FAILED".red());
            println!("  Error: {}", e);
        }
    }

    // Then check for stored token
    print!("  Token lookup: ");
    match get_token_from_keychain() {
        Ok(token) => {
            let masked = mask_token(&token);
            println!("{}", "FOUND".green());
            println!("  Value: {}", masked);
        }
        Err(e) => {
            println!("{}", "NOT FOUND".yellow());
            println!("  Details: {}", e);
        }
    }
    println!();

    // Check config file
    println!("{}", "3. Config File".bold());
    match Config::config_path() {
        Some(path) => {
            println!("  Path: {}", path.display());
            if path.exists() {
                println!("  File: {}", "EXISTS".green());
                match get_token_from_config() {
                    Ok(token) => {
                        let masked = mask_token(&token);
                        println!("  Token: {} ({})", "FOUND".green(), masked);
                    }
                    Err(_) => {
                        println!("  Token: {}", "NOT SET".yellow());
                    }
                }
            } else {
                println!("  File: {}", "DOES NOT EXIST".yellow());
            }
        }
        None => {
            println!("  Path: {}", "COULD NOT DETERMINE".red());
        }
    }
    println!();

    // Overall status
    println!("{}", "Summary".bold());
    println!("{}", "-".repeat(50));
    match get_token_with_source() {
        Ok(result) => {
            let masked = mask_token(&result.token);
            println!("  Active token: {} (from {})", masked, result.source);
            println!("  Status: {}", "READY".green().bold());
        }
        Err(e) => {
            println!("  Active token: {}", "NONE".red());
            println!("  Error: {}", e);
            println!("  Status: {}", "NOT AUTHENTICATED".red().bold());
            println!();
            println!("{}", "Troubleshooting:".bold());
            println!("  1. Run 'fgm auth login' to store a new token");
            println!("  2. Or set FIGMA_TOKEN environment variable");
            println!("  3. Check macOS Keychain Access app for 'fgm' entries");
            println!();
            println!("  Manual keychain check:");
            println!(
                "    security find-generic-password -s \"{}\" -a \"{}\" 2>&1",
                service, username
            );
        }
    }
    println!();

    Ok(())
}

/// Mask a token for display (show first 8 and last 4 chars)
fn mask_token(token: &str) -> String {
    if token.len() <= 12 {
        return "*".repeat(token.len());
    }
    format!(
        "{}...{}",
        &token[..8],
        &token[token.len() - 4..]
    )
}
