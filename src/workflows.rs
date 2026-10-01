use crate::{
    api::{FigmaClient, FigmaUrl},
    auth,
    cli::{Commands, ExportCommands},
    output,
};
use anyhow::{bail, Context, Result};
use clap::Args;
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn project() -> Result<Option<(PathBuf, Value)>> {
    let Some(path) = crate::project::find_project_file(&std::env::current_dir()?) else {
        return Ok(None);
    };
    let value: toml::Value = toml::from_str(&fs::read_to_string(&path)?)?;
    Ok(Some((path, serde_json::to_value(value)?)))
}

pub fn source(input: Option<&str>) -> Result<String> {
    let project = project()?;
    source_from(input, project.as_ref().map(|(_, p)| p))
}

fn source_from(input: Option<&str>, project: Option<&Value>) -> Result<String> {
    let value = input
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .or_else(|| project.and_then(|p| p["figma"]["source"].as_str().map(str::to_owned)))
        .context("Provide --source or set [figma].source in fgm.toml")?;
    Ok(project
        .and_then(|p| p["figma"]["aliases"][&value].as_str())
        .unwrap_or(&value)
        .to_owned())
}

pub fn apply_project_defaults(command: &mut Commands, matches: &clap::ArgMatches) -> Result<()> {
    if !matches!(
        command,
        Commands::Export { .. }
            | Commands::Compare(_)
            | Commands::CompareUrl(_)
            | Commands::Snapshot { .. }
    ) {
        return Ok(());
    }
    let bare = matches!(command, Commands::Export { command: None });
    if bare {
        use clap::Parser;
        *command = crate::cli::Cli::try_parse_from(["fgm", "export", "file"])?.command;
    }
    let Some((path, p)) = project()? else {
        if let Commands::Export {
            command: Some(ExportCommands::File {
                file_key_or_url, ..
            }),
        } = command
        {
            if file_key_or_url.is_empty() {
                bail!("Provide a file source or run fgm init --source <url>");
            }
        }
        return Ok(());
    };
    apply_defaults_from(command, matches, &path, &p, bare)
}

fn apply_defaults_from(
    command: &mut Commands,
    matches: &clap::ArgMatches,
    path: &Path,
    p: &Value,
    bare: bool,
) -> Result<()> {
    let root = path.parent().context("Invalid project path")?;
    match command {
        Commands::Export {
            command:
                Some(ExportCommands::File {
                    file_key_or_url,
                    output,
                    scale,
                    all_frames,
                    node,
                    pick,
                    ..
                }),
        } => {
            let from_defaults = file_key_or_url.is_empty();
            *file_key_or_url = source_from(Some(file_key_or_url), Some(p))?;
            if (bare || from_defaults)
                && node.is_empty()
                && !*pick
                && FigmaUrl::parse(file_key_or_url)?.node_id.is_none()
            {
                *all_frames = true;
            }
            if output.is_none() {
                *output = p["export"]["output_dir"].as_str().map(|s| root.join(s));
            }
            if scale.is_none() {
                *scale = p["export"]["scale"].as_f64().map(|s| s as f32);
            }
        }
        Commands::Compare(args) => {
            if matches
                .subcommand_matches("compare")
                .and_then(|m| m.value_source("threshold"))
                != Some(clap::parser::ValueSource::CommandLine)
            {
                if let Some(v) = p["compare"]["threshold"].as_f64() {
                    args.threshold = v as f32;
                }
            }
        }
        Commands::CompareUrl(args) => {
            args.figma_url = source_from(Some(&args.figma_url), Some(p))?;
            if matches
                .subcommand_matches("compare-url")
                .and_then(|m| m.value_source("threshold"))
                != Some(clap::parser::ValueSource::CommandLine)
            {
                if let Some(v) = p["compare"]["threshold"].as_f64() {
                    args.threshold = v as f32;
                }
            }
            if args.scale.is_none() {
                args.scale = p["export"]["scale"].as_f64().map(|v| v as f32);
            }
        }
        Commands::Snapshot { command } => {
            use crate::cli::SnapshotCommands;
            let (sub, field, target) = match command {
                SnapshotCommands::Create { output, .. } => ("create", "output", output),
                SnapshotCommands::List { dir } => ("list", "dir", dir),
                SnapshotCommands::Diff { dir, .. } => ("diff", "dir", dir),
            };
            if matches
                .subcommand_matches("snapshot")
                .and_then(|m| m.subcommand_matches(sub))
                .and_then(|m| m.value_source(field))
                != Some(clap::parser::ValueSource::CommandLine)
            {
                if let Some(dir) = p["snapshot"]["dir"].as_str() {
                    *target = root.join(dir);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Args)]
pub struct FindArgs {
    pub query: String,
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long)]
    pub page: Option<String>,
    #[arg(long)]
    pub exact: bool,
    #[arg(long)]
    pub save_as: Option<String>,
}

async fn document(input: Option<&str>) -> Result<(FigmaClient, FigmaUrl, Value)> {
    let parsed = FigmaUrl::parse(&source(input)?)?;
    let client = FigmaClient::new(auth::get_token()?)?;
    // Preserve layout, typography and component properties not modeled by the legacy API types.
    let file: Value = client
        .get_json(&format!("{}/files/{}", client.base_url(), parsed.file_key))
        .await?;
    Ok((client, parsed, file))
}

fn nodes<'a>(node: &'a Value, page: &str, found: &mut Vec<(&'a Value, String)>) {
    let page = if node["type"] == "CANVAS" {
        node["name"].as_str().unwrap_or("")
    } else {
        page
    };
    if ["FRAME", "COMPONENT", "COMPONENT_SET"]
        .iter()
        .any(|t| node["type"] == *t)
    {
        found.push((node, page.to_owned()));
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            nodes(child, page, found);
        }
    }
}

fn selected<'a>(
    file: &'a Value,
    query: Option<&str>,
    page: Option<&str>,
    exact: bool,
) -> Vec<(&'a Value, String)> {
    let mut all = vec![];
    nodes(&file["document"], "", &mut all);
    all.retain(|(n, p)| {
        page.is_none_or(|v| p.eq_ignore_ascii_case(v))
            && query.is_none_or(|q| {
                let name = n["name"].as_str().unwrap_or("");
                if exact {
                    name.eq_ignore_ascii_case(q)
                } else {
                    name.to_lowercase().contains(&q.to_lowercase())
                }
            })
    });
    all.sort_by(|a, b| a.0["id"].as_str().cmp(&b.0["id"].as_str()));
    all
}

fn link(key: &str, id: &str) -> String {
    format!(
        "https://www.figma.com/design/{key}?node-id={}",
        urlencoding::encode(id)
    )
}

pub async fn find(args: FindArgs) -> Result<()> {
    let (_, parsed, file) = document(args.source.as_deref()).await?;
    let found = selected(&file, Some(&args.query), args.page.as_deref(), args.exact);
    if found.is_empty() {
        bail!("No frames or components matched");
    }
    let results: Vec<_> = found.iter().map(|(n,p)| json!({"id":n["id"],"name":n["name"],"page":p,"url":link(&parsed.file_key,n["id"].as_str().unwrap_or(""))})).collect();
    output::print_json(&results)?;
    if let Some(alias) = args.save_as {
        if found.len() != 1 {
            bail!(
                "Ambiguous selection: {} matches; use --exact and --page before saving an alias",
                found.len()
            );
        }
        let (path, _) = project()?.context("Run fgm init before saving aliases")?;
        let mut config: toml::Value = toml::from_str(&fs::read_to_string(&path)?)?;
        let figma = config
            .as_table_mut()
            .context("Invalid project config")?
            .entry("figma")
            .or_insert_with(|| toml::Value::Table(Default::default()));
        let aliases = figma
            .as_table_mut()
            .context("Invalid figma section")?
            .entry("aliases")
            .or_insert_with(|| toml::Value::Table(Default::default()));
        aliases
            .as_table_mut()
            .context("Invalid aliases section")?
            .insert(
                alias,
                toml::Value::String(results[0]["url"].as_str().unwrap().to_owned()),
            );
        fs::write(path, toml::to_string_pretty(&config)?)?;
    }
    Ok(())
}

#[derive(Args, Clone, Default)]
pub struct DeviceArgs {
    /// Simulator UDID or booted
    #[arg(long, conflicts_with = "android")]
    pub simulator: Option<String>,
    /// Android serial or 'connected' for adb's default device
    #[arg(long, conflicts_with = "simulator")]
    pub android: Option<String>,
}

#[derive(Args)]
pub struct CaptureArgs {
    #[command(flatten)]
    pub device: DeviceArgs,
    #[arg(short, long)]
    pub output: PathBuf,
}

pub async fn capture(device: &DeviceArgs) -> Result<DynamicImage> {
    let mut cmd;
    if let Some(id) = &device.simulator {
        // simctl writes to a file rather than stdout; use a unique path and clean it on every outcome.
        let path = std::env::temp_dir().join(format!(
            "fgm-capture-{}-{}.png",
            std::process::id(),
            rand::random::<u64>()
        ));
        cmd = tokio::process::Command::new("xcrun");
        cmd.args(["simctl", "io", id, "screenshot"])
            .arg(&path)
            .kill_on_drop(true);
        let result = tokio::time::timeout(Duration::from_secs(60), cmd.output()).await;
        let image = match result {
            Ok(Ok(out)) if out.status.success() => {
                image::open(&path).context("Invalid simulator screenshot")
            }
            Ok(Ok(_)) => Err(anyhow::anyhow!(
                "Simulator capture failed; check the simulator is booted"
            )),
            Ok(Err(err)) => Err(err.into()),
            Err(_) => Err(anyhow::anyhow!("Simulator capture timed out")),
        };
        let _ = fs::remove_file(path);
        return image;
    } else if let Some(id) = &device.android {
        cmd = tokio::process::Command::new("adb");
        if id != "connected" {
            cmd.args(["-s", id]);
        }
        cmd.args(["exec-out", "screencap", "-p"]).kill_on_drop(true);
    } else {
        bail!("Select --simulator or --android");
    }
    let out = tokio::time::timeout(Duration::from_secs(60), cmd.output())
        .await
        .context("Device capture timed out")??;
    if !out.status.success() {
        bail!("Android capture failed; check adb authorization and device selection");
    }
    Ok(image::load_from_memory(&out.stdout)?)
}

pub async fn capture_command(args: CaptureArgs) -> Result<()> {
    capture(&args.device).await?.save(&args.output)?;
    output::print_success("Screenshot captured");
    Ok(())
}

#[derive(Args)]
pub struct ReviewArgs {
    /// Local reference image; omit when using --source
    pub design: Option<PathBuf>,
    #[arg(long, conflicts_with_all = ["simulator", "android"])]
    pub screenshot: Option<PathBuf>,
    #[arg(long, conflicts_with = "design")]
    pub source: Option<String>,
    #[command(flatten)]
    pub device: DeviceArgs,
    #[arg(long)]
    pub threshold: Option<f32>,
    #[arg(long, default_value_t = 10)]
    pub tolerance: u8,
    #[arg(long)]
    pub scale: Option<f32>,
    /// Crop these screenshot pixels from the top (safe area)
    #[arg(long, default_value_t = 0)]
    pub crop_top: u32,
    #[arg(long, default_value_t = 0)]
    pub crop_bottom: u32,
    /// Explicitly resize screenshot to design dimensions
    #[arg(long)]
    pub normalize: bool,
    /// Ignore rectangle x,y,width,height in final comparison coordinates (repeatable)
    #[arg(long)]
    pub mask: Vec<String>,
    #[arg(short, long, default_value = "fgm-review")]
    pub output: PathBuf,
    /// Exclude source URL/version from the bundle; image contents still require review
    #[arg(long)]
    pub share: bool,
    #[arg(long)]
    pub open: bool,
}

fn rectangle(value: &str, width: u32, height: u32) -> Result<[u32; 4]> {
    let values: Vec<u32> = value
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    let [x, y, w, h]: [u32; 4] = values
        .try_into()
        .map_err(|_| anyhow::anyhow!("Mask must be x,y,width,height"))?;
    if w == 0
        || h == 0
        || x.checked_add(w).is_none_or(|v| v > width)
        || y.checked_add(h).is_none_or(|v| v > height)
    {
        bail!("Mask is outside the image");
    }
    Ok([x, y, w, h])
}

fn prepare(
    design: &DynamicImage,
    screenshot: DynamicImage,
    args: &ReviewArgs,
) -> Result<(DynamicImage, DynamicImage)> {
    let (w, h) = screenshot.dimensions();
    let crop = args
        .crop_top
        .checked_add(args.crop_bottom)
        .context("Crop overflow")?;
    if crop >= h {
        bail!("Crop removes the entire screenshot");
    }
    let mut screenshot = screenshot.crop_imm(0, args.crop_top, w, h - crop);
    if args.normalize {
        screenshot = screenshot.resize_exact(
            design.width(),
            design.height(),
            image::imageops::FilterType::Lanczos3,
        );
    }
    let mut d = design.to_rgba8();
    let mut s = screenshot.to_rgba8();
    for value in &args.mask {
        let [x, y, w, h] = rectangle(value, d.width().min(s.width()), d.height().min(s.height()))?;
        for yy in y..y + h {
            for xx in x..x + w {
                d.put_pixel(xx, yy, Rgba([0, 0, 0, 255]));
                s.put_pixel(xx, yy, Rgba([0, 0, 0, 255]));
            }
        }
    }
    Ok((d.into(), s.into()))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn unmasked_diff(
    design: &DynamicImage,
    screenshot: &DynamicImage,
    masks: &[String],
    tolerance: u8,
) -> Result<(f32, Option<[u32; 4]>)> {
    let width = design.width().min(screenshot.width());
    let height = design.height().min(screenshot.height());
    let rects: Vec<_> = masks
        .iter()
        .map(|s| rectangle(s, width, height))
        .collect::<Result<_>>()?;
    let d = design.to_rgba8();
    let s = screenshot.to_rgba8();
    let mut total = 0u64;
    let mut changed = 0u64;
    let mut bounds: Option<[u32; 4]> = None;
    for y in 0..height {
        for x in 0..width {
            if rects
                .iter()
                .any(|[l, t, w, h]| x >= *l && x < l + w && y >= *t && y < t + h)
            {
                continue;
            }
            total += 1;
            if d.get_pixel(x, y)
                .0
                .iter()
                .zip(s.get_pixel(x, y).0)
                .any(|(a, b)| a.abs_diff(b) > tolerance)
            {
                changed += 1;
                bounds = Some(match bounds {
                    None => [x, y, x, y],
                    Some([l, t, r, b]) => [l.min(x), t.min(y), r.max(x), b.max(y)],
                });
            }
        }
    }
    if total == 0 {
        bail!("No unmasked pixels remain");
    }
    Ok((changed as f32 / total as f32 * 100.0, bounds))
}

pub async fn review(args: ReviewArgs) -> Result<()> {
    let defaults = project()?;
    let threshold = args
        .threshold
        .or_else(|| {
            defaults
                .as_ref()
                .and_then(|(_, p)| p["compare"]["threshold"].as_f64().map(|v| v as f32))
        })
        .unwrap_or(5.0);
    if !(0.0..=100.0).contains(&threshold) {
        bail!("Threshold must be 0-100");
    }
    let mut provenance = json!({"capture":if args.screenshot.is_some() {"file"} else if args.device.simulator.is_some() {"simulator"} else {"android"},"captured_at":chrono::Utc::now().to_rfc3339()});
    let design = if let Some(path) = &args.design {
        image::open(path)?
    } else {
        let (client, parsed, file) = document(args.source.as_deref()).await?;
        let id = parsed
            .node_id
            .context("Review source must select a node; use a saved alias or node-id URL")?;
        let scale = args
            .scale
            .or_else(|| {
                defaults
                    .as_ref()
                    .and_then(|(_, p)| p["export"]["scale"].as_f64().map(|v| v as f32))
            })
            .unwrap_or(2.0);
        if !(1.0..=4.0).contains(&scale) {
            bail!("Scale must be 1-4");
        }
        let images = client
            .export_images(&parsed.file_key, std::slice::from_ref(&id), "png", scale)
            .await?;
        let url = images
            .images
            .get(&id)
            .and_then(|s| s.as_deref())
            .context("Figma did not return an image")?;
        provenance["source"] = json!(link(&parsed.file_key, &id));
        provenance["version"] = file["version"].clone();
        image::load_from_memory(&client.download_image(url).await?)?
    };
    let screenshot = if let Some(path) = &args.screenshot {
        image::open(path)?
    } else {
        capture(&args.device).await?
    };
    let (design, screenshot) = prepare(&design, screenshot, &args)?;
    let result = crate::commands::compare::calculate_diff_internal(
        &design,
        &screenshot,
        args.tolerance,
        Some(threshold),
        false,
    )?;
    let (diff_percent, bounds) = unmasked_diff(&design, &screenshot, &args.mask, args.tolerance)?;
    let passed = result.dimensions_match && diff_percent <= threshold;
    fs::create_dir_all(&args.output)?;
    design.save(args.output.join("design.png"))?;
    screenshot.save(args.output.join("screenshot.png"))?;
    let diff = crate::commands::compare::generate_diff_image(&design, &screenshot, args.tolerance);
    diff.save(args.output.join("diff.png"))?;
    let mut overlay = design.to_rgba8();
    let s = screenshot.to_rgba8();
    for (x, y, p) in overlay.enumerate_pixels_mut() {
        if x >= s.width() || y >= s.height() {
            continue;
        }
        let q = s.get_pixel(x, y);
        for c in 0..4 {
            p[c] = ((p[c] as u16 + q[c] as u16) / 2) as u8;
        }
    }
    overlay.save(args.output.join("overlay.png"))?;
    if args.share {
        provenance.as_object_mut().unwrap().remove("source");
        provenance.as_object_mut().unwrap().remove("version");
    }
    let report = json!({"schema_version":1,"passed":passed,"diff_percent":diff_percent,"threshold":threshold,"dimensions_match":result.dimensions_match,"changed_bounds":bounds,"mask":args.mask,"crop_top":args.crop_top,"crop_bottom":args.crop_bottom,"normalized":args.normalize,"provenance":provenance});
    fs::write(
        args.output.join("report.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    let html = format!("<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>fgm visual review</title><style>body{{font:16px sans-serif;background:#faf7f0;margin:2rem}}section{{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:1rem}}img{{max-width:100%;border:1px solid #ddd}}pre{{white-space:pre-wrap}}</style><h1>Visual review: {}</h1><p>{:.2}% changed</p><section>{}</section><pre>{}</pre>",if passed {"PASS"} else {"FAIL"}, diff_percent,["design","screenshot","overlay","diff"].iter().map(|n|format!("<article><h2>{n}</h2><img src=\"{n}.png\" alt=\"{n}\"></article>")).collect::<String>(),escape(&serde_json::to_string_pretty(&report)?));
    fs::write(args.output.join("index.html"), html)?;
    output::print_json(&report)?;
    if args.open {
        open::that(args.output.join("index.html"))?;
    }
    if !passed {
        bail!("Visual review failed; bundle written");
    }
    Ok(())
}

#[derive(Args)]
pub struct PackArgs {
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long)]
    pub page: Option<String>,
    #[arg(long)]
    pub changed_only: bool,
    #[arg(short, long, default_value = "fgm-pack")]
    pub output: PathBuf,
}

pub async fn pack(args: PackArgs) -> Result<()> {
    let (client, parsed, file) = document(args.source.as_deref()).await?;
    let mut selected = selected(&file, args.query.as_deref(), args.page.as_deref(), false);
    if let Some(id) = &parsed.node_id {
        selected.retain(|(n, _)| n["id"] == *id);
    }
    if selected.is_empty() {
        bail!("Pack scope matched no frames/components");
    }
    fs::create_dir_all(&args.output)?;
    let state_path = args.output.join("manifest.json");
    let old: Value = if args.changed_only && state_path.exists() {
        serde_json::from_str(&fs::read_to_string(&state_path)?)?
    } else {
        Value::Null
    };
    let mut assets = BTreeMap::new();
    let mut thumbnails = vec![];
    for (node, page) in selected {
        let id = node["id"].as_str().context("Missing node ID")?;
        let filename = format!("{}.png", crate::variables::identifier(id));
        let prior = &old["assets"][id];
        let mut unchanged = prior["node"] == *node
            && old["version"] == file["version"]
            && old["file_key"] == parsed.file_key
            && args.output.join(&filename).exists();
        if !args.changed_only || !unchanged {
            let images = client
                .export_images(&parsed.file_key, &[id.to_owned()], "png", 1.0)
                .await?;
            let url = images
                .images
                .get(id)
                .and_then(|s| s.as_deref())
                .context("Missing pack export")?;
            let bytes = client.download_image(url).await?;
            image::load_from_memory(&bytes)?;
            unchanged = old["file_key"] == parsed.file_key
                && prior["node"] == *node
                && fs::read(args.output.join(&filename)).is_ok_and(|existing| existing == bytes);
            if !args.changed_only || !unchanged {
                fs::write(args.output.join(&filename), bytes)?;
            }
        }
        thumbnails.push(
            image::open(args.output.join(&filename))?
                .thumbnail(240, 240)
                .to_rgba8(),
        );
        assets.insert(id.to_owned(),json!({"node":node,"page":page,"image":filename,"url":link(&parsed.file_key,id),"changed":!unchanged,"contact_sheet_index":thumbnails.len()-1}));
    }
    let rows = thumbnails.len().div_ceil(4);
    let mut sheet = RgbaImage::from_pixel(960, (rows as u32) * 260, Rgba([245, 245, 245, 255]));
    for (index, img) in thumbnails.iter().enumerate() {
        image::imageops::overlay(
            &mut sheet,
            img,
            (index % 4 * 240) as i64,
            (index / 4 * 260) as i64,
        );
    }
    sheet.save(args.output.join("contact-sheet.png"))?;
    let manifest = json!({"schema_version":1,"file_key":parsed.file_key,"version":file["version"],"assets":assets,"contact_sheet":"contact-sheet.png"});
    fs::write(state_path, serde_json::to_string_pretty(&manifest)?)?;
    output::print_success(
        "Scoped pack exported with node layout, typography, component properties and contact sheet",
    );
    Ok(())
}

#[derive(Args)]
pub struct CheckMapArgs {
    pub map: PathBuf,
    #[arg(long)]
    pub root: Option<PathBuf>,
}

pub(crate) fn declares_symbol(text: &str, symbol: &str) -> bool {
    let text = code_only(text);
    let words: Vec<_> = text
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .collect();
    words.windows(2).any(|w| {
        [
            "struct",
            "class",
            "enum",
            "interface",
            "object",
            "fun",
            "func",
            "function",
            "const",
            "let",
            "type",
        ]
        .contains(&w[0])
            && w[1] == symbol
    })
}

fn code_only(text: &str) -> String {
    let mut chars = text.chars().peekable();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'/') {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            out.push(' ');
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut depth = 1;
            while let Some(c) = chars.next() {
                if c == '/' && chars.peek() == Some(&'*') {
                    chars.next();
                    depth += 1;
                } else if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            out.push(' ');
        } else if ['"', '\'', '`'].contains(&c) {
            while let Some(next) = chars.next() {
                if next == '\\' {
                    chars.next();
                } else if next == c {
                    break;
                }
            }
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

pub fn check_map(args: CheckMapArgs) -> Result<()> {
    let value: toml::Value = toml::from_str(&fs::read_to_string(&args.map)?)?;
    let root = args.root.unwrap_or_else(|| map_root(&args.map));
    let components = value["components"]
        .as_table()
        .context("Map needs a [components] table")?;
    let mut results = BTreeMap::new();
    let mut failed = 0;
    for (name, entry) in components {
        let path = entry.get("code_path").and_then(toml::Value::as_str);
        let symbol = entry.get("symbol").and_then(toml::Value::as_str);
        let valid = path.is_some_and(|p| {
            fs::read_to_string(root.join(p))
                .is_ok_and(|text| symbol.is_none_or(|s| declares_symbol(&text, s)))
        });
        if !valid {
            failed += 1;
        }
        results.insert(
            name,
            json!({"valid":valid,"linked":path.is_some(),"symbol_checked":symbol.is_some()}),
        );
    }
    output::print_json(
        &json!({"total":components.len(),"valid":components.len()-failed,"missing_or_broken":failed,"components":results}),
    )?;
    if failed > 0 {
        bail!("Component source coverage is incomplete");
    }
    Ok(())
}

pub(crate) fn map_root(map: &Path) -> PathBuf {
    crate::project::find_project_file(map.parent().unwrap_or(Path::new(".")))
        .and_then(|p| p.parent().map(Path::to_owned))
        .unwrap_or_else(|| {
            map.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .to_owned()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn masks_are_bounded() {
        assert_eq!(rectangle("1,2,3,4", 10, 10).unwrap(), [1, 2, 3, 4]);
        for s in ["1,2,3", "0,0,0,2", "4294967295,0,2,1", "9,0,2,1"] {
            assert!(rectangle(s, 10, 10).is_err());
        }
    }
    #[test]
    fn search_preserves_page_and_ambiguity() {
        let file = json!({"document":{"children":[{"type":"CANVAS","name":"Mobile","children":[{"id":"1:2","type":"FRAME","name":"Settings"},{"id":"1:3","type":"COMPONENT","name":"Settings"}]}]}});
        assert_eq!(
            selected(&file, Some("settings"), Some("Mobile"), true).len(),
            2
        );
        assert!(selected(&file, Some("settings"), Some("Web"), false).is_empty());
    }
    #[test]
    fn source_symbols_require_a_declaration() {
        assert!(declares_symbol(
            "struct SettingsView: View {}",
            "SettingsView"
        ));
        assert!(declares_symbol(
            "@Composable fun SettingsView() {}",
            "SettingsView"
        ));
        assert!(declares_symbol(
            "export const SettingsView = () => null",
            "SettingsView"
        ));
        assert!(!declares_symbol("use(SettingsView)", "SettingsView"));
        assert!(!declares_symbol(
            "// struct SettingsView {}\n/* class SettingsView */",
            "SettingsView"
        ));
        assert!(!declares_symbol(
            "let sample = \"struct SettingsView {}\"",
            "SettingsView"
        ));
    }
    #[test]
    fn html_escapes_untrusted_metadata() {
        assert_eq!(escape("<script>\"&"), "&lt;script&gt;&quot;&amp;");
    }

    fn review_args(dir: &Path) -> ReviewArgs {
        ReviewArgs {
            design: Some(dir.join("reference.png")),
            screenshot: Some(dir.join("actual.png")),
            source: None,
            device: Default::default(),
            threshold: Some(5.0),
            tolerance: 10,
            scale: None,
            crop_top: 0,
            crop_bottom: 0,
            normalize: false,
            mask: vec![],
            output: dir.join("bundle"),
            share: true,
            open: false,
        }
    }

    #[test]
    fn masks_do_not_dilute_the_diff_and_overlap_is_not_double_counted() {
        let d: DynamicImage = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 255])).into();
        let s: DynamicImage = RgbaImage::from_pixel(10, 10, Rgba([255, 0, 0, 255])).into();
        assert_eq!(
            unmasked_diff(&d, &s, &["0,0,9,10".into(), "0,0,5,10".into()], 10)
                .unwrap()
                .0,
            100.0
        );
        assert!(unmasked_diff(&d, &s, &["0,0,10,10".into()], 10).is_err());
    }

    #[test]
    fn crop_normalization_and_masks_are_explicit() {
        let d: DynamicImage = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255])).into();
        let s: DynamicImage = RgbaImage::from_pixel(4, 6, Rgba([255, 0, 0, 255])).into();
        let mut args = review_args(Path::new("."));
        args.crop_top = 1;
        args.crop_bottom = 1;
        args.normalize = true;
        args.mask = vec!["0,0,1,1".into()];
        let (d, s) = prepare(&d, s, &args).unwrap();
        assert_eq!(s.dimensions(), (2, 2));
        assert_eq!(d.get_pixel(0, 0), s.get_pixel(0, 0));
        assert_eq!(
            unmasked_diff(&d, &s, &args.mask, 10).unwrap().1,
            Some([0, 0, 1, 1])
        );
        args.crop_top = u32::MAX;
        assert!(prepare(&d, s, &args).is_err());
    }

    #[tokio::test]
    async fn portable_bundle_is_written_for_pass_and_dimension_failure() {
        let dir = tempfile::tempdir().unwrap();
        let img = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255]));
        img.save(dir.path().join("reference.png")).unwrap();
        img.save(dir.path().join("actual.png")).unwrap();
        review(review_args(dir.path())).await.unwrap();
        for f in [
            "design.png",
            "screenshot.png",
            "diff.png",
            "overlay.png",
            "index.html",
            "report.json",
        ] {
            assert!(dir.path().join("bundle").join(f).exists());
        }
        let text = fs::read_to_string(dir.path().join("bundle/report.json")).unwrap();
        assert!(!text.contains(dir.path().to_str().unwrap()));
        assert!(serde_json::from_str::<Value>(&text).unwrap()["passed"]
            .as_bool()
            .unwrap());
        RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]))
            .save(dir.path().join("actual.png"))
            .unwrap();
        assert!(review(review_args(dir.path())).await.is_err());
        let report: Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("bundle/report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["passed"], false);
        assert_eq!(report["dimensions_match"], false);
    }

    #[test]
    fn project_defaults_preserve_explicit_overrides_and_resolve_aliases() {
        use clap::{CommandFactory, FromArgMatches};
        let config = json!({"figma":{"source":"abc123","aliases":{"settings":"abc123:1:2"}},"export":{"output_dir":"assets","scale":3},"compare":{"threshold":2}});
        let matches = crate::cli::Cli::command()
            .try_get_matches_from(["fgm", "export", "file", "settings"])
            .unwrap();
        let mut cli = crate::cli::Cli::from_arg_matches(&matches).unwrap();
        apply_defaults_from(
            &mut cli.command,
            &matches,
            Path::new("/tmp/example/fgm.toml"),
            &config,
            false,
        )
        .unwrap();
        match cli.command {
            Commands::Export {
                command:
                    Some(ExportCommands::File {
                        file_key_or_url,
                        output,
                        scale,
                        ..
                    }),
            } => {
                assert_eq!(file_key_or_url, "abc123:1:2");
                assert_eq!(output.unwrap(), Path::new("/tmp/example/assets"));
                assert_eq!(scale, Some(3.0));
            }
            _ => panic!("export"),
        }
        for (extra, expected) in [(vec![], 2.0), (vec!["--threshold", "5"], 5.0)] {
            let mut argv = vec!["fgm", "compare", "a.png", "b.png"];
            argv.extend(extra);
            let matches = crate::cli::Cli::command()
                .try_get_matches_from(argv)
                .unwrap();
            let mut cli = crate::cli::Cli::from_arg_matches(&matches).unwrap();
            apply_defaults_from(
                &mut cli.command,
                &matches,
                Path::new("/tmp/example/fgm.toml"),
                &config,
                false,
            )
            .unwrap();
            match cli.command {
                Commands::Compare(a) => assert_eq!(a.threshold, expected),
                _ => panic!("compare"),
            }
        }
    }

    #[tokio::test]
    async fn capture_requires_device_selection_without_spawning_tools() {
        assert!(capture(&DeviceArgs::default()).await.is_err());
    }

    #[test]
    fn component_map_checks_paths_and_symbols_relative_to_map() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("components.toml");
        fs::write(dir.path().join("View.swift"), "struct ExampleView {}\n").unwrap();
        fs::write(
            &path,
            "[components.example]\ncode_path = 'View.swift'\nsymbol = 'ExampleView'\n",
        )
        .unwrap();
        assert!(check_map(CheckMapArgs {
            map: path.clone(),
            root: None
        })
        .is_ok());
        fs::write(dir.path().join("View.swift"), "// struct ExampleView {}\n").unwrap();
        assert!(check_map(CheckMapArgs {
            map: path,
            root: None
        })
        .is_err());
    }
}
