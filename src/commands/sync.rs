use crate::api::{FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::SyncArgs;
use anyhow::Result;
use colored::Colorize;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub async fn run(args: SyncArgs) -> Result<()> {
    let content = fs::read_to_string(&args.manifest)?;
    let manifest: SyncManifest = toml::from_str(&content)?;

    println!("{}", format!("Asset Sync: {}", manifest.project.name).bold());

    if args.dry_run {
        println!("{}", "(Dry run - no files will be modified)".yellow());
    }

    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    // Process each asset definition
    let mut synced = 0;
    let mut skipped = 0;
    let mut errors = 0;

    for (name, asset) in &manifest.assets {
        println!();
        println!("  {} {}", "→".cyan(), name.bold());

        // Parse Figma URL/key
        let parsed = FigmaUrl::parse(&asset.figma)?;
        let node_id = asset.node.clone().or(parsed.node_id);

        let node_id = match node_id {
            Some(id) => id,
            None => {
                println!("    {}: Missing node ID", "skip".yellow());
                skipped += 1;
                continue;
            }
        };

        // Determine output path
        let output_path = resolve_output_path(&manifest.project.output_dir, &asset.output, name);

        // Check if file exists and force flag
        if output_path.exists() && !args.force && !args.dry_run {
            println!("    {}: {} (use --force to overwrite)", "exists".dimmed(), output_path.display());
            skipped += 1;
            continue;
        }

        if args.dry_run {
            println!("    would export {} → {}", node_id, output_path.display());
            synced += 1;
            continue;
        }

        // Export the asset
        let format = asset.format.as_deref().unwrap_or("png");
        let scale = asset.scale.unwrap_or(2);

        match export_asset(&client, &parsed.file_key, &node_id, format, scale, &output_path).await {
            Ok(_) => {
                println!("    {} {}", "✓".green(), output_path.display());
                synced += 1;
            }
            Err(e) => {
                println!("    {}: {}", "error".red(), e);
                errors += 1;
            }
        }

        // Rate limit protection
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    }

    // Summary
    println!();
    println!("{}", "Summary:".bold());
    println!("  Synced: {} | Skipped: {} | Errors: {}", synced, skipped, errors);

    if args.dry_run {
        println!();
        println!("{}", "Run without --dry-run to apply changes".yellow());
    }

    if errors > 0 {
        std::process::exit(1);
    }

    Ok(())
}

async fn export_asset(
    client: &FigmaClient,
    file_key: &str,
    node_id: &str,
    format: &str,
    scale: u8,
    output: &Path,
) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    let images = client.export_images(file_key, &[node_id.to_string()], format, scale).await?;

    if let Some(err) = &images.err {
        anyhow::bail!("Figma API error: {}", err);
    }

    let url = images
        .images
        .get(node_id)
        .and_then(|u| u.as_ref())
        .ok_or_else(|| anyhow::anyhow!("No image URL returned"))?;

    let bytes = client.download_image(url).await?;
    fs::write(output, bytes)?;

    Ok(())
}

fn resolve_output_path(base_dir: &Option<String>, asset_output: &Option<String>, name: &str) -> PathBuf {
    let base = base_dir.as_ref().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));

    if let Some(output) = asset_output {
        if Path::new(output).is_absolute() {
            PathBuf::from(output)
        } else {
            base.join(output)
        }
    } else {
        // Default: name.png in base directory
        base.join(format!("{}.png", name))
    }
}

/// Sync manifest format
#[derive(Deserialize)]
struct SyncManifest {
    project: ProjectConfig,
    #[serde(default)]
    assets: HashMap<String, AssetDefinition>,
}

#[derive(Deserialize)]
struct ProjectConfig {
    name: String,
    #[serde(default)]
    output_dir: Option<String>,
}

#[derive(Deserialize)]
struct AssetDefinition {
    /// Figma file URL or key
    figma: String,
    /// Node ID (can be in URL)
    #[serde(default)]
    node: Option<String>,
    /// Output path relative to project output_dir
    #[serde(default)]
    output: Option<String>,
    /// Export format (png, svg, pdf, jpg)
    #[serde(default)]
    format: Option<String>,
    /// Export scale (1-4)
    #[serde(default)]
    scale: Option<u8>,
}
