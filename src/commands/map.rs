use crate::api::{FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::MapCommands;
use anyhow::Result;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub async fn run(command: MapCommands) -> Result<()> {
    match command {
        MapCommands::Init { file_key_or_url, output } => init(&file_key_or_url, &output).await,
        MapCommands::Coverage { map } => coverage(&map),
        MapCommands::Update { map } => update(&map).await,
        MapCommands::Link { component, code_path, map } => link(&component, &code_path, &map),
    }
}

/// Component map file format
#[derive(Serialize, Deserialize)]
struct ComponentMap {
    /// Figma file information
    figma: FigmaSource,
    /// Mapping of Figma components to code
    #[serde(default)]
    components: HashMap<String, ComponentEntry>,
}

#[derive(Serialize, Deserialize)]
struct FigmaSource {
    file_key: String,
    file_name: String,
    last_sync: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct ComponentEntry {
    /// Figma node ID
    node_id: String,
    /// Component name from Figma
    figma_name: String,
    /// Path to code implementation (if linked)
    #[serde(default)]
    code_path: Option<String>,
    /// Implementation status
    #[serde(default)]
    status: ComponentStatus,
    /// Notes
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ComponentStatus {
    #[default]
    NotStarted,
    InProgress,
    Implemented,
    NeedsUpdate,
}

impl std::fmt::Display for ComponentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentStatus::NotStarted => write!(f, "not_started"),
            ComponentStatus::InProgress => write!(f, "in_progress"),
            ComponentStatus::Implemented => write!(f, "implemented"),
            ComponentStatus::NeedsUpdate => write!(f, "needs_update"),
        }
    }
}

/// Initialize a component map from a Figma file
async fn init(file_key_or_url: &str, output: &Path) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    let parsed = FigmaUrl::parse(file_key_or_url)?;
    let file_key = &parsed.file_key;

    println!("{}", "Fetching Figma file...".bold());

    let file = client.get_file(file_key).await?;

    // Extract all components from the document
    let mut components = HashMap::new();
    extract_components(&file.document, &mut components);

    // Also include components from the file's component metadata
    for (key, comp) in &file.components {
        if !components.contains_key(key) {
            components.insert(
                key.clone(),
                ComponentEntry {
                    node_id: key.clone(),
                    figma_name: comp.name.clone(),
                    code_path: None,
                    status: ComponentStatus::NotStarted,
                    notes: if comp.description.is_empty() {
                        None
                    } else {
                        Some(comp.description.clone())
                    },
                },
            );
        }
    }

    let map = ComponentMap {
        figma: FigmaSource {
            file_key: file_key.to_string(),
            file_name: file.name.clone(),
            last_sync: chrono::Utc::now().to_rfc3339(),
        },
        components,
    };

    let toml_content = toml::to_string_pretty(&map)?;
    fs::write(output, &toml_content)?;

    println!();
    println!("{}", format!("Created component map: {}", output.display()).green());
    println!("  File: {} ({})", file.name, file_key);
    println!("  Components: {}", map.components.len());
    println!();
    println!("{}", "Next steps:".bold());
    println!("  1. Review the generated file");
    println!("  2. Link components to code: fgm map link <component> <code-path>");
    println!("  3. Check coverage: fgm map coverage");

    Ok(())
}

/// Show implementation coverage
fn coverage(map_path: &Path) -> Result<()> {
    let content = fs::read_to_string(map_path)?;
    let map: ComponentMap = toml::from_str(&content)?;

    println!("{}", format!("Component Coverage: {}", map.figma.file_name).bold());
    println!("  Last synced: {}", map.figma.last_sync);
    println!();

    let total = map.components.len();
    let implemented: Vec<_> = map.components.values()
        .filter(|c| c.status == ComponentStatus::Implemented)
        .collect();
    let in_progress: Vec<_> = map.components.values()
        .filter(|c| c.status == ComponentStatus::InProgress)
        .collect();
    let needs_update: Vec<_> = map.components.values()
        .filter(|c| c.status == ComponentStatus::NeedsUpdate)
        .collect();
    let not_started: Vec<_> = map.components.values()
        .filter(|c| c.status == ComponentStatus::NotStarted)
        .collect();

    // Coverage bar
    let pct = if total > 0 { (implemented.len() * 100) / total } else { 0 };
    let bar_width = 30;
    let filled = (pct * bar_width) / 100;
    let bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);
    println!("  [{}] {}%", bar.green(), pct);
    println!();

    // Summary
    println!("{}", "Status:".bold());
    println!("  {} Implemented ({}/{})", "✓".green(), implemented.len(), total);
    println!("  {} In Progress ({})", "→".yellow(), in_progress.len());
    println!("  {} Needs Update ({})", "!".red(), needs_update.len());
    println!("  {} Not Started ({})", "○".dimmed(), not_started.len());

    // List non-implemented components
    if !not_started.is_empty() || !needs_update.is_empty() {
        println!();
        println!("{}", "Pending:".bold());

        for comp in needs_update.iter().chain(not_started.iter()).take(10) {
            let status_icon = match comp.status {
                ComponentStatus::NeedsUpdate => "!".red(),
                _ => "○".dimmed(),
            };
            println!("  {} {}", status_icon, comp.figma_name);
        }

        let remaining = not_started.len() + needs_update.len();
        if remaining > 10 {
            println!("  ... and {} more", remaining - 10);
        }
    }

    // Verify linked files exist
    let mut broken_links = Vec::new();
    for comp in implemented.iter().chain(in_progress.iter()) {
        if let Some(path) = &comp.code_path {
            if !Path::new(path).exists() {
                broken_links.push((&comp.figma_name, path));
            }
        }
    }

    if !broken_links.is_empty() {
        println!();
        println!("{}", "⚠ Broken Links:".yellow().bold());
        for (name, path) in broken_links {
            println!("  {} → {}", name, path.red());
        }
    }

    Ok(())
}

/// Update map with latest components from Figma
async fn update(map_path: &Path) -> Result<()> {
    let content = fs::read_to_string(map_path)?;
    let mut map: ComponentMap = toml::from_str(&content)?;

    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    println!("{}", "Updating from Figma...".bold());

    let file = client.get_file(&map.figma.file_key).await?;

    // Extract current components
    let mut new_components = HashMap::new();
    extract_components(&file.document, &mut new_components);

    // Merge: keep existing entries, add new ones, mark removed
    let mut added = 0;
    let mut removed = 0;

    for (key, entry) in new_components {
        if !map.components.contains_key(&key) {
            map.components.insert(key, entry);
            added += 1;
        }
    }

    // Mark components that no longer exist in Figma
    for (key, entry) in &mut map.components {
        // Check if component still exists in file.components
        if !file.components.contains_key(key) {
            if entry.status == ComponentStatus::Implemented {
                entry.status = ComponentStatus::NeedsUpdate;
                entry.notes = Some("Component may have been removed from Figma".to_string());
                removed += 1;
            }
        }
    }

    // Update sync time
    map.figma.last_sync = chrono::Utc::now().to_rfc3339();
    map.figma.file_name = file.name;

    let toml_content = toml::to_string_pretty(&map)?;
    fs::write(map_path, &toml_content)?;

    println!("{}", "Updated!".green());
    println!("  Added: {}", added);
    println!("  Potentially removed: {}", removed);

    Ok(())
}

/// Link a component to its code implementation
fn link(component: &str, code_path: &Path, map_path: &Path) -> Result<()> {
    let content = fs::read_to_string(map_path)?;
    let mut map: ComponentMap = toml::from_str(&content)?;

    // Find component by key or name
    let key = map.components.iter()
        .find(|(k, v)| *k == component || v.figma_name == component)
        .map(|(k, _)| k.clone());

    let key = key.ok_or_else(|| {
        anyhow::anyhow!("Component '{}' not found in map. Use 'fgm map coverage' to see available components.", component)
    })?;

    // Verify code path exists
    if !code_path.exists() {
        println!("{}", format!("Warning: {} does not exist yet", code_path.display()).yellow());
    }

    // Update the entry
    if let Some(entry) = map.components.get_mut(&key) {
        entry.code_path = Some(code_path.to_string_lossy().to_string());
        entry.status = ComponentStatus::Implemented;

        println!("{}", "Linked!".green());
        println!("  Component: {}", entry.figma_name);
        println!("  Code: {}", code_path.display());
    }

    let toml_content = toml::to_string_pretty(&map)?;
    fs::write(map_path, &toml_content)?;

    Ok(())
}

/// Extract COMPONENT nodes from document tree
fn extract_components(document: &crate::api::types::Document, components: &mut HashMap<String, ComponentEntry>) {
    fn visit_node(node: &crate::api::types::Node, components: &mut HashMap<String, ComponentEntry>) {
        if node.node_type == "COMPONENT" || node.node_type == "COMPONENT_SET" {
            components.insert(
                node.id.clone(),
                ComponentEntry {
                    node_id: node.id.clone(),
                    figma_name: node.name.clone(),
                    code_path: None,
                    status: ComponentStatus::NotStarted,
                    notes: None,
                },
            );
        }

        if let Some(children) = &node.children {
            for child in children {
                visit_node(child, components);
            }
        }
    }

    if let Some(children) = &document.children {
        for child in children {
            visit_node(child, components);
        }
    }
}
