use crate::cli::{CompareUrlArgs, ExportCommands, RunArgs, SnapshotCommands, SyncArgs};
use crate::commands;
use crate::output;
use crate::reporting::{write_report, ReportItem, ReportStatus, ReportSummary};
use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

pub async fn run(args: RunArgs) -> Result<()> {
    let content = std::fs::read_to_string(&args.manifest)?;
    let manifest: RunManifest = toml::from_str(&content)?;

    let mut items = Vec::new();
    for job in manifest.jobs {
        let name = job.name();
        output::print_status(&format!("Running job: {}", name));
        let result = match job {
            RunJob::ExportBatch { manifest, .. } => {
                commands::export::run(ExportCommands::Batch { manifest }).await
            }
            RunJob::Sync {
                manifest, force, ..
            } => {
                commands::sync::run(SyncArgs {
                    manifest,
                    dry_run: false,
                    force,
                    report: None,
                    report_format: crate::reporting::ReportFormat::Json,
                })
                .await
            }
            RunJob::CompareUrl {
                figma_url,
                screenshot,
                threshold,
                scale,
                tolerance,
                fast,
                ..
            } => {
                commands::compare_url::run(CompareUrlArgs {
                    figma_url,
                    screenshot,
                    output: None,
                    threshold: threshold.unwrap_or(5.0),
                    scale,
                    tolerance: tolerance.unwrap_or(10),
                    fast,
                    report: None,
                    report_format: crate::reporting::ReportFormat::Json,
                    watch: false,
                    watch_interval: 5,
                })
                .await
            }
            RunJob::SnapshotCreate {
                file_key_or_url,
                name,
                node,
                output,
            } => {
                commands::snapshot::run(SnapshotCommands::Create {
                    file_key_or_url,
                    name,
                    node,
                    pick: false,
                    output,
                    watch: false,
                    watch_interval: 5,
                })
                .await
            }
        };

        items.push(ReportItem::new(
            name,
            if result.is_ok() {
                ReportStatus::Ok
            } else {
                ReportStatus::Fail
            },
            result
                .as_ref()
                .map(|_| "Completed".to_string())
                .unwrap_or_else(|err| err.to_string()),
        ));
    }

    let summary = ReportSummary {
        title: "fgm run".to_string(),
        items,
    };

    if let Some(report_path) = args.report.as_deref() {
        write_report(report_path, args.report_format, &summary)?;
        output::print_status(&format!("Report: {}", report_path.display()));
    }

    if output::format() == crate::output::OutputFormat::Json {
        output::print_json(&summary)?;
    }

    if summary.exit_code() != 0 {
        anyhow::bail!("One or more run jobs failed");
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct RunManifest {
    #[serde(default)]
    jobs: Vec<RunJob>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RunJob {
    ExportBatch {
        name: Option<String>,
        manifest: PathBuf,
    },
    Sync {
        name: Option<String>,
        manifest: PathBuf,
        #[serde(default)]
        force: bool,
    },
    CompareUrl {
        name: Option<String>,
        figma_url: String,
        screenshot: PathBuf,
        threshold: Option<f32>,
        scale: Option<f32>,
        tolerance: Option<u8>,
        #[serde(default)]
        fast: bool,
    },
    SnapshotCreate {
        name: String,
        file_key_or_url: String,
        #[serde(default)]
        node: Vec<String>,
        #[serde(default = "default_snapshot_dir")]
        output: PathBuf,
    },
}

impl RunJob {
    fn name(&self) -> String {
        match self {
            RunJob::ExportBatch { name, manifest } => name
                .clone()
                .unwrap_or_else(|| format!("export-batch {}", manifest.display())),
            RunJob::Sync { name, manifest, .. } => name
                .clone()
                .unwrap_or_else(|| format!("sync {}", manifest.display())),
            RunJob::CompareUrl {
                name, figma_url, ..
            } => name
                .clone()
                .unwrap_or_else(|| format!("compare-url {}", figma_url)),
            RunJob::SnapshotCreate { name, .. } => name.clone(),
        }
    }
}

fn default_snapshot_dir() -> PathBuf {
    PathBuf::from(".fgm-snapshots")
}
