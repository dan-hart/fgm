use crate::api::{FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::{ExportCommands, Platform};
use crate::config::Config;
use crate::output;
use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::Path;

pub async fn run(command: ExportCommands) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;
    let config = Config::load().unwrap_or_default();

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
            let default_format = crate::cli::ExportFormat::from_config(
                &config.export.default_format,
            )
            .unwrap_or(crate::cli::ExportFormat::Png);
            let format = format.unwrap_or(default_format);
            let scale = scale.unwrap_or(config.export.default_scale);
            if !(1.0..=4.0).contains(&scale) {
                anyhow::bail!("Scale must be between 1 and 4");
            }
            let output = output.unwrap_or_else(|| {
                config
                    .export
                    .output_dir
                    .clone()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| ".".into())
            });

            if let Some(platform) = platform {
                export_platform(
                    &client,
                    &parsed.file_key,
                    &node_ids,
                    all_frames,
                    &output,
                    name.as_deref(),
                    platform,
                )
                .await
            } else {
                export_file(
                    &client,
                    &parsed.file_key,
                    &node_ids,
                    all_frames,
                    &format.to_string(),
                    scale,
                    &output,
                    name.as_deref(),
                )
                .await
            }
        }
        ExportCommands::Batch { manifest } => batch_export(&client, &manifest, &config).await,
    }
}

async fn export_file(
    client: &FigmaClient,
    file_key: &str,
    node_ids: &[String],
    all_frames: bool,
    format: &str,
    scale: f32,
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
        anyhow::bail!(
            "No nodes specified. Use --node, --all-frames, or a URL with ?node-id="
        );
    } else {
        node_ids.to_vec()
    };

    if ids_to_export.is_empty() {
        anyhow::bail!("No frames found to export");
    }

    output::print_status(
        &format!(
            "Exporting {} node(s) as {} at {}x...",
            ids_to_export.len(),
            format,
            scale
        )
        .bold()
        .to_string(),
    );

    // Get export URLs (batch in chunks to avoid API limits)
    // The client handles rate limiting automatically with retries and backoff
    const BATCH_SIZE: usize = 20;
    let mut all_images: std::collections::HashMap<String, Option<String>> = std::collections::HashMap::new();
    let chunks: Vec<_> = ids_to_export.chunks(BATCH_SIZE).collect();
    let total_chunks = chunks.len();

    for (i, chunk) in chunks.into_iter().enumerate() {
        let chunk_vec: Vec<String> = chunk.to_vec();
        let images = client.export_images(file_key, &chunk_vec, format, scale).await?;

        if let Some(err) = &images.err {
            output::print_warning(&format!("API Error: {}", err));
            if images.status == Some(400) || images.status == Some(404) {
                output::print_warning("Some node IDs may be invalid or inaccessible");
            }
            continue;
        }

        all_images.extend(images.images);

        // Small delay between batches for courtesy (rate limiting is handled by client)
        if i < total_chunks - 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }
    }

    if all_images.is_empty() {
        anyhow::bail!("No images were exported");
    }

    // Download each image
    let pb = if output::is_quiet() || output::format() == crate::output::OutputFormat::Json {
        ProgressBar::hidden()
    } else {
        let bar = ProgressBar::new(all_images.len() as u64);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")?
                .progress_chars("#>-"),
        );
        bar
    };

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
    output::print_success(&format!("Exported to {}", output.display()));
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

async fn batch_export(client: &FigmaClient, manifest_path: &Path, config: &Config) -> Result<()> {
    let content = fs::read_to_string(manifest_path)?;
    let manifest: BatchManifest = toml::from_str(&content)?;

    output::print_status(
        &format!("Batch exporting {} items...", manifest.exports.len())
            .bold()
            .to_string(),
    );

    for export in manifest.exports {
        // Support URLs in manifest files too
        let parsed = FigmaUrl::parse(&export.file)?;
        let node_id = export.node.or(parsed.node_id).unwrap_or_default();
        let ids = if node_id.is_empty() { vec![] } else { vec![node_id] };

        let output = std::path::PathBuf::from(&export.output.unwrap_or_else(|| {
            config
                .export
                .output_dir
                .clone()
                .unwrap_or_else(|| ".".to_string())
        }));
        let format = export
            .format
            .clone()
            .unwrap_or_else(|| config.export.default_format.clone());
        let scale = export.scale.unwrap_or(config.export.default_scale);
        if !(1.0..=4.0).contains(&scale) {
            anyhow::bail!("Scale must be between 1 and 4");
        }
        export_file(
            client,
            &parsed.file_key,
            &ids,
            false,
            &format,
            scale,
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
        anyhow::bail!(
            "No nodes specified. Use --node, --all-frames, or a URL with ?node-id="
        );
    } else {
        node_ids.to_vec()
    };

    if ids_to_export.is_empty() {
        anyhow::bail!("No frames found to export");
    }

    // Define scale factors for each platform
    let scales: Vec<(f32, &str)> = match platform {
        Platform::Ios => vec![(1.0, ""), (2.0, "@2x"), (3.0, "@3x")],
        Platform::Android => vec![(1.0, "mdpi"), (1.5, "hdpi"), (2.0, "xhdpi"), (3.0, "xxhdpi"), (4.0, "xxxhdpi")],
        Platform::Web => vec![(1.0, ""), (2.0, "@2x")],
    };

    let platform_name = match platform {
        Platform::Ios => "iOS",
        Platform::Android => "Android",
        Platform::Web => "Web",
    };

    output::print_status(
        &format!(
            "Exporting {} node(s) for {} ({} sizes)...",
            ids_to_export.len(),
            platform_name,
            scales.len()
        )
        .bold()
        .to_string(),
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
        output::print_status(&format!(
            "  Exporting at {}x ({})...",
            scale,
            if suffix.is_empty() { "1x" } else { suffix }
        ));

                        let images =
                            client.export_images(file_key, &ids_to_export, "png", *scale).await?;

        if let Some(err) = &images.err {
            output::print_warning(&format!("API Error: {}", err));
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

    output::print_success(&format!("Exported to {}", output.display()));

    // Print platform-specific guidance
    match platform {
        Platform::Ios => {
            output::print_status("");
            output::print_status(&"iOS Usage:".bold().to_string());
            output::print_status("  Add images to Assets.xcassets in Xcode");
            output::print_status("  Use: Image(\"name\") or UIImage(named: \"name\")");
        }
        Platform::Android => {
            output::print_status("");
            output::print_status(&"Android Usage:".bold().to_string());
            output::print_status("  Copy drawable-* folders to app/src/main/res/");
            output::print_status("  Use: @drawable/name or R.drawable.name");
        }
        Platform::Web => {
            output::print_status("");
            output::print_status(&"Web Usage:".bold().to_string());
            output::print_status("  Use srcset for responsive images:");
            output::print_status("  <img src=\"icon.png\" srcset=\"icon@2x.png 2x\">");
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
    scale: Option<f32>,
    #[serde(default)]
    output: Option<String>,
}
