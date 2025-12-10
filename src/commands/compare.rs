use crate::cli::CompareArgs;
use anyhow::{anyhow, Result};
use colored::Colorize;
use image::{GenericImageView, Rgba};
use serde::Serialize;
use std::fs;
use std::path::Path;

pub async fn run(args: CompareArgs) -> Result<()> {
    if args.batch {
        batch_compare(&args.image1, &args.image2, args.output.as_deref(), args.report.as_deref(), args.threshold).await
    } else {
        single_compare(&args.image1, &args.image2, args.output.as_deref(), args.threshold).await
    }
}

async fn single_compare(
    image1_path: &Path,
    image2_path: &Path,
    output_path: Option<&Path>,
    threshold: f32,
) -> Result<()> {
    println!("{}", "Comparing images...".bold());
    println!("  Design:     {}", image1_path.display());
    println!("  Screenshot: {}", image2_path.display());

    let img1 = image::open(image1_path)?;
    let img2 = image::open(image2_path)?;

    let (w1, h1) = img1.dimensions();
    let (w2, h2) = img2.dimensions();

    // Check dimensions
    if w1 != w2 || h1 != h2 {
        println!(
            "{}",
            format!(
                "Dimension mismatch: {}x{} vs {}x{}",
                w1, h1, w2, h2
            )
            .yellow()
        );
        // Still continue with comparison but warn
    }

    println!("  Dimensions: {}x{} vs {}x{}", w1, h1, w2, h2);

    // Calculate pixel diff
    let result = calculate_diff(&img1, &img2)?;

    let diff_percent = result.diff_percent;
    let passed = diff_percent <= threshold;

    if passed {
        println!(
            "  Pixel diff: {:.2}% {}",
            diff_percent,
            "(acceptable)".green()
        );
    } else {
        println!(
            "  Pixel diff: {:.2}% {}",
            diff_percent,
            format!("(exceeds {:.1}% threshold)", threshold).red()
        );
    }

    // Generate diff image if output path specified
    if let Some(output) = output_path {
        let diff_img = generate_diff_image(&img1, &img2)?;
        diff_img.save(output)?;
        println!("  Diff image: {}", output.display().to_string().cyan());
    }

    // Exit with code 1 if threshold exceeded
    if !passed {
        std::process::exit(1);
    }

    Ok(())
}

async fn batch_compare(
    dir1: &Path,
    dir2: &Path,
    output_dir: Option<&Path>,
    report_path: Option<&Path>,
    threshold: f32,
) -> Result<()> {
    println!("{}", "Batch comparing directories...".bold());
    println!("  Design dir:     {}", dir1.display());
    println!("  Screenshot dir: {}", dir2.display());

    // Create output directory if specified
    if let Some(output) = output_dir {
        fs::create_dir_all(output)?;
    }

    let mut results = Vec::new();
    let mut passed = 0;
    let mut failed = 0;

    // Find matching images
    for entry in fs::read_dir(dir1)? {
        let entry = entry?;
        let path1 = entry.path();

        if !is_image(&path1) {
            continue;
        }

        let filename = path1.file_name().unwrap();
        let path2 = dir2.join(filename);

        if !path2.exists() {
            println!("  {} - {}", filename.to_string_lossy().yellow(), "missing in screenshot dir");
            failed += 1;
            continue;
        }

        let img1 = image::open(&path1)?;
        let img2 = image::open(&path2)?;

        let diff_result = calculate_diff(&img1, &img2)?;
        let passes = diff_result.diff_percent <= threshold;

        if passes {
            passed += 1;
            print!("  {} - ", filename.to_string_lossy());
            println!("{:.2}% {}", diff_result.diff_percent, "OK".green());
        } else {
            failed += 1;
            print!("  {} - ", filename.to_string_lossy());
            println!("{:.2}% {}", diff_result.diff_percent, "FAIL".red());
        }

        // Generate diff image
        if let Some(output) = output_dir {
            let diff_img = generate_diff_image(&img1, &img2)?;
            let diff_filename = format!("diff-{}", filename.to_string_lossy());
            diff_img.save(output.join(&diff_filename))?;
        }

        results.push(CompareResult {
            file: filename.to_string_lossy().to_string(),
            diff_percent: diff_result.diff_percent,
            passed: passes,
            dimensions_match: diff_result.dimensions_match,
        });
    }

    println!();
    println!("{}", "Summary:".bold());
    println!("  Passed: {}", passed.to_string().green());
    println!("  Failed: {}", failed.to_string().red());

    // Write report if specified
    if let Some(report) = report_path {
        let report_data = BatchReport {
            total: results.len(),
            passed,
            failed,
            threshold,
            results,
        };
        let json = serde_json::to_string_pretty(&report_data)?;
        fs::write(report, json)?;
        println!("  Report: {}", report.display().to_string().cyan());
    }

    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

struct DiffResult {
    diff_percent: f32,
    dimensions_match: bool,
}

fn calculate_diff(img1: &image::DynamicImage, img2: &image::DynamicImage) -> Result<DiffResult> {
    let (w1, h1) = img1.dimensions();
    let (w2, h2) = img2.dimensions();

    let dimensions_match = w1 == w2 && h1 == h2;

    // Use the smaller dimensions for comparison
    let width = w1.min(w2);
    let height = h1.min(h2);
    let total_pixels = (width * height) as f64;

    if total_pixels == 0.0 {
        return Err(anyhow!("Images have zero dimensions"));
    }

    let mut diff_pixels = 0u64;

    for y in 0..height {
        for x in 0..width {
            let p1 = img1.get_pixel(x, y);
            let p2 = img2.get_pixel(x, y);

            if !pixels_similar(&p1, &p2, 10) {
                diff_pixels += 1;
            }
        }
    }

    let diff_percent = (diff_pixels as f64 / total_pixels * 100.0) as f32;

    Ok(DiffResult {
        diff_percent,
        dimensions_match,
    })
}

fn pixels_similar(p1: &Rgba<u8>, p2: &Rgba<u8>, tolerance: u8) -> bool {
    let diff_r = (p1[0] as i16 - p2[0] as i16).unsigned_abs() as u8;
    let diff_g = (p1[1] as i16 - p2[1] as i16).unsigned_abs() as u8;
    let diff_b = (p1[2] as i16 - p2[2] as i16).unsigned_abs() as u8;

    diff_r <= tolerance && diff_g <= tolerance && diff_b <= tolerance
}

fn generate_diff_image(
    img1: &image::DynamicImage,
    img2: &image::DynamicImage,
) -> Result<image::RgbaImage> {
    let (w1, h1) = img1.dimensions();
    let (w2, h2) = img2.dimensions();

    let width = w1.max(w2);
    let height = h1.max(h2);

    let mut diff_img = image::RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let in_bounds1 = x < w1 && y < h1;
            let in_bounds2 = x < w2 && y < h2;

            let pixel = if in_bounds1 && in_bounds2 {
                let p1 = img1.get_pixel(x, y);
                let p2 = img2.get_pixel(x, y);

                if pixels_similar(&p1, &p2, 10) {
                    // Same - show dimmed original
                    Rgba([p1[0] / 2, p1[1] / 2, p1[2] / 2, 255])
                } else {
                    // Different - highlight in red
                    Rgba([255, 0, 0, 200])
                }
            } else if in_bounds1 {
                // Only in image 1 - show in blue
                Rgba([0, 0, 255, 200])
            } else if in_bounds2 {
                // Only in image 2 - show in green
                Rgba([0, 255, 0, 200])
            } else {
                Rgba([0, 0, 0, 0])
            };

            diff_img.put_pixel(x, y, pixel);
        }
    }

    Ok(diff_img)
}

fn is_image(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp"),
        None => false,
    }
}

#[derive(Serialize)]
struct BatchReport {
    total: usize,
    passed: usize,
    failed: usize,
    threshold: f32,
    results: Vec<CompareResult>,
}

#[derive(Serialize)]
struct CompareResult {
    file: String,
    diff_percent: f32,
    passed: bool,
    dimensions_match: bool,
}
