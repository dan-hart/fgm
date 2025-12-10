use crate::auth::{get_token, remove_token, store_token};
use crate::cli::AuthCommands;
use anyhow::Result;
use colored::Colorize;
use std::io::{self, Write};

pub async fn run(command: AuthCommands) -> Result<()> {
    match command {
        AuthCommands::Login => login().await,
        AuthCommands::Logout => logout().await,
        AuthCommands::Status => status().await,
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

        // Store the token
        store_token(&token)?;
        println!("{}", "Token stored securely in keychain.".green());
    } else {
        println!("{}", "invalid!".red());
        println!("Please check your token and try again.");
    }

    Ok(())
}

async fn logout() -> Result<()> {
    match remove_token() {
        Ok(_) => {
            println!("{}", "Token removed from keychain.".green());
        }
        Err(e) => {
            println!("{}: {}", "Failed to remove token".red(), e);
        }
    }
    Ok(())
}

async fn status() -> Result<()> {
    match get_token() {
        Ok(token) => {
            println!("{}", "Authenticated".green().bold());

            // Validate and show info
            let client = crate::api::FigmaClient::new(token)?;
            if client.validate_token().await? {
                println!("  Token: {}", "valid".green());
            } else {
                println!("  Token: {}", "invalid or expired".red());
            }
        }
        Err(_) => {
            println!("{}", "Not authenticated".red().bold());
            println!("Run 'fgm auth login' to authenticate.");
        }
    }
    Ok(())
}
