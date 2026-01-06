use crate::cli::CompareArgs;
use crate::output;
use anyhow::{anyhow, Result};
use colored::Colorize;
use image::{GenericImageView, Rgba};
use serde::Serialize;
use std::fs;
use std::path::Path;

pub async fn run(args: CompareArgs) -> Result<()> {
    if !(0.0..=100.0).contains(&args.threshold) {
        anyhow::bail!("Threshold must be between 0 and 100");
    }
    if args.batch {
        batch_compare(
            &args.image1,
            &args.image2,
            args.output.as_deref(),
            args.report.as_deref(),
            args.threshold,
            args.tolerance,
            args.fast,
        )
        .await
    } else {
        single_compare(
            &args.image1,
            &args.image2,
            args.output.as_deref(),
            args.threshold,
            args.tolerance,
            args.fast,
        )
        .await
    }
}

async fn single_compare(
    image1_path: &Path,
    image2_path: &Path,
    output_path: Option<&Path>,
    threshold: f32,
    tolerance: u8,
    fast: bool,
) -> Result<()> {
    output::print_status(&"Comparing images...".bold().to_string());
    output::print_status(&format!("  Design:     {}", image1_path.display()));
    output::print_status(&format!("  Screenshot: {}", image2_path.display()));

    let img1 = image::open(image1_path)?;
    let img2 = image::open(image2_path)?;

    let (w1, h1) = img1.dimensions();
    let (w2, h2) = img2.dimensions();

    // Check dimensions
    if w1 != w2 || h1 != h2 {
        output::print_warning(&format!(
            "Dimension mismatch: {}x{} vs {}x{}",
            w1, h1, w2, h2
        ));
        // Still continue with comparison but warn
    }

    output::print_status(&format!("  Dimensions: {}x{} vs {}x{}", w1, h1, w2, h2));

    // Calculate pixel diff
    let result = calculate_diff_internal(
        &img1,
        &img2,
        tolerance,
        Some(threshold),
        fast && output_path.is_none(),
    )?;

    let diff_percent = result.diff_percent;
    let passed = diff_percent <= threshold;

    if passed {
        output::print_status(&format!(
            "  Pixel diff: {:.2}% {}",
            diff_percent,
            "(acceptable)".green()
        ));
    } else {
        let suffix = if result.early_exit {
            " (stopped early)"
        } else {
            ""
        };
        output::print_status(&format!(
            "  Pixel diff: {:.2}% {}{}",
            diff_percent,
            format!("(exceeds {:.1}% threshold)", threshold).red(),
            suffix
        ));
    }

    // Generate diff image if output path specified
    if let Some(output) = output_path {
        let diff_img = generate_diff_image(&img1, &img2, tolerance);
        diff_img.save(output)?;
        output::print_status(&format!(
            "  Diff image: {}",
            output.display().to_string().cyan()
        ));
    }

    if output::format() == crate::output::OutputFormat::Json {
        let result = SingleCompareOutput {
            image1: image1_path.display().to_string(),
            image2: image2_path.display().to_string(),
            diff_percent,
            threshold,
            passed,
            dimensions_match: result.dimensions_match,
            early_exit: result.early_exit,
            diff_image: output_path.map(|p| p.display().to_string()),
        };
        output::print_json(&result)?;
    }

    if !passed {
        anyhow::bail!("Pixel diff exceeded threshold");
    }

    Ok(())
}

async fn batch_compare(
    dir1: &Path,
    dir2: &Path,
    output_dir: Option<&Path>,
    report_path: Option<&Path>,
    threshold: f32,
    tolerance: u8,
    fast: bool,
) -> Result<()> {
    output::print_status(&"Batch comparing directories...".bold().to_string());
    output::print_status(&format!("  Design dir:     {}", dir1.display()));
    output::print_status(&format!("  Screenshot dir: {}", dir2.display()));

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
            output::print_status(&format!(
                "  {} - {}",
                filename.to_string_lossy().yellow(),
                "missing in screenshot dir"
            ));
            failed += 1;
            continue;
        }

        let img1 = image::open(&path1)?;
        let img2 = image::open(&path2)?;

        let diff_result = calculate_diff_internal(
            &img1,
            &img2,
            tolerance,
            Some(threshold),
            fast && output_dir.is_none(),
        )?;
        let passes = diff_result.diff_percent <= threshold;

        if passes {
            passed += 1;
            output::print_status(&format!(
                "  {} - {:.2}% {}",
                filename.to_string_lossy(),
                diff_result.diff_percent,
                "OK".green()
            ));
        } else {
            failed += 1;
            let suffix = if diff_result.early_exit {
                " (stopped early)"
            } else {
                ""
            };
            output::print_status(&format!(
                "  {} - {:.2}% {}{}",
                filename.to_string_lossy(),
                diff_result.diff_percent,
                "FAIL".red(),
                suffix
            ));
        }

        // Generate diff image
        if let Some(output) = output_dir {
            let diff_img = generate_diff_image(&img1, &img2, tolerance);
            let diff_filename = format!("diff-{}", filename.to_string_lossy());
            diff_img.save(output.join(&diff_filename))?;
        }

        results.push(CompareResult {
            file: filename.to_string_lossy().to_string(),
            diff_percent: diff_result.diff_percent,
            passed: passes,
            dimensions_match: diff_result.dimensions_match,
            early_exit: diff_result.early_exit,
        });
    }

    output::print_status("");
    output::print_status(&"Summary:".bold().to_string());
    output::print_status(&format!("  Passed: {}", passed.to_string().green()));
    output::print_status(&format!("  Failed: {}", failed.to_string().red()));

    let report_data = BatchReport {
        total: results.len(),
        passed,
        failed,
        threshold,
        results,
    };

    if let Some(report) = report_path {
        let json = serde_json::to_string_pretty(&report_data)?;
        fs::write(report, json)?;
        output::print_status(&format!(
            "  Report: {}",
            report.display().to_string().cyan()
        ));
    }

    if output::format() == crate::output::OutputFormat::Json {
        output::print_json(&report_data)?;
    }

    if failed > 0 {
        anyhow::bail!("One or more comparisons exceeded the threshold");
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct DiffResult {
    pub diff_percent: f32,
    pub dimensions_match: bool,
    pub early_exit: bool,
}

/// Calculate the percentage of pixels that differ between two images
/// Returns a value from 0.0 to 100.0
pub fn calculate_diff(
    img1: &image::DynamicImage,
    img2: &image::DynamicImage,
    tolerance: u8,
) -> f32 {
    calculate_diff_internal(img1, img2, tolerance, None, false)
        .map(|r| r.diff_percent)
        .unwrap_or(100.0)
}

/// Generate a visual diff image highlighting differences in red
pub fn generate_diff_image(
    img1: &image::DynamicImage,
    img2: &image::DynamicImage,
    tolerance: u8,
) -> image::RgbaImage {
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

                if pixels_similar(&p1, &p2, tolerance) {
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

    diff_img
}

pub fn calculate_diff_internal(
    img1: &image::DynamicImage,
    img2: &image::DynamicImage,
    tolerance: u8,
    threshold: Option<f32>,
    fast: bool,
) -> Result<DiffResult> {
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

    let early_exit_limit = if fast {
        threshold.map(|t| ((t as f64 / 100.0) * total_pixels).ceil() as u64)
    } else {
        None
    };

    let mut diff_pixels = 0u64;

    for y in 0..height {
        for x in 0..width {
            let p1 = img1.get_pixel(x, y);
            let p2 = img2.get_pixel(x, y);

            if !pixels_similar(&p1, &p2, tolerance) {
                diff_pixels += 1;
                if let Some(limit) = early_exit_limit {
                    if diff_pixels > limit {
                        let diff_percent = (diff_pixels as f64 / total_pixels * 100.0) as f32;
                        return Ok(DiffResult {
                            diff_percent,
                            dimensions_match,
                            early_exit: true,
                        });
                    }
                }
            }
        }
    }

    let diff_percent = (diff_pixels as f64 / total_pixels * 100.0) as f32;

    Ok(DiffResult {
        diff_percent,
        dimensions_match,
        early_exit: false,
    })
}

fn pixels_similar(p1: &Rgba<u8>, p2: &Rgba<u8>, tolerance: u8) -> bool {
    let diff_r = (p1[0] as i16 - p2[0] as i16).unsigned_abs() as u8;
    let diff_g = (p1[1] as i16 - p2[1] as i16).unsigned_abs() as u8;
    let diff_b = (p1[2] as i16 - p2[2] as i16).unsigned_abs() as u8;

    diff_r <= tolerance && diff_g <= tolerance && diff_b <= tolerance
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
    early_exit: bool,
}

#[derive(Serialize)]
struct SingleCompareOutput {
    image1: String,
    image2: String,
    diff_percent: f32,
    threshold: f32,
    passed: bool,
    dimensions_match: bool,
    early_exit: bool,
    diff_image: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgba, RgbaImage};

    fn image_from_pixels(pixels: &[[u8; 4]], width: u32, height: u32) -> DynamicImage {
        let mut img = RgbaImage::new(width, height);
        for (i, pixel) in pixels.iter().enumerate() {
            let x = (i as u32) % width;
            let y = (i as u32) / width;
            img.put_pixel(x, y, Rgba(*pixel));
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn diff_identical_is_zero() {
        let pixels = vec![
            [0, 0, 0, 255],
            [10, 10, 10, 255],
            [20, 20, 20, 255],
            [30, 30, 30, 255],
        ];
        let img1 = image_from_pixels(&pixels, 2, 2);
        let img2 = image_from_pixels(&pixels, 2, 2);
        let result = calculate_diff_internal(&img1, &img2, 0, None, false).unwrap();
        assert_eq!(result.diff_percent, 0.0);
    }

    #[test]
    fn diff_single_pixel_is_25_percent() {
        let pixels1 = vec![
            [0, 0, 0, 255],
            [0, 0, 0, 255],
            [0, 0, 0, 255],
            [0, 0, 0, 255],
        ];
        let pixels2 = vec![
            [0, 0, 0, 255],
            [255, 0, 0, 255],
            [0, 0, 0, 255],
            [0, 0, 0, 255],
        ];
        let img1 = image_from_pixels(&pixels1, 2, 2);
        let img2 = image_from_pixels(&pixels2, 2, 2);
        let result = calculate_diff_internal(&img1, &img2, 0, None, false).unwrap();
        assert!((result.diff_percent - 25.0).abs() < 0.01);
    }

    #[test]
    fn early_exit_triggers_when_threshold_exceeded() {
        let pixels1 = vec![[0, 0, 0, 255]; 4];
        let pixels2 = vec![
            [255, 0, 0, 255],
            [255, 0, 0, 255],
            [0, 0, 0, 255],
            [0, 0, 0, 255],
        ];
        let img1 = image_from_pixels(&pixels1, 2, 2);
        let img2 = image_from_pixels(&pixels2, 2, 2);
        let result = calculate_diff_internal(&img1, &img2, 0, Some(10.0), true).unwrap();
        assert!(result.early_exit);
        assert!(result.diff_percent > 10.0);
    }
}
