use crate::api::{FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::{ExportCommands, Platform};
use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::Path;

pub async fn run(command: ExportCommands) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    match command {
        ExportCommands::File {
            file_key_or_url,
            node,
            all_frames,
            format,
            scale,
            output,
            name,
            platform,
        } => {
            // Parse URL or file key
            let parsed = FigmaUrl::parse(&file_key_or_url)?;

            // Merge node IDs from URL and command line
            let mut node_ids = node;
            if let Some(url_node_id) = parsed.node_id {
                if !node_ids.contains(&url_node_id) {
                    node_ids.push(url_node_id);
                }
            }

            // Handle platform-specific export
            if let Some(platform) = platform {
                export_platform(&client, &parsed.file_key, &node_ids, all_frames, &output, name.as_deref(), platform).await
            } else {
                export_file(&client, &parsed.file_key, &node_ids, all_frames, &format.to_string(), scale, &output, name.as_deref()).await
            }
        }
        ExportCommands::Batch { manifest } => batch_export(&client, &manifest).await,
    }
}

async fn export_file(
    client: &FigmaClient,
    file_key: &str,
    node_ids: &[String],
    all_frames: bool,
    format: &str,
    scale: u8,
    output: &Path,
    custom_name: Option<&str>,
) -> Result<()> {
    // Ensure output directory exists
    fs::create_dir_all(output)?;

    let ids_to_export: Vec<String> = if all_frames {
        // Get all top-level frames
        let file = client.get_file(file_key).await?;
        extract_frame_ids(&file.document)
    } else if node_ids.is_empty() {
        println!("{}", "No nodes specified. Use --node, --all-frames, or a URL with ?node-id=".yellow());
        return Ok(());
    } else {
        node_ids.to_vec()
    };

    if ids_to_export.is_empty() {
        println!("{}", "No frames found to export".yellow());
        return Ok(());
    }

    println!(
        "{}",
        format!("Exporting {} node(s) as {} at {}x...", ids_to_export.len(), format, scale).bold()
    );

    // Get export URLs (batch in chunks to avoid API limits)
    // Figma API has strict rate limits, so we batch and add delays
    const BATCH_SIZE: usize = 20;
    let mut all_images: std::collections::HashMap<String, Option<String>> = std::collections::HashMap::new();
    let chunks: Vec<_> = ids_to_export.chunks(BATCH_SIZE).collect();
    let total_chunks = chunks.len();

    for (i, chunk) in chunks.into_iter().enumerate() {
        let chunk_vec: Vec<String> = chunk.to_vec();
        let images = client.export_images(file_key, &chunk_vec, format, scale).await?;

        if let Some(err) = &images.err {
            if err.contains("Rate limit") {
                println!("{}: {} - waiting 30s...", "Rate limited".yellow(), err);
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                // Retry this chunk
                let retry = client.export_images(file_key, &chunk_vec, format, scale).await?;
                if retry.err.is_none() {
                    all_images.extend(retry.images);
                }
            } else {
                println!("{}: {}", "API Error".red(), err);
                if images.status == Some(400) || images.status == Some(404) {
                    println!("{}", "  Some node IDs may be invalid or inaccessible".yellow());
                }
            }
            continue;
        }

        all_images.extend(images.images);

        // Add delay between batches to avoid rate limiting (except for last batch)
        if i < total_chunks - 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    if all_images.is_empty() {
        println!("{}", "No images were exported".yellow());
        return Ok(());
    }

    // Download each image
    let pb = ProgressBar::new(all_images.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")?
            .progress_chars("#>-"),
    );

    // Use custom name only if there's a single image
    let use_custom_name = custom_name.is_some() && all_images.len() == 1;

    for (i, (node_id, url)) in all_images.into_iter().enumerate() {
        if let Some(url) = url {
            let bytes = client.download_image(&url).await?;
            let filename = if use_custom_name {
                format!("{}.{}", custom_name.unwrap(), format)
            } else if let Some(name) = custom_name {
                // Multiple images with custom name - append index
                format!("{}-{}.{}", name, i + 1, format)
            } else {
                format!("{}.{}", node_id.replace(":", "-"), format)
            };
            let filepath = output.join(&filename);
            fs::write(&filepath, bytes)?;
            pb.set_message(filename);
        }
        pb.inc(1);
    }

    pb.finish_with_message("done");
    println!("{}", format!("Exported to {}", output.display()).green());
    Ok(())
}

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

async fn batch_export(client: &FigmaClient, manifest_path: &Path) -> Result<()> {
    let content = fs::read_to_string(manifest_path)?;
    let manifest: BatchManifest = toml::from_str(&content)?;

    println!(
        "{}",
        format!("Batch exporting {} items...", manifest.exports.len()).bold()
    );

    for export in manifest.exports {
        // Support URLs in manifest files too
        let parsed = FigmaUrl::parse(&export.file)?;
        let node_id = export.node.or(parsed.node_id).unwrap_or_default();
        let ids = if node_id.is_empty() { vec![] } else { vec![node_id] };

        let output = std::path::PathBuf::from(&export.output.unwrap_or_else(|| ".".to_string()));
        export_file(
            client,
            &parsed.file_key,
            &ids,
            false,
            &export.format.unwrap_or_else(|| "png".to_string()),
            export.scale.unwrap_or(2),
            &output,
            export.name.as_deref(),
        )
        .await?;
    }

    Ok(())
}

/// Export for specific platform with all required sizes
async fn export_platform(
    client: &FigmaClient,
    file_key: &str,
    node_ids: &[String],
    all_frames: bool,
    output: &Path,
    custom_name: Option<&str>,
    platform: Platform,
) -> Result<()> {
    let ids_to_export: Vec<String> = if all_frames {
        let file = client.get_file(file_key).await?;
        extract_frame_ids(&file.document)
    } else if node_ids.is_empty() {
        println!("{}", "No nodes specified. Use --node, --all-frames, or a URL with ?node-id=".yellow());
        return Ok(());
    } else {
        node_ids.to_vec()
    };

    if ids_to_export.is_empty() {
        println!("{}", "No frames found to export".yellow());
        return Ok(());
    }

    // Define scale factors for each platform
    let scales: Vec<(u8, &str)> = match platform {
        Platform::Ios => vec![(1, ""), (2, "@2x"), (3, "@3x")],
        Platform::Android => vec![
            (1, "mdpi"),
            (2, "hdpi"),    // 1.5x rounded to 2
            (2, "xhdpi"),   // 2x
            (3, "xxhdpi"),  // 3x
            (4, "xxxhdpi"), // 4x
        ],
        Platform::Web => vec![(1, ""), (2, "@2x")],
    };

    let platform_name = match platform {
        Platform::Ios => "iOS",
        Platform::Android => "Android",
        Platform::Web => "Web",
    };

    println!(
        "{}",
        format!("Exporting {} node(s) for {} ({} sizes)...",
            ids_to_export.len(), platform_name, scales.len()).bold()
    );

    // Create platform directory structure
    match platform {
        Platform::Android => {
            // Android needs drawable-* directories
            for (_, suffix) in &scales {
                fs::create_dir_all(output.join(format!("drawable-{}", suffix)))?;
            }
        }
        _ => {
            fs::create_dir_all(output)?;
        }
    }

    // Export at each scale
    for (scale, suffix) in &scales {
        println!("  Exporting at {}x ({})...", scale, if suffix.is_empty() { "1x" } else { suffix });

        let images = client.export_images(file_key, &ids_to_export, "png", *scale).await?;

        if let Some(err) = &images.err {
            println!("{}: {}", "API Error".red(), err);
            continue;
        }

        for (node_id, url) in images.images {
            if let Some(url) = url {
                let bytes = client.download_image(&url).await?;
                let default_name = node_id.replace(':', "-");
                let base_name = custom_name.unwrap_or(&default_name);

                let filepath = match platform {
                    Platform::Ios => {
                        // iOS: icon@2x.png, icon@3x.png
                        let filename = if suffix.is_empty() {
                            format!("{}.png", base_name)
                        } else {
                            format!("{}{}.png", base_name, suffix)
                        };
                        output.join(filename)
                    }
                    Platform::Android => {
                        // Android: drawable-hdpi/icon.png, etc.
                        let dir = output.join(format!("drawable-{}", suffix));
                        dir.join(format!("{}.png", base_name))
                    }
                    Platform::Web => {
                        // Web: icon.png, icon@2x.png
                        let filename = if suffix.is_empty() {
                            format!("{}.png", base_name)
                        } else {
                            format!("{}{}.png", base_name, suffix)
                        };
                        output.join(filename)
                    }
                };

                fs::write(&filepath, bytes)?;
            }
        }

        // Rate limit protection
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    println!("{}", format!("Exported to {}", output.display()).green());

    // Print platform-specific guidance
    match platform {
        Platform::Ios => {
            println!();
            println!("{}", "iOS Usage:".bold());
            println!("  Add images to Assets.xcassets in Xcode");
            println!("  Use: Image(\"name\") or UIImage(named: \"name\")");
        }
        Platform::Android => {
            println!();
            println!("{}", "Android Usage:".bold());
            println!("  Copy drawable-* folders to app/src/main/res/");
            println!("  Use: @drawable/name or R.drawable.name");
        }
        Platform::Web => {
            println!();
            println!("{}", "Web Usage:".bold());
            println!("  Use srcset for responsive images:");
            println!("  <img src=\"icon.png\" srcset=\"icon@2x.png 2x\">");
        }
    }

    Ok(())
}

#[derive(serde::Deserialize)]
struct BatchManifest {
    exports: Vec<ExportItem>,
}

#[derive(serde::Deserialize)]
struct ExportItem {
    file: String,
    #[serde(default)]
    node: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    scale: Option<u8>,
    #[serde(default)]
    output: Option<String>,
}
