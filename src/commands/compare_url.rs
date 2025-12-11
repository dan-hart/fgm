use crate::api::{FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::CompareUrlArgs;
use crate::commands::compare;
use anyhow::Result;
use colored::Colorize;
use std::fs;

/// Compare a Figma design directly against a screenshot
/// Exports the Figma frame and runs pixel comparison in one command
pub async fn run(args: CompareUrlArgs) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;

    // Parse URL to get file key and node ID
    let parsed = FigmaUrl::parse(&args.figma_url)?;
    let node_id = parsed.node_id.ok_or_else(|| {
        anyhow::anyhow!("URL must include a node-id parameter (e.g., ?node-id=1-2)")
    })?;

    println!("{}", "Exporting Figma design...".bold());
    println!("  File: {}", parsed.file_key);
    println!("  Node: {}", node_id);
    println!("  Scale: {}x", args.scale);

    // Export the Figma node
    let images = client
        .export_images(&parsed.file_key, &[node_id.clone()], "png", args.scale)
        .await?;

    if let Some(err) = &images.err {
        anyhow::bail!("Figma API error: {}", err);
    }

    let url = images
        .images
        .get(&node_id)
        .and_then(|u| u.as_ref())
        .ok_or_else(|| anyhow::anyhow!("No image URL returned for node {}", node_id))?;

    // Download the exported image
    println!("  Downloading export...");
    let figma_bytes = client.download_image(url).await?;

    // Create temp file for Figma export
    let temp_dir = std::env::temp_dir();
    let figma_path = temp_dir.join(format!("fgm-compare-{}.png", node_id.replace(':', "-")));
    fs::write(&figma_path, &figma_bytes)?;

    // Load both images
    let figma_img = image::open(&figma_path)?;
    let screenshot_img = image::open(&args.screenshot)?;

    println!();
    println!("{}", "Comparing images...".bold());

    // Get dimensions
    use image::GenericImageView;
    let (fw, fh) = figma_img.dimensions();
    let (sw, sh) = screenshot_img.dimensions();

    println!("  Figma:      {}x{}", fw, fh);
    println!("  Screenshot: {}x{}", sw, sh);

    // Check dimensions
    if fw != sw || fh != sh {
        println!();
        println!("{}", "⚠ Dimension mismatch!".yellow());
        println!("  Consider adjusting --scale or resizing the screenshot");

        // Still calculate diff for reference
        let diff_percent = compare::calculate_diff(&figma_img, &screenshot_img);
        println!("  Pixel diff: {:.2}%", diff_percent);

        // Clean up temp file
        let _ = fs::remove_file(&figma_path);

        return Ok(());
    }

    // Calculate difference
    let diff_percent = compare::calculate_diff(&figma_img, &screenshot_img);

    println!();
    if diff_percent <= args.threshold {
        println!(
            "{} Pixel diff: {:.2}% (threshold: {:.1}%)",
            "✓".green().bold(),
            diff_percent,
            args.threshold
        );
    } else {
        println!(
            "{} Pixel diff: {:.2}% (threshold: {:.1}%)",
            "✗".red().bold(),
            diff_percent,
            args.threshold
        );
    }

    // Generate diff image if output specified
    if let Some(output_path) = &args.output {
        let diff_img = compare::generate_diff_image(&figma_img, &screenshot_img);
        diff_img.save(output_path)?;
        println!("  Diff image saved to: {}", output_path.display());
    }

    // Clean up temp file
    let _ = fs::remove_file(&figma_path);

    // Exit with appropriate code for CI
    if diff_percent > args.threshold {
        std::process::exit(1);
    }

    Ok(())
}
