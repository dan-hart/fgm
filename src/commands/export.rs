use crate::api::rate_limit::RateLimitTelemetry;
use crate::api::{CacheKey, CacheTTL, FigmaClient, FigmaUrl};
use crate::auth::get_token;
use crate::cli::{ExportCommands, ExportFormat, ExportProfile, Platform};
use crate::config::Config;
use crate::output;
use anyhow::{anyhow, Result};
use clap::Parser;
use colored::Colorize;
use image::GenericImageView;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const INITIAL_BATCH_SIZE: usize = 20;
const MIN_BATCH_SIZE: usize = 5;
const MAX_BATCH_SIZE: usize = 40;
const DOWNLOAD_STATUS_INTERVAL: usize = 5;

#[derive(Clone)]
struct ResolvedFileOptions {
    format: String,
    scale: f32,
    output: PathBuf,
    llm_pack: bool,
    manifest_name: String,
    resume: bool,
    delta: bool,
    profile: Option<ExportProfile>,
    low_rate: bool,
    source_input: String,
    quick_mode: bool,
}

#[derive(Parser, Debug)]
struct QuickExportArgs {
    /// Figma file key or URL to export
    input: String,
    /// Output directory
    #[arg(short, long, help = "Where to save exported files")]
    output: Option<PathBuf>,
    /// Image format: png, svg, pdf, jpg
    #[arg(short, long, help = "Output format")]
    format: Option<ExportFormat>,
    /// Scale factor (1-4)
    #[arg(short, long, value_parser = clap::value_parser!(f32), help = "Scale multiplier (1-4)")]
    scale: Option<f32>,
    /// Emit an LLM-focused manifest JSON alongside exported images
    #[arg(long, help = "Write manifest JSON with metadata for LLM workflows")]
    llm_pack: bool,
    /// Manifest filename for --llm-pack
    #[arg(
        long,
        default_value = "manifest.json",
        requires = "llm_pack",
        help = "Manifest filename when --llm-pack is enabled"
    )]
    manifest_name: String,
    /// Skip rewriting image files when content is unchanged
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = "Resume mode: skip unchanged files"
    )]
    resume: bool,
    /// Skip export URL/image fetches when file version is unchanged
    #[arg(long, help = "Delta mode: skip unchanged file versions")]
    delta: bool,
    /// Apply a preset export profile
    #[arg(long, value_enum, help = "Export profile preset")]
    profile: Option<ExportProfile>,
}

#[derive(Debug, Clone, Serialize, Default)]
struct ExportTelemetry {
    api_calls: u64,
    export_batches: u64,
    cache_hits: u64,
    cache_misses: u64,
    download_requests: u64,
    skipped_writes: u64,
    elapsed_ms: u64,
    rate_limits: Option<RateLimitTelemetry>,
}

#[derive(Debug, Clone)]
struct PlannedAsset {
    order: usize,
    node_id: String,
    image_url: String,
    filename: String,
}

#[derive(Debug)]
struct DownloadedAsset {
    order: usize,
    node_id: String,
    image_url: String,
    filename: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Default)]
struct NodeContext {
    node_name: String,
    page_name: Option<String>,
    source_width: Option<u32>,
    source_height: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
struct ExportedAssetRecord {
    node_id: String,
    node_name: String,
    page_name: Option<String>,
    filename: String,
    relative_path: String,
    source_image_url: String,
    source_width: Option<u32>,
    source_height: Option<u32>,
    exported_width: Option<u32>,
    exported_height: Option<u32>,
    bytes: usize,
    content_hash: String,
    skipped_write: bool,
}

#[derive(Debug, Serialize)]
struct LlmPackManifest {
    generated_at: String,
    file_key: String,
    source_input: String,
    quick_mode: bool,
    profile: Option<String>,
    format: String,
    scale: f32,
    output_dir: String,
    telemetry: ExportTelemetry,
    assets: Vec<ExportedAssetRecord>,
}

#[derive(Debug, Serialize)]
struct ExportJsonSummary {
    file_key: String,
    source_input: String,
    output_dir: String,
    format: String,
    scale: f32,
    asset_count: usize,
    quick_mode: bool,
    llm_pack: bool,
    delta: bool,
    profile: Option<String>,
    telemetry: ExportTelemetry,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ResumeIndex {
    #[serde(default = "default_resume_index_version")]
    version: u8,
    #[serde(default)]
    file_version: Option<String>,
    #[serde(default)]
    files: HashMap<String, ResumeIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResumeIndexEntry {
    node_id: String,
    content_hash: String,
    updated_at: String,
}

fn default_resume_index_version() -> u8 {
    1
}

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
            llm_pack,
            manifest_name,
            resume,
            delta,
            profile,
        } => {
            let parsed = FigmaUrl::parse(&file_key_or_url)?;
            let mut node_ids = node;
            if let Some(url_node_id) = parsed.node_id {
                if !node_ids.contains(&url_node_id) {
                    node_ids.push(url_node_id);
                }
            }

            let options = resolve_file_options(
                &config,
                format,
                scale,
                output,
                llm_pack,
                manifest_name,
                resume,
                delta,
                profile,
                file_key_or_url,
                false,
            )?;

            if let Some(platform) = platform {
                if options.llm_pack {
                    output::print_warning("--llm-pack is ignored for --platform exports");
                }
                export_platform(
                    &client,
                    &parsed.file_key,
                    &node_ids,
                    all_frames,
                    &options.output,
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
                    name.as_deref(),
                    &options,
                )
                .await
            }
        }
        ExportCommands::Batch { manifest } => batch_export(&client, &manifest, &config).await,
    }
}

pub async fn run_quick(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        anyhow::bail!("Quick mode requires a file key or URL: fgm \"<url>\"");
    }

    let quick = QuickExportArgs::try_parse_from(
        std::iter::once("fgm-quick".to_string()).chain(args.clone()),
    )
    .map_err(|err| anyhow!(err.to_string()))?;

    if !looks_like_quick_input(&quick.input) {
        anyhow::bail!(
            "Unknown command '{}'. Use a Figma URL for quick mode, or run `fgm --help`.",
            quick.input
        );
    }

    let token = get_token()?;
    let client = FigmaClient::new(token)?;
    let config = Config::load().unwrap_or_default();

    let parsed = FigmaUrl::parse(&quick.input)?;
    let options = resolve_file_options(
        &config,
        quick.format,
        quick.scale,
        quick.output,
        quick.llm_pack,
        quick.manifest_name,
        quick.resume,
        quick.delta,
        quick.profile,
        quick.input,
        true,
    )?;

    export_file(&client, &parsed.file_key, &[], true, None, &options).await
}

fn resolve_file_options(
    config: &Config,
    format: Option<ExportFormat>,
    scale: Option<f32>,
    output: Option<PathBuf>,
    llm_pack: bool,
    manifest_name: String,
    resume: bool,
    delta: bool,
    profile: Option<ExportProfile>,
    source_input: String,
    quick_mode: bool,
) -> Result<ResolvedFileOptions> {
    let default_format =
        ExportFormat::from_config(&config.export.default_format).unwrap_or(ExportFormat::Png);

    let format_was_set = format.is_some();
    let scale_was_set = scale.is_some();
    let mut resolved_format = format.unwrap_or(default_format);
    let mut resolved_scale = scale.unwrap_or(config.export.default_scale);
    let mut resolved_llm_pack = llm_pack;
    let mut resolved_resume = resume;
    let mut resolved_delta = delta;
    let mut low_rate = false;

    if let Some(ExportProfile::PixelPerfect) = &profile {
        if !format_was_set {
            resolved_format = ExportFormat::Png;
        }
        if !scale_was_set {
            resolved_scale = 2.0;
        }
        if !llm_pack {
            resolved_llm_pack = true;
        }
        if !resume {
            resolved_resume = true;
        }
    }
    if let Some(ExportProfile::LowRate) = &profile {
        low_rate = true;
        if !format_was_set {
            resolved_format = ExportFormat::Png;
        }
        if !scale_was_set {
            resolved_scale = 1.0;
        }
        if !resume {
            resolved_resume = true;
        }
        if !delta {
            resolved_delta = true;
        }
    }

    if !(1.0..=4.0).contains(&resolved_scale) {
        anyhow::bail!("Scale must be between 1 and 4");
    }

    let output = output.unwrap_or_else(|| {
        config
            .export
            .output_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if quick_mode {
                    PathBuf::from("./fgm-exports")
                } else {
                    PathBuf::from(".")
                }
            })
    });

    Ok(ResolvedFileOptions {
        format: resolved_format.to_string(),
        scale: resolved_scale,
        output,
        llm_pack: resolved_llm_pack,
        manifest_name,
        resume: resolved_resume,
        delta: resolved_delta,
        profile,
        low_rate,
        source_input,
        quick_mode,
    })
}

async fn export_file(
    client: &FigmaClient,
    file_key: &str,
    node_ids: &[String],
    all_frames: bool,
    custom_name: Option<&str>,
    options: &ResolvedFileOptions,
) -> Result<()> {
    fs::create_dir_all(&options.output)?;

    let start = Instant::now();
    let mut telemetry = ExportTelemetry::default();

    if all_frames {
        output::print_status("Discovering top-level frames...");
    }

    let mut ids_to_export: Vec<String> = if all_frames {
        telemetry.api_calls += 1;
        list_top_level_frame_ids(client, file_key).await?
    } else if node_ids.is_empty() {
        anyhow::bail!("No nodes specified. Use --node, --all-frames, or a URL with ?node-id=");
    } else {
        node_ids.to_vec()
    };

    ids_to_export.sort();
    ids_to_export.dedup();

    if ids_to_export.is_empty() {
        anyhow::bail!("No frames found to export");
    }

    let resume_index_path = options.output.join(".fgm-export-index.json");
    let mut resume_index = if options.resume || options.delta {
        load_resume_index(&resume_index_path)?
    } else {
        ResumeIndex::default()
    };

    let current_file_version = if options.delta {
        telemetry.api_calls = telemetry.api_calls.saturating_add(1);
        client.get_file(file_key).await.ok().map(|file| file.version)
    } else {
        None
    };

    if should_skip_delta_export(
        options,
        &resume_index,
        current_file_version.as_deref(),
        &ids_to_export,
        custom_name,
    ) {
        telemetry.elapsed_ms = start.elapsed().as_millis() as u64;
        telemetry.skipped_writes = ids_to_export.len() as u64;
        telemetry.rate_limits = Some(client.rate_limit_telemetry().await);

        if output::format() == crate::output::OutputFormat::Json {
            let summary = ExportJsonSummary {
                file_key: file_key.to_string(),
                source_input: options.source_input.clone(),
                output_dir: options.output.display().to_string(),
                format: options.format.clone(),
                scale: options.scale,
                asset_count: ids_to_export.len(),
                quick_mode: options.quick_mode,
                llm_pack: options.llm_pack,
                delta: options.delta,
                profile: options.profile.as_ref().map(profile_name),
                telemetry,
            };
            output::print_json(&summary)?;
        } else {
            output::print_status(&format!(
                "Delta mode: file version unchanged ({}), skipping export API calls",
                current_file_version.as_deref().unwrap_or("unknown")
            ));
        }

        output::print_success(&format!("Exported to {}", options.output.display()));
        return Ok(());
    }

    output::print_status(
        &format!(
            "Exporting {} node(s) as {} at {}x to {}...",
            ids_to_export.len(),
            options.format,
            options.scale,
            options.output.display()
        )
        .bold()
        .to_string(),
    );
    output::print_status("Resolving image URLs with adaptive batching...");

    let mut all_images: HashMap<String, Option<String>> = HashMap::new();
    let mut batch_size = initial_batch_size(options.low_rate, ids_to_export.len());
    let mut cursor = 0usize;

    while cursor < ids_to_export.len() {
        let mut retry_count = 0u8;
        let chunk_end = (cursor + batch_size).min(ids_to_export.len());
        let current_chunk: Vec<String> = ids_to_export[cursor..chunk_end].to_vec();
        let used_full_batch = current_chunk.len() == batch_size;

        let mut saw_rate_limit = false;

        loop {
            let cache_key = CacheKey::Images(
                file_key.to_string(),
                CacheKey::hash_export_params(&current_chunk, &options.format, options.scale),
            );
            if client.cache().contains(&cache_key) {
                telemetry.cache_hits = telemetry.cache_hits.saturating_add(1);
            } else {
                telemetry.cache_misses = telemetry.cache_misses.saturating_add(1);
            }

            telemetry.export_batches = telemetry.export_batches.saturating_add(1);
            telemetry.api_calls = telemetry.api_calls.saturating_add(1);

            let before = client.rate_limit_telemetry().await;
            let result = client
                .export_images(file_key, &current_chunk, &options.format, options.scale)
                .await;
            let after = client.rate_limit_telemetry().await;

            if after.total_retries > before.total_retries
                || after.total_rate_limited_responses > before.total_rate_limited_responses
            {
                saw_rate_limit = true;
            }

            match result {
                Ok(images) => {
                    if let Some(err) = &images.err {
                        if is_rate_limit_message(err) && retry_count < 3 {
                            retry_count += 1;
                            batch_size = next_batch_size_for_mode(batch_size, true, false, options);
                            let delay_ms = retry_after_delay_ms(client, options).await;
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                            continue;
                        }

                        output::print_warning(&format!("API Error: {}", err));
                        if images.status == Some(400) || images.status == Some(404) {
                            output::print_warning("Some node IDs may be invalid or inaccessible");
                        }
                    } else {
                        all_images.extend(images.images);
                    }
                    break;
                }
                Err(err) => {
                    if is_rate_limit_message(&err.to_string()) && retry_count < 3 {
                        retry_count += 1;
                        saw_rate_limit = true;
                        batch_size = next_batch_size_for_mode(batch_size, true, false, options);
                        let delay_ms = retry_after_delay_ms(client, options).await;
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                        continue;
                    }
                    return Err(err);
                }
            }
        }

        batch_size = next_batch_size_for_mode(batch_size, saw_rate_limit, used_full_batch, options);
        cursor = chunk_end;
        output::print_status(&format_resolution_progress(
            cursor,
            ids_to_export.len(),
            batch_size,
        ));

        if cursor < ids_to_export.len() {
            tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        }
    }

    if all_images.is_empty() {
        anyhow::bail!("No images were exported");
    }

    let mut planned_assets: Vec<PlannedAsset> = all_images
        .into_iter()
        .filter_map(|(node_id, url)| {
            url.map(|image_url| PlannedAsset {
                order: 0,
                filename: String::new(),
                node_id,
                image_url,
            })
        })
        .collect();

    planned_assets.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    let use_custom_name = custom_name.is_some() && planned_assets.len() == 1;
    for (i, asset) in planned_assets.iter_mut().enumerate() {
        asset.order = i;
        asset.filename = build_filename(
            &asset.node_id,
            i,
            custom_name,
            use_custom_name,
            &options.format,
        );
    }
    let total_assets = planned_assets.len();
    let mut download_concurrency = client.download_parallelism().max(1);
    if options.low_rate {
        download_concurrency = download_concurrency.min(4).max(1);
    }
    output::print_status(&format!(
        "Downloading {} image(s) with up to {} concurrent requests...",
        total_assets, download_concurrency
    ));

    let semaphore = Arc::new(Semaphore::new(download_concurrency));
    let mut joins = JoinSet::new();

    for asset in planned_assets {
        let sem = semaphore.clone();
        let client = client.clone();
        joins.spawn(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|_| anyhow!("download semaphore closed"))?;
            let bytes = client.download_image(&asset.image_url).await?;
            Ok::<DownloadedAsset, anyhow::Error>(DownloadedAsset {
                order: asset.order,
                node_id: asset.node_id,
                image_url: asset.image_url,
                filename: asset.filename,
                bytes,
            })
        });
    }

    let mut downloaded = Vec::new();
    let mut downloaded_count = 0usize;
    while let Some(join_result) = joins.join_next().await {
        let asset = join_result.map_err(|err| anyhow!("download task failed: {}", err))??;
        downloaded.push(asset);
        downloaded_count = downloaded_count.saturating_add(1);
        if should_emit_download_status(downloaded_count, total_assets) {
            output::print_status(&format_download_progress(downloaded_count, total_assets));
        }
    }

    downloaded.sort_by_key(|asset| asset.order);

    telemetry.download_requests = downloaded.len() as u64;
    telemetry.api_calls = telemetry
        .api_calls
        .saturating_add(telemetry.download_requests);

    let pb = if output::is_quiet() || output::format() == crate::output::OutputFormat::Json {
        ProgressBar::hidden()
    } else {
        let bar = ProgressBar::new(downloaded.len() as u64);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")?
                .progress_chars("#>-"),
        );
        bar
    };
    output::print_status("Writing files to disk...");

    let mut asset_records = Vec::new();
    for asset in downloaded {
        let filepath = options.output.join(&asset.filename);
        let content_hash = hash_bytes(&asset.bytes);
        let existing_hash = if options.resume {
            hash_existing_file(&filepath)?
        } else {
            None
        };

        let skipped_write =
            options.resume && existing_hash.as_deref() == Some(content_hash.as_str());
        if !skipped_write {
            fs::write(&filepath, &asset.bytes)?;
        } else {
            telemetry.skipped_writes = telemetry.skipped_writes.saturating_add(1);
        }

        resume_index.files.insert(
            asset.filename.clone(),
            ResumeIndexEntry {
                node_id: asset.node_id.clone(),
                content_hash: content_hash.clone(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        );

        let (exported_width, exported_height) = image::load_from_memory(&asset.bytes)
            .ok()
            .map(|img| img.dimensions())
            .map(|(w, h)| (Some(w), Some(h)))
            .unwrap_or((None, None));

        asset_records.push(ExportedAssetRecord {
            node_id: asset.node_id,
            node_name: String::new(),
            page_name: None,
            filename: asset.filename.clone(),
            relative_path: asset.filename.clone(),
            source_image_url: asset.image_url,
            source_width: None,
            source_height: None,
            exported_width,
            exported_height,
            bytes: asset.bytes.len(),
            content_hash,
            skipped_write,
        });

        pb.set_message(asset.filename);
        pb.inc(1);
    }

    pb.finish_with_message("done");

    if let Some(version) = current_file_version {
        resume_index.file_version = Some(version);
    }

    if options.resume || options.delta {
        save_resume_index(&resume_index_path, &resume_index)?;
    }

    let written_count = asset_records
        .len()
        .saturating_sub(telemetry.skipped_writes as usize);
    output::print_status(&format!(
        "Saved {} file(s), skipped {} unchanged file(s)",
        written_count, telemetry.skipped_writes
    ));

    let mut node_context = HashMap::new();
    if options.llm_pack {
        telemetry.api_calls = telemetry.api_calls.saturating_add(1);
        if let Ok(file) = client.get_file(file_key).await {
            node_context = build_node_context_map(&file.document);
        }
    }

    for record in &mut asset_records {
        if let Some(ctx) = node_context.get(&record.node_id) {
            record.node_name = ctx.node_name.clone();
            record.page_name = ctx.page_name.clone();
            record.source_width = ctx.source_width;
            record.source_height = ctx.source_height;
        } else {
            record.node_name = record.node_id.clone();
        }
    }

    telemetry.elapsed_ms = start.elapsed().as_millis() as u64;
    telemetry.rate_limits = Some(client.rate_limit_telemetry().await);

    if options.llm_pack {
        let manifest_path = options.output.join(&options.manifest_name);
        let manifest = LlmPackManifest {
            generated_at: chrono::Utc::now().to_rfc3339(),
            file_key: file_key.to_string(),
            source_input: options.source_input.clone(),
            quick_mode: options.quick_mode,
            profile: options.profile.as_ref().map(profile_name),
            format: options.format.clone(),
            scale: options.scale,
            output_dir: options.output.display().to_string(),
            telemetry: telemetry.clone(),
            assets: asset_records.clone(),
        };

        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
        output::print_status(&format!(
            "LLM pack manifest written: {}",
            manifest_path.display()
        ));
    }

    if output::format() == crate::output::OutputFormat::Json {
        let summary = ExportJsonSummary {
            file_key: file_key.to_string(),
            source_input: options.source_input.clone(),
            output_dir: options.output.display().to_string(),
            format: options.format.clone(),
            scale: options.scale,
            asset_count: asset_records.len(),
            quick_mode: options.quick_mode,
            llm_pack: options.llm_pack,
            delta: options.delta,
            profile: options.profile.as_ref().map(profile_name),
            telemetry: telemetry.clone(),
        };
        output::print_json(&summary)?;
    }

    output::print_success(&format!("Exported to {}", options.output.display()));
    Ok(())
}

fn build_filename(
    node_id: &str,
    index: usize,
    custom_name: Option<&str>,
    use_custom_name: bool,
    format: &str,
) -> String {
    if use_custom_name {
        format!("{}.{}", custom_name.unwrap_or("export"), format)
    } else if let Some(name) = custom_name {
        format!("{}-{}.{}", name, index + 1, format)
    } else {
        format!("{}.{}", node_id.replace(':', "-"), format)
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn hash_existing_file(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    Ok(Some(hash_bytes(&bytes)))
}

fn load_resume_index(path: &Path) -> Result<ResumeIndex> {
    if !path.exists() {
        return Ok(ResumeIndex::default());
    }
    let content = fs::read_to_string(path)?;
    let index: ResumeIndex = serde_json::from_str(&content).unwrap_or_default();
    Ok(index)
}

fn save_resume_index(path: &Path, index: &ResumeIndex) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(index)?)?;
    Ok(())
}

fn profile_name(profile: &ExportProfile) -> String {
    match profile {
        ExportProfile::PixelPerfect => "pixel-perfect".to_string(),
        ExportProfile::LowRate => "low-rate".to_string(),
    }
}

fn initial_batch_size(low_rate: bool, total_nodes: usize) -> usize {
    let base = if low_rate { 10 } else { INITIAL_BATCH_SIZE };
    base.min(total_nodes.max(1))
}

fn next_batch_size(current: usize, saw_rate_limit: bool, filled_batch: bool) -> usize {
    if saw_rate_limit {
        return (current / 2).max(MIN_BATCH_SIZE);
    }
    if filled_batch {
        return (current + 5).min(MAX_BATCH_SIZE);
    }
    current
}

fn next_batch_size_for_mode(
    current: usize,
    saw_rate_limit: bool,
    filled_batch: bool,
    options: &ResolvedFileOptions,
) -> usize {
    if !options.low_rate {
        return next_batch_size(current, saw_rate_limit, filled_batch);
    }

    if saw_rate_limit {
        return (current / 2).max(MIN_BATCH_SIZE);
    }
    if filled_batch {
        return (current + 3).min(20);
    }
    current
}

async fn retry_after_delay_ms(client: &FigmaClient, options: &ResolvedFileOptions) -> u64 {
    let fallback = if options.low_rate { 700 } else { 400 };
    let telemetry = client.rate_limit_telemetry().await;
    telemetry
        .retry_after
        .map(|seconds| seconds.saturating_mul(1000))
        .unwrap_or(fallback)
        .max(fallback)
}

fn should_skip_delta_export(
    options: &ResolvedFileOptions,
    resume_index: &ResumeIndex,
    current_file_version: Option<&str>,
    node_ids: &[String],
    custom_name: Option<&str>,
) -> bool {
    if !(options.delta && options.resume) {
        return false;
    }

    let Some(current_version) = current_file_version else {
        return false;
    };

    if resume_index.file_version.as_deref() != Some(current_version) {
        return false;
    }

    let use_custom_name = custom_name.is_some() && node_ids.len() == 1;
    for (index, node_id) in node_ids.iter().enumerate() {
        let filename = build_filename(
            node_id,
            index,
            custom_name,
            use_custom_name,
            &options.format,
        );
        let output_path = options.output.join(&filename);
        if !output_path.exists() {
            return false;
        }
        let Some(entry) = resume_index.files.get(&filename) else {
            return false;
        };
        if entry.node_id != *node_id {
            return false;
        }
    }

    true
}

fn format_resolution_progress(resolved: usize, total: usize, next_batch: usize) -> String {
    format!(
        "Resolved export URLs: {}/{} nodes (next batch {})",
        resolved, total, next_batch
    )
}

fn format_download_progress(downloaded: usize, total: usize) -> String {
    format!("Downloaded images: {}/{}", downloaded, total)
}

fn should_emit_download_status(downloaded: usize, total: usize) -> bool {
    downloaded == total || downloaded % DOWNLOAD_STATUS_INTERVAL == 0
}

fn is_rate_limit_message(msg: &str) -> bool {
    let lowered = msg.to_lowercase();
    lowered.contains("rate limit") || lowered.contains("429")
}

fn looks_like_quick_input(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.contains("figma.com/") || trimmed.starts_with("www.figma.com/") {
        return true;
    }

    // Figma file keys are typically long alphanumeric IDs.
    trimmed.len() >= 8 && trimmed.chars().all(|c| c.is_ascii_alphanumeric())
}

async fn list_top_level_frame_ids(client: &FigmaClient, file_key: &str) -> Result<Vec<String>> {
    if let Some(cached_file) = client
        .cache()
        .get::<crate::api::types::File>(&CacheKey::File(file_key.to_string()))
    {
        let cached_ids = extract_frame_ids(&cached_file.document);
        if !cached_ids.is_empty() {
            return Ok(cached_ids);
        }
    }

    let shallow_cache_key = CacheKey::FileMeta(format!("{}:depth2", file_key));
    if let Some((cached_shallow, _)) = client
        .cache()
        .get_with_freshness::<serde_json::Value>(&shallow_cache_key)
    {
        let ids = extract_frame_ids_from_json(&cached_shallow);
        if !ids.is_empty() {
            return Ok(ids);
        }
    }

    let shallow_url = format!("{}/files/{}?depth=2", client.base_url(), file_key);
    if let Ok(shallow) = client.get_json::<serde_json::Value>(&shallow_url).await {
        client
            .cache()
            .set(&shallow_cache_key, &shallow, CacheTTL::FILE_META_LIGHT);
        let ids = extract_frame_ids_from_json(&shallow);
        if !ids.is_empty() {
            return Ok(ids);
        }
    }

    let file = client.get_file(file_key).await?;
    Ok(extract_frame_ids(&file.document))
}

fn extract_frame_ids(document: &crate::api::types::Document) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(children) = &document.children {
        for child in children {
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

fn extract_frame_ids_from_json(file_json: &serde_json::Value) -> Vec<String> {
    let mut ids = Vec::new();
    let pages = file_json
        .get("document")
        .and_then(|v| v.get("children"))
        .and_then(|v| v.as_array());

    if let Some(pages) = pages {
        for page in pages {
            if page
                .get("type")
                .and_then(|v| v.as_str())
                .map(|t| t == "CANVAS")
                .unwrap_or(false)
            {
                if let Some(frames) = page.get("children").and_then(|v| v.as_array()) {
                    for frame in frames {
                        let node_type = frame.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if (node_type == "FRAME" || node_type == "COMPONENT")
                            && frame.get("id").and_then(|v| v.as_str()).is_some()
                        {
                            ids.push(frame["id"].as_str().unwrap_or_default().to_string());
                        }
                    }
                }
            }
        }
    }

    ids
}

fn build_node_context_map(document: &crate::api::types::Document) -> HashMap<String, NodeContext> {
    let mut map = HashMap::new();
    if let Some(pages) = &document.children {
        for page in pages {
            if page.node_type != "CANVAS" {
                continue;
            }
            let page_name = page.name.clone();
            if let Some(children) = &page.children {
                for node in children {
                    collect_node_context(node, Some(&page_name), &mut map);
                }
            }
        }
    }
    map
}

fn collect_node_context(
    node: &crate::api::types::Node,
    page_name: Option<&str>,
    out: &mut HashMap<String, NodeContext>,
) {
    let (source_width, source_height) = node
        .absolute_bounding_box
        .as_ref()
        .map(|bb| {
            (
                Some(bb.width.round() as u32),
                Some(bb.height.round() as u32),
            )
        })
        .unwrap_or((None, None));

    out.insert(
        node.id.clone(),
        NodeContext {
            node_name: node.name.clone(),
            page_name: page_name.map(str::to_string),
            source_width,
            source_height,
        },
    );

    if let Some(children) = &node.children {
        for child in children {
            collect_node_context(child, page_name, out);
        }
    }
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
        let parsed = FigmaUrl::parse(&export.file)?;
        let node_id = export.node.or(parsed.node_id).unwrap_or_default();
        let ids = if node_id.is_empty() {
            vec![]
        } else {
            vec![node_id]
        };

        let output = PathBuf::from(&export.output.unwrap_or_else(|| {
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

        let options = ResolvedFileOptions {
            format,
            scale,
            output,
            llm_pack: false,
            manifest_name: "manifest.json".to_string(),
            resume: false,
            delta: false,
            profile: None,
            low_rate: false,
            source_input: export.file,
            quick_mode: false,
        };

        export_file(
            client,
            &parsed.file_key,
            &ids,
            false,
            export.name.as_deref(),
            &options,
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
        list_top_level_frame_ids(client, file_key).await?
    } else if node_ids.is_empty() {
        anyhow::bail!("No nodes specified. Use --node, --all-frames, or a URL with ?node-id=");
    } else {
        node_ids.to_vec()
    };

    if ids_to_export.is_empty() {
        anyhow::bail!("No frames found to export");
    }

    let scales: Vec<(f32, &str)> = match platform {
        Platform::Ios => vec![(1.0, ""), (2.0, "@2x"), (3.0, "@3x")],
        Platform::Android => vec![
            (1.0, "mdpi"),
            (1.5, "hdpi"),
            (2.0, "xhdpi"),
            (3.0, "xxhdpi"),
            (4.0, "xxxhdpi"),
        ],
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

    match platform {
        Platform::Android => {
            for (_, suffix) in &scales {
                fs::create_dir_all(output.join(format!("drawable-{}", suffix)))?;
            }
        }
        _ => {
            fs::create_dir_all(output)?;
        }
    }

    for (scale, suffix) in &scales {
        output::print_status(&format!(
            "  Exporting at {}x ({})...",
            scale,
            if suffix.is_empty() { "1x" } else { suffix }
        ));

        let images = client
            .export_images(file_key, &ids_to_export, "png", *scale)
            .await?;

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
                        let filename = if suffix.is_empty() {
                            format!("{}.png", base_name)
                        } else {
                            format!("{}{}.png", base_name, suffix)
                        };
                        output.join(filename)
                    }
                    Platform::Android => {
                        let dir = output.join(format!("drawable-{}", suffix));
                        dir.join(format!("{}.png", base_name))
                    }
                    Platform::Web => {
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

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    output::print_success(&format!("Exported to {}", output.display()));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_batch_size_reduces_on_rate_limit() {
        assert_eq!(next_batch_size(20, true, true), 10);
        assert_eq!(next_batch_size(6, true, true), 5);
    }

    #[test]
    fn next_batch_size_grows_on_full_success() {
        assert_eq!(next_batch_size(20, false, true), 25);
        assert_eq!(next_batch_size(38, false, true), 40);
        assert_eq!(next_batch_size(40, false, true), 40);
    }

    #[test]
    fn build_filename_is_deterministic() {
        let a = build_filename("1:2", 0, None, false, "png");
        let b = build_filename("1:2", 0, None, false, "png");
        assert_eq!(a, b);
        assert_eq!(a, "1-2.png");
    }

    #[test]
    fn hash_export_content_changes_with_bytes() {
        let h1 = hash_bytes(b"abc");
        let h2 = hash_bytes(b"abcd");
        assert_ne!(h1, h2);
    }

    #[test]
    fn extract_frame_ids_from_depth_json_extracts_canvases() {
        let value = serde_json::json!({
            "document": {
                "children": [
                    {
                        "type": "CANVAS",
                        "children": [
                            { "id": "1:2", "type": "FRAME" },
                            { "id": "1:3", "type": "COMPONENT" },
                            { "id": "1:4", "type": "GROUP" }
                        ]
                    }
                ]
            }
        });

        let ids = extract_frame_ids_from_json(&value);
        assert_eq!(ids, vec!["1:2".to_string(), "1:3".to_string()]);
    }

    #[test]
    fn pixel_perfect_profile_applies_expected_defaults() {
        let config = Config::default();
        let options = resolve_file_options(
            &config,
            None,
            None,
            None,
            false,
            "manifest.json".to_string(),
            false,
            false,
            Some(ExportProfile::PixelPerfect),
            "abc123".to_string(),
            false,
        )
        .expect("options should resolve");

        assert_eq!(options.format, "png");
        assert_eq!(options.scale, 2.0);
        assert!(options.llm_pack);
        assert!(options.resume);
    }

    #[test]
    fn low_rate_profile_applies_delta_and_resume_defaults() {
        let config = Config::default();
        let options = resolve_file_options(
            &config,
            None,
            None,
            None,
            false,
            "manifest.json".to_string(),
            false,
            false,
            Some(ExportProfile::LowRate),
            "abc123".to_string(),
            true,
        )
        .expect("options should resolve");

        assert_eq!(options.format, "png");
        assert_eq!(options.scale, 1.0);
        assert!(options.resume);
        assert!(options.delta);
        assert!(options.low_rate);
    }

    #[test]
    fn delta_skip_requires_matching_version_and_outputs() {
        let mut index = ResumeIndex::default();
        index.file_version = Some("v1".to_string());
        index.files.insert(
            "1-2.png".to_string(),
            ResumeIndexEntry {
                node_id: "1:2".to_string(),
                content_hash: "abc".to_string(),
                updated_at: "2026-03-02T00:00:00Z".to_string(),
            },
        );

        let options = ResolvedFileOptions {
            format: "png".to_string(),
            scale: 1.0,
            output: PathBuf::from("."),
            llm_pack: false,
            manifest_name: "manifest.json".to_string(),
            resume: true,
            delta: true,
            profile: Some(ExportProfile::LowRate),
            low_rate: true,
            source_input: "abc123".to_string(),
            quick_mode: false,
        };

        assert!(!should_skip_delta_export(
            &options,
            &index,
            Some("v1"),
            &[String::from("1:2")],
            None
        ));
    }

    #[test]
    fn quick_input_validation_rejects_command_like_words() {
        assert!(!looks_like_quick_input("filess"));
        assert!(!looks_like_quick_input("export"));
        assert!(looks_like_quick_input("abc123def"));
        assert!(looks_like_quick_input(
            "https://www.figma.com/design/abc123/Name?node-id=1-2"
        ));
    }

    #[test]
    fn resolution_progress_message_is_readable() {
        assert_eq!(
            format_resolution_progress(20, 75, 25),
            "Resolved export URLs: 20/75 nodes (next batch 25)"
        );
    }

    #[test]
    fn download_progress_message_is_readable() {
        assert_eq!(
            format_download_progress(12, 40),
            "Downloaded images: 12/40"
        );
    }
}
