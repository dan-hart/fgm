//! Cache management commands
//!
//! Provides commands to warm up, inspect, and clear the Figma API cache.

use crate::api::{create_shared_cache, FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::CacheCommands;
use crate::config::Config;
use crate::output;
use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

pub async fn run(command: CacheCommands) -> Result<()> {
    match command {
        CacheCommands::Warmup {
            file_key_or_url,
            include_images,
        } => warmup(&file_key_or_url, include_images).await,
        CacheCommands::Status => status().await,
        CacheCommands::Clear { all, file } => clear(all, file.as_deref()).await,
    }
}

/// Prefetch all data for a Figma file
///
/// This is the "fetch EVERYTHING from Figma on the first call" feature.
/// Warms the cache with file metadata, versions, and optionally image URLs.
async fn warmup(file_key_or_url: &str, include_images: bool) -> Result<()> {
    let token = get_token()?;
    let cache = create_shared_cache();
    let client = FigmaClient::with_cache(token, cache.clone())?;
    let config = Config::load().unwrap_or_default();
    if !(1.0..=4.0).contains(&config.export.default_scale) {
        anyhow::bail!("Scale must be between 1 and 4");
    }

    let parsed = FigmaUrl::parse(file_key_or_url)?;
    let file_key = &parsed.file_key;

    output::print_status(&format!("Warming cache for file: {}", file_key).bold().to_string());

    let pb = if output::is_quiet() || output::format() == crate::output::OutputFormat::Json {
        ProgressBar::hidden()
    } else {
        let bar = ProgressBar::new(4);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")?
                .progress_chars("#>-"),
        );
        bar
    };

    // 1. Fetch file metadata (this caches the full document tree)
    pb.set_message("Fetching file metadata...");
    let file = client.get_file(file_key).await?;
    pb.inc(1);

    // 2. Fetch version history
    pb.set_message("Fetching versions...");
    let _versions = client.get_versions(file_key).await?;
    pb.inc(1);

    // 3. Extract all node IDs from the document
    pb.set_message("Extracting nodes...");
    let node_ids = extract_all_node_ids(&file.document);
    let frame_ids = extract_frame_ids(&file.document);
    output::print_status(&format!(
        "  Found {} total nodes, {} top-level frames",
        node_ids.len(),
        frame_ids.len()
    ));
    pb.inc(1);

    // 4. Optionally prefetch image URLs for all frames
    if include_images && !frame_ids.is_empty() {
        pb.set_message("Prefetching image URLs...");
        // Batch in chunks of 50 to avoid rate limits
        for chunk in frame_ids.chunks(50) {
            let chunk_vec: Vec<String> = chunk.to_vec();
            let _images = client
                .export_images(file_key, &chunk_vec, "png", config.export.default_scale)
                .await;
            // Small delay between batches
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        }
    }
    pb.inc(1);

    pb.finish_with_message("Complete!");

    // Show cache stats
    let stats = cache.stats();
    output::print_status("");
    output::print_status(&"Cache warmed successfully:".green().to_string());
    output::print_status(&format!("  File: {}", file.name));
    output::print_status(&format!("  Memory entries: {}", stats.memory_entries));
    output::print_status(&format!("  Disk entries: {}", stats.disk_entries));
    if let Some(path) = stats.disk_path {
        output::print_status(&format!("  Cache location: {}", path.display()));
    }

    Ok(())
}

/// Show cache statistics
async fn status() -> Result<()> {
    let cache = create_shared_cache();
    let stats = cache.stats();

    output::print_status(&"Cache Status:".bold().to_string());
    output::print_status(&format!("  Memory entries: {}", stats.memory_entries));
    output::print_status(&format!(
        "  Memory size: {} bytes",
        stats.memory_weighted_size
    ));
    output::print_status(&format!(
        "  Disk caching: {}",
        if stats.disk_enabled {
            "enabled".green()
        } else {
            "disabled".yellow()
        }
    ));
    output::print_status(&format!("  Disk entries: {}", stats.disk_entries));

    if let Some(path) = stats.disk_path {
        output::print_status(&format!("  Cache location: {}", path.display()));

        // Check if path exists and show disk usage
        if path.exists() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                let total_size: u64 = entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum();
                output::print_status(&format!("  Disk usage: {} KB", total_size / 1024));
            }
        }
    }

    Ok(())
}

/// Clear cached data
async fn clear(all: bool, file_key: Option<&str>) -> Result<()> {
    let cache = create_shared_cache();

    if all {
        cache.clear();
        output::print_success("All caches cleared");
    } else if let Some(key) = file_key {
        cache.invalidate_file(key);
        output::print_success(&format!("Cache cleared for file: {}", key));
    } else {
        output::print_warning(
            "Specify --all to clear entire cache or --file <key> to clear specific file",
        );
        output::print_status("");
        output::print_status("Examples:");
        output::print_status("  fgm cache clear --all");
        output::print_status("  fgm cache clear --file abc123");
    }

    Ok(())
}

/// Extract all node IDs from a document
fn extract_all_node_ids(document: &crate::api::types::Document) -> Vec<String> {
    let mut ids = Vec::new();
    collect_node_ids(&document.children, &mut ids);
    ids
}

/// Recursively collect node IDs
fn collect_node_ids(children: &Option<Vec<crate::api::types::Node>>, ids: &mut Vec<String>) {
    if let Some(nodes) = children {
        for node in nodes {
            ids.push(node.id.clone());
            collect_node_ids(&node.children, ids);
        }
    }
}

/// Extract top-level frame IDs (for image export)
fn extract_frame_ids(document: &crate::api::types::Document) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(children) = &document.children {
        for child in children {
            // Pages
            if child.node_type == "CANVAS" {
                if let Some(frames) = &child.children {
                    for frame in frames {
                        if frame.node_type == "FRAME" || frame.node_type == "COMPONENT" {
                            ids.push(frame.id.clone());
                        }
                    }
                }
            }
        }
    }
    ids
}
