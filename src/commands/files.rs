use crate::api::FigmaClient;
use crate::auth::get_token;
use crate::cli::FilesCommands;
use anyhow::Result;
use colored::Colorize;

pub async fn run(command: FilesCommands) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    match command {
        FilesCommands::List { project, team } => list(&client, project, team).await,
        FilesCommands::Get { file_key } => get(&client, &file_key).await,
        FilesCommands::Tree { file_key, depth } => tree(&client, &file_key, depth).await,
        FilesCommands::Versions { file_key, limit } => versions(&client, &file_key, limit).await,
    }
}

async fn list(client: &FigmaClient, project: Option<String>, team: Option<String>) -> Result<()> {
    if let Some(project_id) = project {
        let files = client.get_project_files(&project_id).await?;
        println!("{}", format!("Files in project {}:", project_id).bold());
        for file in files.files {
            println!("  {} - {}", file.key.cyan(), file.name);
        }
    } else if let Some(team_id) = team {
        let projects = client.get_team_projects(&team_id).await?;
        println!("{}", format!("Projects in team {}:", team_id).bold());
        for project in projects.projects {
            println!("  {} - {}", project.id.cyan(), project.name);
        }
    } else {
        println!("{}", "Please specify --project or --team".yellow());
    }
    Ok(())
}

async fn get(client: &FigmaClient, file_key: &str) -> Result<()> {
    let file = client.get_file(file_key).await?;
    println!("{}", format!("File: {}", file.name).bold());
    println!("  Last modified: {}", file.last_modified);
    println!("  Version: {}", file.version);
    println!("  Components: {}", file.components.len());
    println!("  Styles: {}", file.styles.len());
    Ok(())
}

async fn tree(client: &FigmaClient, file_key: &str, depth: u32) -> Result<()> {
    let file = client.get_file(file_key).await?;
    println!("{}", format!("Node tree for: {}", file.name).bold());
    print_node(&file.document.name, &file.document.id, &file.document.node_type, 0, depth, &file.document.children);
    Ok(())
}

fn print_node(name: &str, id: &str, node_type: &str, current_depth: u32, max_depth: u32, children: &Option<Vec<crate::api::types::Node>>) {
    let indent = "  ".repeat(current_depth as usize);
    println!("{}{} ({}) [{}]", indent, name, node_type.dimmed(), id.cyan());

    if current_depth < max_depth {
        if let Some(children) = children {
            for child in children {
                print_node(&child.name, &child.id, &child.node_type, current_depth + 1, max_depth, &child.children);
            }
        }
    }
}

async fn versions(client: &FigmaClient, file_key: &str, limit: u32) -> Result<()> {
    let versions = client.get_versions(file_key).await?;
    println!("{}", "Version history:".bold());
    for (i, version) in versions.versions.iter().take(limit as usize).enumerate() {
        let label = version.label.as_deref().unwrap_or("(no label)");
        let user = version.user.as_ref().map(|u| u.handle.as_str()).unwrap_or("unknown");
        println!("  {}. {} - {} by {}", i + 1, version.id.cyan(), label, user.dimmed());
        println!("     Created: {}", version.created_at);
        if let Some(desc) = &version.description {
            if !desc.is_empty() {
                println!("     {}", desc);
            }
        }
    }
    Ok(())
}
