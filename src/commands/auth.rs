use crate::auth::{
    get_keychain_info, get_token_from_config, get_token_from_keychain, get_token_with_source,
    is_keychain_enabled, remove_token, store_token_in_config, store_token_in_keychain,
    test_keychain_access,
};
use crate::cli::AuthCommands;
use crate::config::Config;
use crate::output;
use anyhow::Result;
use colored::Colorize;
use std::env;
use std::io::{self, Write};

pub async fn run(command: AuthCommands) -> Result<()> {
    match command {
        AuthCommands::Login { keychain } => login(keychain).await,
        AuthCommands::Logout => logout().await,
        AuthCommands::Status => status().await,
        AuthCommands::Debug => debug().await,
    }
}

async fn login(store_in_keychain_only: bool) -> Result<()> {
    output::print_status(&"Figma Personal Access Token Setup".bold().to_string());
    output::print_status("");
    output::print_status("To get a personal access token:");
    output::print_status("  1. Go to https://www.figma.com/developers/api#access-tokens");
    output::print_status("  2. Click 'Get personal access token'");
    output::print_status("  3. Select scopes: file_content:read (required)");
    output::print_status("  4. Copy the generated token");
    output::print_status("");

    if let Err(_) = open::that("https://www.figma.com/developers/api#access-tokens") {
        output::print_warning("Could not open browser automatically.");
    }

    print!("Paste your token: ");
    io::stdout().flush()?;

    let mut token = String::new();
    io::stdin().read_line(&mut token)?;
    let token = token.trim().to_string();

    if token.is_empty() {
        output::print_error("No token provided. Aborting.");
        return Ok(());
    }

    // Validate the token
    print!("Validating token... ");
    io::stdout().flush()?;

    let client = crate::api::FigmaClient::new(token.clone())?;
    if client.validate_token().await? {
        output::print_status(&"valid!".green().to_string());

        if store_in_keychain_only {
            if !is_keychain_enabled() {
                anyhow::bail!("Keychain access is disabled (use without --no-keychain)");
            }
            store_token_in_keychain(&token)?;
            output::print_success("Token stored securely in keychain.");
        } else {
            // Default to config file storage to avoid keychain prompts
            store_token_in_config(&token)?;
            let config_path = Config::config_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "~/.config/fgm/config.toml".to_string());
            output::print_success(&format!("Token stored in config file ({}).", config_path));
            output::print_warning("Config file storage is plaintext. Keep this file secure.");
        }

        match get_token_with_source() {
            Ok(result) => {
                output::print_success(&format!(
                    "Verified: Token accessible from {}.",
                    result.source
                ));
            }
            Err(e) => {
                output::print_warning(&format!(
                    "Could not verify token retrieval: {}",
                    e
                ));
                output::print_status("Run 'fgm auth debug' to diagnose the issue.");
            }
        }
    } else {
        output::print_error("Invalid token. Please check your token and try again.");
    }

    Ok(())
}

async fn logout() -> Result<()> {
    match remove_token() {
        Ok(_) => {
            output::print_success("Token removed.");
        }
        Err(e) => {
            output::print_error(&format!("Failed to remove token: {}", e));
        }
    }
    Ok(())
}

async fn status() -> Result<()> {
    match get_token_with_source() {
        Ok(result) => {
            output::print_status(&"Authenticated".green().bold().to_string());
            output::print_status(&format!("  Source: {}", result.source));

            let client = crate::api::FigmaClient::new(result.token)?;
            if client.validate_token().await? {
                output::print_status(&format!("  Token: {}", "valid".green()));
            } else {
                output::print_status(&format!("  Token: {}", "invalid or expired".red()));
            }
        }
        Err(_) => {
            output::print_status(&"Not authenticated".red().bold().to_string());
            output::print_status("Run 'fgm auth login' to authenticate.");
            output::print_status("");
            output::print_status("Tip: Run 'fgm auth debug' to diagnose authentication issues.");
        }
    }
    Ok(())
}

async fn debug() -> Result<()> {
    output::print_status(&"Authentication Debug Information".bold().to_string());
    output::print_status(&"=".repeat(50));
    output::print_status("");

    let (service, username) = get_keychain_info();
    output::print_status(&"Keychain Configuration:".bold().to_string());
    output::print_status(&format!("  Service name: {}", service));
    output::print_status(&format!("  Account name: {}", username));
    output::print_status("");

    output::print_status(&"1. Environment Variable (FIGMA_TOKEN)".bold().to_string());
    match env::var("FIGMA_TOKEN") {
        Ok(token) if !token.is_empty() => {
            let masked = mask_token(&token);
            output::print_status(&format!("  Status: {}", "SET".green()));
            output::print_status(&format!("  Value:  {}", masked));
        }
        Ok(_) => {
            output::print_status(&format!("  Status: {}", "SET BUT EMPTY".yellow()));
        }
        Err(_) => {
            output::print_status(&format!("  Status: {}", "NOT SET".yellow()));
        }
    }
    output::print_status("");

    output::print_status(&"2. System Keychain".bold().to_string());
    if !is_keychain_enabled() {
        output::print_status(&"  Status: SKIPPED (disabled)".yellow().to_string());
    } else {
        print!("  Access test: ");
        match test_keychain_access() {
            Ok(_) => {
                output::print_status(&"PASSED".green().to_string());
            }
            Err(e) => {
                output::print_status(&"FAILED".red().to_string());
                output::print_status(&format!("  Error: {}", e));
            }
        }

        print!("  Token lookup: ");
        match get_token_from_keychain() {
            Ok(token) => {
                let masked = mask_token(&token);
                output::print_status(&"FOUND".green().to_string());
                output::print_status(&format!("  Value: {}", masked));
            }
            Err(e) => {
                output::print_status(&"NOT FOUND".yellow().to_string());
                output::print_status(&format!("  Details: {}", e));
            }
        }
    }
    output::print_status("");

    output::print_status(&"3. Config File".bold().to_string());
    match Config::config_path() {
        Some(path) => {
            output::print_status(&format!("  Path: {}", path.display()));
            if path.exists() {
                output::print_status(&format!("  File: {}", "EXISTS".green()));
                match get_token_from_config() {
                    Ok(token) => {
                        let masked = mask_token(&token);
                        output::print_status(&format!(
                            "  Token: {} ({})",
                            "FOUND".green(),
                            masked
                        ));
                    }
                    Err(_) => {
                        output::print_status(&format!("  Token: {}", "NOT SET".yellow()));
                    }
                }
            } else {
                output::print_status(&format!("  File: {}", "DOES NOT EXIST".yellow()));
            }
        }
        None => {
            output::print_status(&format!("  Path: {}", "COULD NOT DETERMINE".red()));
        }
    }
    output::print_status("");

    output::print_status(&"Summary".bold().to_string());
    output::print_status(&"-".repeat(50));
    match get_token_with_source() {
        Ok(result) => {
            let masked = mask_token(&result.token);
            output::print_status(&format!(
                "  Active token: {} (from {})",
                masked, result.source
            ));
            output::print_status(&format!("  Status: {}", "READY".green().bold()));
        }
        Err(e) => {
            output::print_status(&format!("  Active token: {}", "NONE".red()));
            output::print_status(&format!("  Error: {}", e));
            output::print_status(&format!(
                "  Status: {}",
                "NOT AUTHENTICATED".red().bold()
            ));
            output::print_status("");
            output::print_status(&"Troubleshooting:".bold().to_string());
            output::print_status("  1. Run 'fgm auth login' to store a new token");
            output::print_status("  2. Or set FIGMA_TOKEN environment variable");
            output::print_status("  3. Check macOS Keychain Access app for 'fgm' entries");
            output::print_status("");
            output::print_status(&"Manual keychain check:".bold().to_string());
            output::print_status(&format!(
                "    security find-generic-password -s \"{}\" -a \"{}\" 2>&1",
                service, username
            ));
        }
    }
    output::print_status("");

    Ok(())
}

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
