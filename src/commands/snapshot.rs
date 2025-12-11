use crate::api::{FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::SnapshotCommands;
use anyhow::Result;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub async fn run(command: SnapshotCommands) -> Result<()> {
    match command {
        SnapshotCommands::Create {
            file_key_or_url,
            name,
            node,
            output,
        } => create(&file_key_or_url, &name, &node, &output).await,
        SnapshotCommands::List { dir } => list(&dir),
        SnapshotCommands::Diff { from, to, dir, output } => diff(&from, &to, &dir, output.as_deref()).await,
    }
}

/// Snapshot metadata stored alongside images
#[derive(Serialize, Deserialize)]
struct SnapshotMeta {
    name: String,
    file_key: String,
    created_at: String,
    nodes: Vec<NodeSnapshot>,
}

#[derive(Serialize, Deserialize)]
struct NodeSnapshot {
    id: String,
    name: String,
    filename: String,
}

async fn create(file_key_or_url: &str, name: &str, nodes: &[String], output: &Path) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    let parsed = FigmaUrl::parse(file_key_or_url)?;
    let file_key = &parsed.file_key;

    // Merge nodes from URL and command line
    let mut node_ids: Vec<String> = nodes.to_vec();
    if let Some(url_node_id) = parsed.node_id {
        if !node_ids.contains(&url_node_id) {
            node_ids.push(url_node_id);
        }
    }

    // Create snapshot directory
    let snapshot_dir = output.join(name);
    fs::create_dir_all(&snapshot_dir)?;

    println!("{}", format!("Creating snapshot '{}'...", name).bold());

    // Get file info to find frames
    let file = client.get_file(file_key).await?;

    // Determine which nodes to snapshot
    let ids_to_export: Vec<String> = if node_ids.is_empty() {
        // Default to all top-level frames
        extract_frame_info(&file.document)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    } else {
        node_ids
    };

    if ids_to_export.is_empty() {
        println!("{}", "No frames found to snapshot".yellow());
        return Ok(());
    }

    println!("  Exporting {} nodes...", ids_to_export.len());

    // Export images at 2x for comparison
    let images = client.export_images(file_key, &ids_to_export, "png", 2).await?;

    if let Some(err) = &images.err {
        println!("{}: {}", "API Error".red(), err);
        return Ok(());
    }

    // Build node name lookup
    let frame_info = extract_frame_info(&file.document);
    let name_lookup: std::collections::HashMap<String, String> = frame_info.into_iter().collect();

    // Download and save images
    let mut snapshots = Vec::new();
    for (node_id, url) in images.images {
        if let Some(url) = url {
            let bytes = client.download_image(&url).await?;
            let safe_id = node_id.replace(':', "-");
            let filename = format!("{}.png", safe_id);
            let filepath = snapshot_dir.join(&filename);
            fs::write(&filepath, bytes)?;

            let node_name = name_lookup.get(&node_id).cloned().unwrap_or_else(|| node_id.clone());
            println!("  {} {}", "✓".green(), node_name);

            snapshots.push(NodeSnapshot {
                id: node_id,
                name: node_name,
                filename,
            });
        }
    }

    // Save metadata
    let meta = SnapshotMeta {
        name: name.to_string(),
        file_key: file_key.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        nodes: snapshots,
    };

    let meta_path = snapshot_dir.join("snapshot.json");
    let meta_json = serde_json::to_string_pretty(&meta)?;
    fs::write(&meta_path, meta_json)?;

    println!();
    println!("{}", format!("Snapshot '{}' created at {}", name, snapshot_dir.display()).green());
    Ok(())
}

fn list(dir: &Path) -> Result<()> {
    if !dir.exists() {
        println!("{}", format!("No snapshots directory at {}", dir.display()).yellow());
        return Ok(());
    }

    println!("{}", "Snapshots:".bold());

    let mut found = false;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let meta_path = path.join("snapshot.json");
            if meta_path.exists() {
                let content = fs::read_to_string(&meta_path)?;
                let meta: SnapshotMeta = serde_json::from_str(&content)?;

                println!();
                println!("  {} ({})", meta.name.cyan(), meta.created_at.dimmed());
                println!("    File: {}", meta.file_key);
                println!("    Nodes: {}", meta.nodes.len());
                found = true;
            }
        }
    }

    if !found {
        println!("{}", "  No snapshots found".yellow());
    }

    Ok(())
}

async fn diff(from: &str, to: &str, dir: &Path, output: Option<&Path>) -> Result<()> {
    let from_dir = dir.join(from);
    let to_dir = dir.join(to);

    if !from_dir.exists() {
        anyhow::bail!("Snapshot '{}' not found at {}", from, from_dir.display());
    }
    if !to_dir.exists() {
        anyhow::bail!("Snapshot '{}' not found at {}", to, to_dir.display());
    }

    // Load metadata
    let from_meta: SnapshotMeta = serde_json::from_str(&fs::read_to_string(from_dir.join("snapshot.json"))?)?;
    let to_meta: SnapshotMeta = serde_json::from_str(&fs::read_to_string(to_dir.join("snapshot.json"))?)?;

    println!("{}", format!("Comparing '{}' → '{}'", from, to).bold());
    println!("  From: {} ({})", from_meta.name, from_meta.created_at);
    println!("  To:   {} ({})", to_meta.name, to_meta.created_at);
    println!();

    // Create output directory if specified
    let diff_output = if let Some(out) = output {
        fs::create_dir_all(out)?;
        Some(out)
    } else {
        None
    };

    // Build lookup of to-snapshot nodes
    let to_nodes: std::collections::HashMap<String, &NodeSnapshot> =
        to_meta.nodes.iter().map(|n| (n.id.clone(), n)).collect();

    let mut total = 0;
    let mut changed = 0;
    let mut added = 0;
    let mut removed = 0;

    // Compare each node from the "from" snapshot
    for from_node in &from_meta.nodes {
        total += 1;

        if let Some(to_node) = to_nodes.get(&from_node.id) {
            // Node exists in both - compare images
            let from_path = from_dir.join(&from_node.filename);
            let to_path = to_dir.join(&to_node.filename);

            let from_img = image::open(&from_path)?;
            let to_img = image::open(&to_path)?;

            let diff_percent = crate::commands::compare::calculate_diff(&from_img, &to_img);

            if diff_percent > 0.1 {
                changed += 1;
                println!("  {} {} ({:.1}% different)", "~".yellow(), from_node.name, diff_percent);

                // Generate diff image if output specified
                if let Some(out_dir) = diff_output {
                    let diff_img = crate::commands::compare::generate_diff_image(&from_img, &to_img);
                    let diff_path = out_dir.join(format!("{}-diff.png", from_node.id.replace(':', "-")));
                    diff_img.save(&diff_path)?;
                }
            } else {
                println!("  {} {} (unchanged)", "=".dimmed(), from_node.name.dimmed());
            }
        } else {
            // Node was removed
            removed += 1;
            println!("  {} {} (removed)", "-".red(), from_node.name);
        }
    }

    // Find added nodes
    for to_node in &to_meta.nodes {
        if !from_meta.nodes.iter().any(|n| n.id == to_node.id) {
            added += 1;
            total += 1;
            println!("  {} {} (added)", "+".green(), to_node.name);
        }
    }

    println!();
    println!("{}", "Summary:".bold());
    println!("  Total: {} | Changed: {} | Added: {} | Removed: {}",
             total, changed, added, removed);

    if let Some(out_dir) = diff_output {
        println!("  Diff images saved to: {}", out_dir.display());
    }

    Ok(())
}

/// Extract frame IDs and names from document
fn extract_frame_info(document: &crate::api::types::Document) -> Vec<(String, String)> {
    let mut frames = Vec::new();
    if let Some(children) = &document.children {
        for child in children {
            if child.node_type == "CANVAS" {
                if let Some(page_frames) = &child.children {
                    for frame in page_frames {
                        if frame.node_type == "FRAME" || frame.node_type == "COMPONENT" {
                            frames.push((frame.id.clone(), frame.name.clone()));
                        }
                    }
                }
            }
        }
    }
    frames
}
