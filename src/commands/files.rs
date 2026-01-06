use crate::api::{FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::FilesCommands;
use crate::output;
use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use tabled::Tabled;

pub async fn run(command: FilesCommands) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    match command {
        FilesCommands::List { project, team } => list(&client, project, team).await,
        FilesCommands::Get { file_key_or_url } => {
            let parsed = FigmaUrl::parse(&file_key_or_url)?;
            get(&client, &parsed.file_key).await
        }
        FilesCommands::Tree { file_key_or_url, depth } => {
            let parsed = FigmaUrl::parse(&file_key_or_url)?;
            tree(&client, &parsed.file_key, depth).await
        }
        FilesCommands::Versions { file_key_or_url, limit } => {
            let parsed = FigmaUrl::parse(&file_key_or_url)?;
            versions(&client, &parsed.file_key, limit).await
        }
    }
}

async fn list(client: &FigmaClient, project: Option<String>, team: Option<String>) -> Result<()> {
    if let Some(project_id) = project {
        let files = client.get_project_files(&project_id).await?;
        let rows: Vec<FileRow> = files
            .files
            .into_iter()
            .map(|file| FileRow {
                key: file.key,
                name: file.name,
                last_modified: file.last_modified,
            })
            .collect();

        if output::format() == crate::output::OutputFormat::Json {
            let out = FilesListOutput {
                project_id,
                files: rows,
            };
            output::print_json(&out)?;
        } else {
            output::print_status(&format!("Files in project {}:", project_id).bold().to_string());
            output::print_table(&rows);
        }
    } else if let Some(team_id) = team {
        let projects = client.get_team_projects(&team_id).await?;
        let rows: Vec<ProjectRow> = projects
            .projects
            .into_iter()
            .map(|project| ProjectRow {
                id: project.id,
                name: project.name,
            })
            .collect();

        if output::format() == crate::output::OutputFormat::Json {
            let out = ProjectsListOutput {
                team_id,
                projects: rows,
            };
            output::print_json(&out)?;
        } else {
            output::print_status(&format!("Projects in team {}:", team_id).bold().to_string());
            output::print_table(&rows);
        }
    } else {
        anyhow::bail!("Please specify --project or --team");
    }
    Ok(())
}

async fn get(client: &FigmaClient, file_key: &str) -> Result<()> {
    let file = client.get_file(file_key).await?;
    let info = FileInfo {
        name: file.name,
        last_modified: file.last_modified,
        version: file.version,
        components: file.components.len(),
        styles: file.styles.len(),
    };

    if output::format() == crate::output::OutputFormat::Json {
        output::print_json(&info)?;
    } else {
        output::print_status(&format!("File: {}", info.name).bold().to_string());
        output::print_status(&format!("  Last modified: {}", info.last_modified));
        output::print_status(&format!("  Version: {}", info.version));
        output::print_status(&format!("  Components: {}", info.components));
        output::print_status(&format!("  Styles: {}", info.styles));
    }
    Ok(())
}

async fn tree(client: &FigmaClient, file_key: &str, depth: u32) -> Result<()> {
    let file = client.get_file(file_key).await?;
    if output::format() == crate::output::OutputFormat::Json {
        let tree = TreeNode::from_document(&file.document, 0, depth);
        output::print_json(&tree)?;
    } else {
        output::print_status(&format!("Node tree for: {}", file.name).bold().to_string());
        print_node(
            &file.document.name,
            &file.document.id,
            &file.document.node_type,
            0,
            depth,
            &file.document.children,
        );
    }
    Ok(())
}

fn print_node(name: &str, id: &str, node_type: &str, current_depth: u32, max_depth: u32, children: &Option<Vec<crate::api::types::Node>>) {
    let indent = "  ".repeat(current_depth as usize);
    output::print_status(&format!(
        "{}{} ({}) [{}]",
        indent,
        name,
        node_type.dimmed(),
        id.cyan()
    ));

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
    let rows: Vec<VersionRow> = versions
        .versions
        .into_iter()
        .take(limit as usize)
        .map(|version| VersionRow {
            id: version.id,
            label: version.label.unwrap_or_else(|| "(no label)".to_string()),
            user: version
                .user
                .map(|u| u.handle)
                .unwrap_or_else(|| "unknown".to_string()),
            created_at: version.created_at,
            description: version.description.unwrap_or_default(),
        })
        .collect();

    if output::format() == crate::output::OutputFormat::Json {
        let out = VersionsOutput {
            file_key: file_key.to_string(),
            versions: rows,
        };
        output::print_json(&out)?;
    } else {
        output::print_status(&"Version history:".bold().to_string());
        output::print_table(&rows);
    }
    Ok(())
}

#[derive(Serialize)]
struct FilesListOutput {
    project_id: String,
    files: Vec<FileRow>,
}

#[derive(Serialize)]
struct ProjectsListOutput {
    team_id: String,
    projects: Vec<ProjectRow>,
}

#[derive(Serialize)]
struct VersionsOutput {
    file_key: String,
    versions: Vec<VersionRow>,
}

#[derive(Serialize)]
struct FileInfo {
    name: String,
    last_modified: String,
    version: String,
    components: usize,
    styles: usize,
}

#[derive(Tabled, Serialize)]
struct FileRow {
    key: String,
    name: String,
    last_modified: String,
}

#[derive(Tabled, Serialize)]
struct ProjectRow {
    id: String,
    name: String,
}

#[derive(Tabled, Serialize)]
struct VersionRow {
    id: String,
    label: String,
    user: String,
    created_at: String,
    description: String,
}

#[derive(Serialize)]
struct TreeNode {
    name: String,
    id: String,
    node_type: String,
    children: Vec<TreeNode>,
}

impl TreeNode {
    fn from_document(
        document: &crate::api::types::Document,
        current_depth: u32,
        max_depth: u32,
    ) -> Self {
        let children = if current_depth < max_depth {
            document
                .children
                .as_ref()
                .map(|nodes| {
                    nodes
                        .iter()
                        .map(|node| TreeNode::from_node(node, current_depth + 1, max_depth))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        TreeNode {
            name: document.name.clone(),
            id: document.id.clone(),
            node_type: document.node_type.clone(),
            children,
        }
    }

    fn from_node(
        node: &crate::api::types::Node,
        current_depth: u32,
        max_depth: u32,
    ) -> Self {
        let children = if current_depth < max_depth {
            node.children
                .as_ref()
                .map(|nodes| {
                    nodes
                        .iter()
                        .map(|child| TreeNode::from_node(child, current_depth + 1, max_depth))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        TreeNode {
            name: node.name.clone(),
            id: node.id.clone(),
            node_type: node.node_type.clone(),
            children,
        }
    }
}
