use crate::api::FigmaClient;
use crate::auth::get_token;
use crate::cli::TokensCommands;
use anyhow::Result;
use colored::Colorize;

pub async fn run(command: TokensCommands) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    match command {
        TokensCommands::Colors { file_key } => colors(&client, &file_key).await,
        TokensCommands::Typography { file_key } => typography(&client, &file_key).await,
        TokensCommands::Spacing { file_key } => spacing(&client, &file_key).await,
        TokensCommands::Export {
            file_key,
            format,
            output,
        } => export(&client, &file_key, format, output).await,
    }
}

async fn colors(client: &FigmaClient, file_key: &str) -> Result<()> {
    let file = client.get_file(file_key).await?;
    println!("{}", "Color Styles:".bold());

    for (key, style) in &file.styles {
        if style.style_type == "FILL" {
            println!("  {} ({})", style.name.cyan(), key.dimmed());
        }
    }

    // TODO: Extract actual color values from nodes
    println!();
    println!("{}", "Note: Full color extraction coming in Phase 4".yellow());
    Ok(())
}

async fn typography(client: &FigmaClient, file_key: &str) -> Result<()> {
    let file = client.get_file(file_key).await?;
    println!("{}", "Typography Styles:".bold());

    for (key, style) in &file.styles {
        if style.style_type == "TEXT" {
            println!("  {} ({})", style.name.cyan(), key.dimmed());
        }
    }

    // TODO: Extract actual typography values
    println!();
    println!("{}", "Note: Full typography extraction coming in Phase 4".yellow());
    Ok(())
}

async fn spacing(client: &FigmaClient, file_key: &str) -> Result<()> {
    println!("{}", "Spacing values from auto-layout:".bold());
    println!("{}", "Note: Spacing extraction coming in Phase 4".yellow());
    let _ = (client, file_key); // Suppress unused warnings
    Ok(())
}

async fn export(
    client: &FigmaClient,
    file_key: &str,
    format: crate::cli::TokenFormat,
    output: Option<std::path::PathBuf>,
) -> Result<()> {
    println!("{}", "Exporting design tokens...".bold());
    println!("{}", "Note: Full token export coming in Phase 4".yellow());
    let _ = (client, file_key, format, output); // Suppress unused warnings
    Ok(())
}
