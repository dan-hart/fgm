use crate::api::{FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::CompareUrlArgs;
use crate::commands::compare;
use crate::config::Config;
use crate::output;
use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use std::fs;

/// Compare a Figma design directly against a screenshot
/// Exports the Figma frame and runs pixel comparison in one command
pub async fn run(args: CompareUrlArgs) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;
    let config = Config::load().unwrap_or_default();

    // Parse URL to get file key and node ID
    let parsed = FigmaUrl::parse(&args.figma_url)?;
    let node_id = parsed.node_id.ok_or_else(|| {
        anyhow::anyhow!("URL must include a node-id parameter (e.g., ?node-id=1-2)")
    })?;

    let scale = args.scale.unwrap_or(config.export.default_scale);
    if !(1.0..=4.0).contains(&scale) {
        anyhow::bail!("Scale must be between 1 and 4");
    }
    if !(0.0..=100.0).contains(&args.threshold) {
        anyhow::bail!("Threshold must be between 0 and 100");
    }
    output::print_status(&"Exporting Figma design...".bold().to_string());
    output::print_status(&format!("  File: {}", parsed.file_key));
    output::print_status(&format!("  Node: {}", node_id));
    output::print_status(&format!("  Scale: {}x", scale));

    // Export the Figma node
    let images = client
        .export_images(&parsed.file_key, &[node_id.clone()], "png", scale)
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
    output::print_status("  Downloading export...");
    let figma_bytes = client.download_image(url).await?;

    // Create temp file for Figma export
    let temp_dir = std::env::temp_dir();
    let figma_path = temp_dir.join(format!("fgm-compare-{}.png", node_id.replace(':', "-")));
    fs::write(&figma_path, &figma_bytes)?;

    // Load both images
    let figma_img = image::open(&figma_path)?;
    let screenshot_img = image::open(&args.screenshot)?;

    output::print_status("");
    output::print_status(&"Comparing images...".bold().to_string());

    // Get dimensions
    use image::GenericImageView;
    let (fw, fh) = figma_img.dimensions();
    let (sw, sh) = screenshot_img.dimensions();

    output::print_status(&format!("  Figma:      {}x{}", fw, fh));
    output::print_status(&format!("  Screenshot: {}x{}", sw, sh));

    // Check dimensions
    if fw != sw || fh != sh {
        output::print_status("");
        output::print_warning("Dimension mismatch!");
        output::print_status("  Consider adjusting --scale or resizing the screenshot");

        // Still calculate diff for reference
        let diff_percent = compare::calculate_diff(&figma_img, &screenshot_img, args.tolerance);
        output::print_status(&format!("  Pixel diff: {:.2}%", diff_percent));

        if output::format() == crate::output::OutputFormat::Json {
            let out = CompareUrlOutput {
                file_key: parsed.file_key.clone(),
                node_id: node_id.clone(),
                screenshot: args.screenshot.display().to_string(),
                diff_percent,
                threshold: args.threshold,
                passed: false,
                dimensions_match: false,
                early_exit: false,
                diff_image: args.output.as_ref().map(|p| p.display().to_string()),
            };
            output::print_json(&out)?;
        }

        // Clean up temp file
        let _ = fs::remove_file(&figma_path);

        return Ok(());
    }

    // Calculate difference
    let diff_result = compare::calculate_diff_internal(
        &figma_img,
        &screenshot_img,
        args.tolerance,
        Some(args.threshold),
        args.fast && args.output.is_none(),
    )?;
    let diff_percent = diff_result.diff_percent;

    output::print_status("");
    if diff_percent <= args.threshold {
        output::print_status(&format!(
            "{} Pixel diff: {:.2}% (threshold: {:.1}%)",
            "✓".green().bold(),
            diff_percent,
            args.threshold
        ));
    } else {
        let suffix = if diff_result.early_exit {
            " (stopped early)"
        } else {
            ""
        };
        output::print_status(&format!(
            "{} Pixel diff: {:.2}% (threshold: {:.1}%){}",
            "✗".red().bold(),
            diff_percent,
            args.threshold,
            suffix
        ));
    }

    // Generate diff image if output specified
    if let Some(output_path) = &args.output {
        let diff_img = compare::generate_diff_image(&figma_img, &screenshot_img, args.tolerance);
        diff_img.save(output_path)?;
        output::print_status(&format!("  Diff image saved to: {}", output_path.display()));
    }

    if output::format() == crate::output::OutputFormat::Json {
        let out = CompareUrlOutput {
            file_key: parsed.file_key.clone(),
            node_id: node_id.clone(),
            screenshot: args.screenshot.display().to_string(),
            diff_percent,
            threshold: args.threshold,
            passed: diff_percent <= args.threshold,
            dimensions_match: fw == sw && fh == sh,
            early_exit: diff_result.early_exit,
            diff_image: args.output.as_ref().map(|p| p.display().to_string()),
        };
        output::print_json(&out)?;
    }

    // Clean up temp file
    let _ = fs::remove_file(&figma_path);

    // Exit with appropriate code for CI
    if diff_percent > args.threshold {
        anyhow::bail!("Pixel diff exceeded threshold");
    }

    Ok(())
}

#[derive(Serialize)]
struct CompareUrlOutput {
    file_key: String,
    node_id: String,
    screenshot: String,
    diff_percent: f32,
    threshold: f32,
    passed: bool,
    dimensions_match: bool,
    early_exit: bool,
    diff_image: Option<String>,
}
