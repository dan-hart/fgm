use crate::api::FigmaClient;
use crate::auth::get_token;
use crate::cli::ExportCommands;
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
            file_key,
            node,
            all_frames,
            format,
            scale,
            output,
        } => {
            export_file(&client, &file_key, &node, all_frames, &format.to_string(), scale, &output).await
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
) -> Result<()> {
    // Ensure output directory exists
    fs::create_dir_all(output)?;

    let ids_to_export: Vec<String> = if all_frames {
        // Get all top-level frames
        let file = client.get_file(file_key).await?;
        extract_frame_ids(&file.document)
    } else if node_ids.is_empty() {
        println!("{}", "No nodes specified. Use --node or --all-frames".yellow());
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

    // Get export URLs
    let images = client.export_images(file_key, &ids_to_export, format, scale).await?;

    if let Some(err) = images.err {
        println!("{}: {}", "API Error".red(), err);
        return Ok(());
    }

    // Download each image
    let pb = ProgressBar::new(ids_to_export.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")?
            .progress_chars("#>-"),
    );

    for (node_id, url) in images.images {
        if let Some(url) = url {
            let bytes = client.download_image(&url).await?;
            let filename = format!("{}.{}", node_id.replace(":", "-"), format);
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
        let ids = vec![export.node.clone()];
        let output = std::path::PathBuf::from(&export.output.unwrap_or_else(|| ".".to_string()));
        export_file(
            client,
            &export.file,
            &ids,
            false,
            &export.format.unwrap_or_else(|| "png".to_string()),
            export.scale.unwrap_or(2),
            &output,
        )
        .await?;
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
    node: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    scale: Option<u8>,
    #[serde(default)]
    output: Option<String>,
}
