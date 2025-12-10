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
    println!("{}", format!("Published components in team {}:", team_id).bold());

    let response = client.get_team_components(team_id).await?;

    if let Some(msg) = &response.message {
        if response.status == Some(403) || response.status == Some(404) {
            println!("{}: {}", "Error".red(), msg);
            println!("{}", "Note: This endpoint requires the team to have a published library.".yellow());
            return Ok(());
        }
    }

    if let Some(meta) = response.meta {
        if meta.components.is_empty() {
            println!("{}", "No published components found".yellow());
            return Ok(());
        }

        for component in &meta.components {
            println!();
            println!("  {} ({})", component.name.cyan(), component.key.dimmed());
            if !component.description.is_empty() {
                println!("    {}", component.description);
            }
            println!("    File: {} | Node: {}", component.file_key, component.node_id);
            println!("    Updated: {}", component.updated_at);
        }
        println!();
        println!("{}", format!("Total: {} components", meta.components.len()).bold());
    } else {
        println!("{}", "No component data returned".yellow());
    }

    Ok(())
}

async fn get(client: &FigmaClient, component_key: &str) -> Result<()> {
    println!("{}", format!("Component: {}", component_key).bold());

    let response = client.get_component(component_key).await?;

    if let Some(msg) = &response.message {
        if response.status == Some(404) {
            println!("{}: {}", "Error".red(), msg);
            return Ok(());
        }
    }

    if let Some(meta) = response.meta {
        println!();
        println!("  Name: {}", meta.name.cyan());
        if !meta.description.is_empty() {
            println!("  Description: {}", meta.description);
        }
        println!("  File: {}", meta.file_key);
        println!("  Node ID: {}", meta.node_id);

        if let Some(frame) = &meta.containing_frame {
            if let Some(name) = &frame.name {
                println!("  Frame: {}", name);
            }
            if let Some(page) = &frame.page_name {
                println!("  Page: {}", page);
            }
        }

        if let Some(created) = &meta.created_at {
            println!("  Created: {}", created);
        }
        if let Some(updated) = &meta.updated_at {
            println!("  Updated: {}", updated);
        }

        if let Some(thumb) = &meta.thumbnail_url {
            println!("  Thumbnail: {}", thumb.dimmed());
        }
    } else {
        println!("{}", "No component data returned".yellow());
    }

    Ok(())
}
