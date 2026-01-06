use crate::api::{FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::{ImageProtocol, PreviewArgs};
use crate::config::Config;
use crate::output;
use anyhow::Result;
use colored::Colorize;
use image::GenericImageView;
use viuer::{Config as ViuConfig, print_from_file};

pub async fn run(args: PreviewArgs) -> Result<()> {
    let token = get_token()?;
    let client = FigmaClient::new(token)?;
    let app_config = Config::load().unwrap_or_default();

    // Parse URL or file key
    let parsed = FigmaUrl::parse(&args.file_key)?;
    let node_id = args.node.or(parsed.node_id);

    output::print_status(&"Fetching preview...".bold().to_string());

    // Get node ID - either from args or get first frame
    let target_node = if let Some(id) = node_id {
        id
    } else {
        // Get file and find first frame
        let file = client.get_file(&parsed.file_key).await?;
        output::print_status(&format!("  File: {}", file.name.cyan()));

        // Find first frame in the document
        let first_frame = find_first_frame(&file.document);
        match first_frame {
            Some(id) => {
                output::print_status(&format!("  Using first frame: {}", id.dimmed()));
                id
            }
            None => {
                anyhow::bail!("No frames found in document. Use --node to specify a node ID.");
            }
        }
    };

    // Export the node as PNG
    let scale = app_config.export.default_scale;
    if !(1.0..=4.0).contains(&scale) {
        anyhow::bail!("Scale must be between 1 and 4");
    }
    let images = client
        .export_images(&parsed.file_key, &[target_node.clone()], "png", scale)
        .await?;

    if let Some(err) = &images.err {
        anyhow::bail!("API Error: {}", err);
    }

    // Get the image URL
    let image_url = images
        .images
        .get(&target_node)
        .and_then(|u| u.as_ref())
        .ok_or_else(|| anyhow::anyhow!("No image URL returned for node {}", target_node))?;

    // Download the image
    let image_bytes = client.download_image(image_url).await?;

    // Load image and display
    let img = image::load_from_memory(&image_bytes)?;
    let (width, height) = img.dimensions();

    output::print_status(&format!("  Dimensions: {}x{}", width, height));
    output::print_status("");

    // Configure viuer based on protocol
    let protocol = args
        .protocol
        .or_else(|| ImageProtocol::from_config(&app_config.defaults.image_protocol))
        .unwrap_or(ImageProtocol::Auto);
    let config = build_viuer_config(&protocol, args.width);

    // Save to temp file for viuer (it works better with files)
    let temp_path = std::env::temp_dir().join("fgm-preview.png");
    img.save(&temp_path)?;

    // Display the image
    match print_from_file(&temp_path, &config) {
        Ok(_) => {}
        Err(e) => {
            output::print_warning(&format!("Preview failed: {}", e));
            output::print_status("Try specifying a protocol: --protocol iterm | kitty | sixel");
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&temp_path);

    Ok(())
}

fn find_first_frame(document: &crate::api::types::Document) -> Option<String> {
    if let Some(children) = &document.children {
        for page in children {
            if page.node_type == "CANVAS" {
                if let Some(frames) = &page.children {
                    for frame in frames {
                        if frame.node_type == "FRAME" || frame.node_type == "COMPONENT" {
                            return Some(frame.id.clone());
                        }
                    }
                }
            }
        }
    }
    None
}

fn build_viuer_config(protocol: &ImageProtocol, width: Option<u32>) -> ViuConfig {
    let mut config = ViuConfig::default();

    // Set width if specified
    if let Some(w) = width {
        config.width = Some(w);
    }

    // Set protocol preference
    match protocol {
        ImageProtocol::Auto => {
            // Let viuer auto-detect
        }
        ImageProtocol::Iterm => {
            config.use_iterm = true;
            config.use_kitty = false;
        }
        ImageProtocol::Kitty => {
            config.use_kitty = true;
            config.use_iterm = false;
        }
        ImageProtocol::Sixel => {
            config.use_kitty = false;
            config.use_iterm = false;
            // viuer will fall back to sixel if available
        }
    }

    config
}
