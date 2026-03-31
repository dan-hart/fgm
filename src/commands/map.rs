use crate::api::{FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::MapCommands;
use crate::output;
use crate::reporting::{write_report, ReportItem, ReportStatus, ReportSummary};
use crate::select;
use anyhow::Result;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub async fn run(command: MapCommands) -> Result<()> {
    match command {
        MapCommands::Init {
            file_key_or_url,
            pick,
            output,
        } => init(&file_key_or_url, pick, &output).await,
        MapCommands::Coverage { map } => coverage(&map),
        MapCommands::Update { map } => update(&map).await,
        MapCommands::Verify {
            map,
            report,
            report_format,
        } => verify(&map, report.as_deref(), report_format).await,
        MapCommands::Link {
            component,
            code_path,
            map,
        } => link(&component, &code_path, &map),
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
async fn init(file_key_or_url: &str, pick: bool, output: &Path) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    let parsed = FigmaUrl::parse(file_key_or_url)?;
    let file_key = &parsed.file_key;

    output::print_status(&"Fetching Figma file...".bold().to_string());

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

    if pick {
        let options = select::component_options(
            components
                .iter()
                .map(|(key, entry)| (key.clone(), entry.figma_name.clone())),
        );
        let picked = select::pick_options(&options, true)?;
        let picked_ids: std::collections::HashSet<_> =
            picked.into_iter().map(|item| item.id).collect();
        components.retain(|key, _| picked_ids.contains(key));
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

    output::print_status("");
    output::print_success(&format!("Created component map: {}", output.display()));
    output::print_status(&format!("  File: {} ({})", file.name, file_key));
    output::print_status(&format!("  Components: {}", map.components.len()));
    output::print_status("");
    output::print_status(&"Next steps:".bold().to_string());
    output::print_status("  1. Review the generated file");
    output::print_status("  2. Link components to code: fgm map link <component> <code-path>");
    output::print_status("  3. Check coverage: fgm map coverage");

    Ok(())
}

/// Show implementation coverage
fn coverage(map_path: &Path) -> Result<()> {
    let content = fs::read_to_string(map_path)?;
    let map: ComponentMap = toml::from_str(&content)?;

    output::print_status(
        &format!("Component Coverage: {}", map.figma.file_name)
            .bold()
            .to_string(),
    );
    output::print_status(&format!("  Last synced: {}", map.figma.last_sync));
    output::print_status("");

    let total = map.components.len();
    let implemented: Vec<_> = map
        .components
        .values()
        .filter(|c| c.status == ComponentStatus::Implemented)
        .collect();
    let in_progress: Vec<_> = map
        .components
        .values()
        .filter(|c| c.status == ComponentStatus::InProgress)
        .collect();
    let needs_update: Vec<_> = map
        .components
        .values()
        .filter(|c| c.status == ComponentStatus::NeedsUpdate)
        .collect();
    let not_started: Vec<_> = map
        .components
        .values()
        .filter(|c| c.status == ComponentStatus::NotStarted)
        .collect();

    // Coverage bar
    let pct = if total > 0 {
        (implemented.len() * 100) / total
    } else {
        0
    };
    let bar_width = 30;
    let filled = (pct * bar_width) / 100;
    let bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);
    output::print_status(&format!("  [{}] {}%", bar.green(), pct));
    output::print_status("");

    // Summary
    output::print_status(&"Status:".bold().to_string());
    output::print_status(&format!(
        "  {} Implemented ({}/{})",
        "✓".green(),
        implemented.len(),
        total
    ));
    output::print_status(&format!(
        "  {} In Progress ({})",
        "→".yellow(),
        in_progress.len()
    ));
    output::print_status(&format!(
        "  {} Needs Update ({})",
        "!".red(),
        needs_update.len()
    ));
    output::print_status(&format!(
        "  {} Not Started ({})",
        "○".dimmed(),
        not_started.len()
    ));

    // List non-implemented components
    if !not_started.is_empty() || !needs_update.is_empty() {
        output::print_status("");
        output::print_status(&"Pending:".bold().to_string());

        for comp in needs_update.iter().chain(not_started.iter()).take(10) {
            let status_icon = match comp.status {
                ComponentStatus::NeedsUpdate => "!".red(),
                _ => "○".dimmed(),
            };
            output::print_status(&format!("  {} {}", status_icon, comp.figma_name));
        }

        let remaining = not_started.len() + needs_update.len();
        if remaining > 10 {
            output::print_status(&format!("  ... and {} more", remaining - 10));
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
        output::print_status("");
        output::print_status(&"⚠ Broken Links:".yellow().bold().to_string());
        for (name, path) in broken_links {
            output::print_status(&format!("  {} → {}", name, path.red()));
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

    output::print_status(&"Updating from Figma...".bold().to_string());

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

    output::print_success("Updated!");
    output::print_status(&format!("  Added: {}", added));
    output::print_status(&format!("  Potentially removed: {}", removed));

    Ok(())
}

async fn verify(
    map_path: &Path,
    report: Option<&Path>,
    report_format: crate::reporting::ReportFormat,
) -> Result<()> {
    let content = fs::read_to_string(map_path)?;
    let map: ComponentMap = toml::from_str(&content)?;

    let token = get_token()?;
    let client = FigmaClient::new(token)?;
    let file = client.get_file(&map.figma.file_key).await?;

    let mut current_components = HashMap::new();
    extract_components(&file.document, &mut current_components);
    for (key, comp) in &file.components {
        current_components
            .entry(key.clone())
            .or_insert(ComponentEntry {
                node_id: key.clone(),
                figma_name: comp.name.clone(),
                code_path: None,
                status: ComponentStatus::NotStarted,
                notes: None,
            });
    }

    let mut items = Vec::new();

    for (key, entry) in &map.components {
        if entry.code_path.is_none() {
            items.push(ReportItem::warn(
                entry.figma_name.clone(),
                "No linked code path".to_string(),
            ));
        }
        if let Some(path) = &entry.code_path {
            if !Path::new(path).exists() {
                items.push(ReportItem::fail(
                    entry.figma_name.clone(),
                    format!("Broken code path {}", path),
                ));
            }
        }
        if !current_components.contains_key(key) {
            items.push(ReportItem::fail(
                entry.figma_name.clone(),
                "Component no longer exists in Figma".to_string(),
            ));
        }
    }

    for (key, entry) in &current_components {
        if !map.components.contains_key(key) {
            items.push(ReportItem::warn(
                entry.figma_name.clone(),
                "Component exists in Figma but is missing from the map".to_string(),
            ));
        }
    }

    let mut code_path_counts: HashMap<&str, usize> = HashMap::new();
    for entry in map.components.values() {
        if let Some(path) = entry.code_path.as_deref() {
            *code_path_counts.entry(path).or_insert(0) += 1;
        }
    }
    for (path, count) in code_path_counts {
        if count > 1 {
            items.push(ReportItem::warn(
                path.to_string(),
                format!("Shared by {} mapped components", count),
            ));
        }
    }

    if items.is_empty() {
        items.push(ReportItem::ok(
            map.figma.file_name.clone(),
            "Map verification passed cleanly".to_string(),
        ));
    }

    let summary = ReportSummary {
        title: format!("fgm map verify {}", map.figma.file_name),
        items,
    };

    output::print_status(&summary.title.bold().to_string());
    for item in &summary.items {
        let marker = match item.status {
            ReportStatus::Ok => "ok".green(),
            ReportStatus::Warn => "warn".yellow(),
            ReportStatus::Fail => "fail".red(),
        };
        output::print_status(&format!("  {:<6} {}: {}", marker, item.name, item.message));
    }

    if let Some(report_path) = report {
        write_report(report_path, report_format, &summary)?;
        output::print_status(&format!("  Report: {}", report_path.display()));
    }

    if summary.exit_code() != 0 {
        anyhow::bail!("Map verification found issues");
    }

    Ok(())
}

/// Link a component to its code implementation
fn link(component: &str, code_path: &Path, map_path: &Path) -> Result<()> {
    let content = fs::read_to_string(map_path)?;
    let mut map: ComponentMap = toml::from_str(&content)?;

    // Find component by key or name
    let key = map
        .components
        .iter()
        .find(|(k, v)| *k == component || v.figma_name == component)
        .map(|(k, _)| k.clone());

    let key = key.ok_or_else(|| {
        anyhow::anyhow!(
            "Component '{}' not found in map. Use 'fgm map coverage' to see available components.",
            component
        )
    })?;

    // Verify code path exists
    if !code_path.exists() {
        output::print_warning(&format!("{} does not exist yet", code_path.display()));
    }

    // Update the entry
    if let Some(entry) = map.components.get_mut(&key) {
        entry.code_path = Some(code_path.to_string_lossy().to_string());
        entry.status = ComponentStatus::Implemented;

        output::print_success("Linked!");
        output::print_status(&format!("  Component: {}", entry.figma_name));
        output::print_status(&format!("  Code: {}", code_path.display()));
    }

    let toml_content = toml::to_string_pretty(&map)?;
    fs::write(map_path, &toml_content)?;

    Ok(())
}

/// Extract COMPONENT nodes from document tree
fn extract_components(
    document: &crate::api::types::Document,
    components: &mut HashMap<String, ComponentEntry>,
) {
    fn visit_node(
        node: &crate::api::types::Node,
        components: &mut HashMap<String, ComponentEntry>,
    ) {
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
