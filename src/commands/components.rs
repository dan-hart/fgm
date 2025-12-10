use crate::api::FigmaClient;
use crate::auth::get_token;
use crate::cli::ComponentsCommands;
use anyhow::Result;
use colored::Colorize;

pub async fn run(command: ComponentsCommands) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    match command {
        ComponentsCommands::List { team_id } => list(&client, &team_id).await,
        ComponentsCommands::Get { component_key } => get(&client, &component_key).await,
    }
}

async fn list(client: &FigmaClient, team_id: &str) -> Result<()> {
    println!("{}", format!("Components in team {}:", team_id).bold());
    println!("{}", "Note: Team components API requires published library".yellow());
    let _ = client; // Suppress unused warning
    Ok(())
}

async fn get(client: &FigmaClient, component_key: &str) -> Result<()> {
    println!("{}", format!("Component: {}", component_key).bold());
    println!("{}", "Note: Component details API coming soon".yellow());
    let _ = client; // Suppress unused warning
    Ok(())
}
