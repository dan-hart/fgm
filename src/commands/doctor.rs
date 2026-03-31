use crate::api::create_shared_cache;
use crate::auth::get_token_with_source;
use crate::cli::DoctorArgs;
use crate::config::Config;
use crate::output;
use crate::project::find_project_file;
use crate::reporting::{write_report, ReportItem, ReportSummary};
use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::Path;

pub async fn run(args: DoctorArgs) -> Result<()> {
    let summary = build_summary(args.output_dir.as_deref()).await;

    if let Some(report_path) = args.report.as_deref() {
        write_report(report_path, args.report_format, &summary)?;
        output::print_status(&format!("Report: {}", report_path.display()));
    }

    if output::format() == crate::output::OutputFormat::Json {
        output::print_json(&summary)?;
    } else {
        print_summary(&summary);
    }

    if summary.exit_code() != 0 {
        anyhow::bail!("Doctor found one or more required issues");
    }

    Ok(())
}

async fn build_summary(output_dir: Option<&Path>) -> ReportSummary {
    let mut items = Vec::new();

    match Config::config_path() {
        Some(path) => items.push(ReportItem::ok(
            "config-path",
            format!("Config path: {}", path.display()),
        )),
        None => items.push(ReportItem::fail(
            "config-path",
            "Could not determine config path",
        )),
    }

    match Config::load() {
        Ok(_) => items.push(ReportItem::ok("config-load", "User config is readable")),
        Err(err) => items.push(ReportItem::fail(
            "config-load",
            format!("Failed to load config: {}", err),
        )),
    }

    match get_token_with_source() {
        Ok(token) => {
            items.push(ReportItem::ok(
                "auth-source",
                format!("Using {}", token.source),
            ));
            match crate::api::FigmaClient::new(token.token) {
                Ok(client) => match client.validate_token().await {
                    Ok(true) => items.push(ReportItem::ok(
                        "auth-validate",
                        "Token validated with Figma API",
                    )),
                    Ok(false) => items.push(ReportItem::fail(
                        "auth-validate",
                        "Token is not valid for the Figma API",
                    )),
                    Err(err) => items.push(ReportItem::warn(
                        "auth-validate",
                        format!("Could not validate token: {}", err),
                    )),
                },
                Err(err) => items.push(ReportItem::fail(
                    "auth-client",
                    format!("Could not create API client: {}", err),
                )),
            }
        }
        Err(err) => items.push(ReportItem::fail("auth-source", err.to_string())),
    }

    let cache = create_shared_cache();
    let stats = cache.stats();
    match stats.disk_path {
        Some(path) => {
            if path.exists() {
                items.push(ReportItem::ok(
                    "cache-path",
                    format!("Cache path: {}", path.display()),
                ));
            } else {
                items.push(ReportItem::warn(
                    "cache-path",
                    format!("Cache path does not exist yet: {}", path.display()),
                ));
            }
        }
        None => items.push(ReportItem::warn(
            "cache-path",
            "Disk cache path is not available",
        )),
    }

    let cwd = std::env::current_dir().ok();
    match cwd.as_deref().and_then(find_project_file) {
        Some(path) => items.push(ReportItem::ok(
            "project-file",
            format!("Found project file: {}", path.display()),
        )),
        None => items.push(ReportItem::warn(
            "project-file",
            "No fgm.toml found in the current workspace",
        )),
    }

    if let Some(dir) = output_dir {
        items.push(check_output_dir(dir));
    }

    ReportSummary {
        title: "fgm doctor".to_string(),
        items,
    }
}

fn check_output_dir(path: &Path) -> ReportItem {
    if path.exists() {
        if path.is_dir() {
            if is_writable_dir(path) {
                ReportItem::ok(
                    "output-dir",
                    format!("Writable output dir: {}", path.display()),
                )
            } else {
                ReportItem::fail(
                    "output-dir",
                    format!("Output dir is not writable: {}", path.display()),
                )
            }
        } else {
            ReportItem::fail(
                "output-dir",
                format!("Output path is not a directory: {}", path.display()),
            )
        }
    } else if let Some(parent) = path.parent() {
        if parent.exists() && is_writable_dir(parent) {
            ReportItem::warn(
                "output-dir",
                format!(
                    "Output dir does not exist yet but parent is writable: {}",
                    path.display()
                ),
            )
        } else {
            ReportItem::fail(
                "output-dir",
                format!("Output dir parent is not writable: {}", path.display()),
            )
        }
    } else {
        ReportItem::fail(
            "output-dir",
            format!("Could not validate output dir: {}", path.display()),
        )
    }
}

fn is_writable_dir(path: &Path) -> bool {
    let probe = path.join(".fgm-write-test");
    match fs::write(&probe, b"ok") {
        Ok(_) => {
            let _ = fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn print_summary(summary: &ReportSummary) {
    output::print_status(&summary.title.bold().to_string());
    output::print_status("");
    for item in &summary.items {
        let status = match item.status {
            crate::reporting::ReportStatus::Ok => "ok".green(),
            crate::reporting::ReportStatus::Warn => "warn".yellow(),
            crate::reporting::ReportStatus::Fail => "fail".red(),
        };
        output::print_status(&format!("  {:<6} {}: {}", status, item.name, item.message));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn output_dir_check_warns_when_parent_is_writable() {
        let base = tempdir().expect("tempdir");
        let report = check_output_dir(&base.path().join("out"));
        assert!(matches!(
            report.status,
            crate::reporting::ReportStatus::Warn
        ));
    }
}
