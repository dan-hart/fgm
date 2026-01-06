use crate::api::FigmaClient;
use crate::auth::get_token;
use crate::cli::ComponentsCommands;
use crate::output;
use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use tabled::Tabled;

pub async fn run(command: ComponentsCommands) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    match command {
        ComponentsCommands::List { team_id } => list(&client, &team_id).await,
        ComponentsCommands::Get { component_key } => get(&client, &component_key).await,
    }
}

async fn list(client: &FigmaClient, team_id: &str) -> Result<()> {
    output::print_status(&format!("Published components in team {}:", team_id).bold().to_string());

    let response = client.get_team_components(team_id).await?;

    if let Some(msg) = &response.message {
        if response.status == Some(403) || response.status == Some(404) {
            anyhow::bail!(
                "{} (this endpoint requires the team to have a published library)",
                msg
            );
        }
    }

    if let Some(meta) = response.meta {
        if meta.components.is_empty() {
            output::print_warning("No published components found");
            return Ok(());
        }

        let rows: Vec<ComponentRow> = meta
            .components
            .iter()
            .map(|component| ComponentRow {
                name: component.name.clone(),
                key: component.key.clone(),
                file_key: component.file_key.clone(),
                node_id: component.node_id.clone(),
                updated_at: component.updated_at.clone(),
            })
            .collect();

        if output::format() == crate::output::OutputFormat::Json {
            let out = ComponentsListOutput {
                team_id: team_id.to_string(),
                components: rows,
            };
            output::print_json(&out)?;
        } else {
            output::print_table(&rows);
            output::print_status(&format!(
                "Total: {} components",
                meta.components.len()
            )
            .bold()
            .to_string());
        }
    } else {
        output::print_warning("No component data returned");
    }

    Ok(())
}

async fn get(client: &FigmaClient, component_key: &str) -> Result<()> {
    output::print_status(&format!("Component: {}", component_key).bold().to_string());

    let response = client.get_component(component_key).await?;

    if let Some(msg) = &response.message {
        if response.status == Some(404) {
            anyhow::bail!("{}", msg);
        }
    }

    if let Some(meta) = response.meta {
        let detail = ComponentDetail {
            key: component_key.to_string(),
            name: meta.name.clone(),
            description: if meta.description.is_empty() {
                None
            } else {
                Some(meta.description.clone())
            },
            file_key: meta.file_key.clone(),
            node_id: meta.node_id.clone(),
            frame: meta
                .containing_frame
                .as_ref()
                .and_then(|frame| frame.name.clone()),
            page: meta
                .containing_frame
                .as_ref()
                .and_then(|frame| frame.page_name.clone()),
            created_at: meta.created_at.clone(),
            updated_at: meta.updated_at.clone(),
            thumbnail_url: meta.thumbnail_url.clone(),
        };

        if output::format() == crate::output::OutputFormat::Json {
            output::print_json(&detail)?;
        } else {
            output::print_status("");
            output::print_status(&format!("  Name: {}", detail.name.cyan()));
            if let Some(desc) = &detail.description {
                output::print_status(&format!("  Description: {}", desc));
            }
            output::print_status(&format!("  File: {}", detail.file_key));
            output::print_status(&format!("  Node ID: {}", detail.node_id));
            if let Some(frame) = &detail.frame {
                output::print_status(&format!("  Frame: {}", frame));
            }
            if let Some(page) = &detail.page {
                output::print_status(&format!("  Page: {}", page));
            }
            if let Some(created) = &detail.created_at {
                output::print_status(&format!("  Created: {}", created));
            }
            if let Some(updated) = &detail.updated_at {
                output::print_status(&format!("  Updated: {}", updated));
            }
            if let Some(thumb) = &detail.thumbnail_url {
                output::print_status(&format!("  Thumbnail: {}", thumb.dimmed()));
            }
        }
    } else {
        output::print_warning("No component data returned");
    }

    Ok(())
}

#[derive(Tabled, Serialize)]
struct ComponentRow {
    name: String,
    key: String,
    file_key: String,
    node_id: String,
    updated_at: String,
}

#[derive(Serialize)]
struct ComponentsListOutput {
    team_id: String,
    components: Vec<ComponentRow>,
}

#[derive(Serialize)]
struct ComponentDetail {
    key: String,
    name: String,
    description: Option<String>,
    file_key: String,
    node_id: String,
    frame: Option<String>,
    page: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    thumbnail_url: Option<String>,
}
