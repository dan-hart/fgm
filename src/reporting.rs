use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ReportFormat {
    Json,
    Md,
    Junit,
    Html,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportItem {
    pub name: String,
    pub status: ReportStatus,
    pub message: String,
}

impl ReportItem {
    pub fn ok(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(name, ReportStatus::Ok, message)
    }

    pub fn warn(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(name, ReportStatus::Warn, message)
    }

    pub fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(name, ReportStatus::Fail, message)
    }

    pub fn new(name: impl Into<String>, status: ReportStatus, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportSummary {
    pub title: String,
    pub items: Vec<ReportItem>,
}

impl ReportSummary {
    pub fn exit_code(&self) -> i32 {
        if self
            .items
            .iter()
            .any(|item| matches!(item.status, ReportStatus::Fail))
        {
            1
        } else {
            0
        }
    }
}

pub fn render_json(summary: &ReportSummary) -> Result<String> {
    Ok(serde_json::to_string_pretty(summary)?)
}

pub fn render_markdown(summary: &ReportSummary) -> String {
    let mut markdown = format!("# {}\n\n", summary.title);
    markdown.push_str("| Check | Status | Message |\n");
    markdown.push_str("| --- | --- | --- |\n");
    for item in &summary.items {
        markdown.push_str(&format!(
            "| {} | {} | {} |\n",
            item.name,
            status_label(item.status),
            item.message.replace('\n', "<br/>")
        ));
    }
    markdown
}

pub fn render_junit(summary: &ReportSummary) -> String {
    let failures = summary
        .items
        .iter()
        .filter(|item| matches!(item.status, ReportStatus::Fail))
        .count();
    let mut xml = format!(
        "<testsuite name=\"{}\" tests=\"{}\" failures=\"{}\">\n",
        escape_xml(&summary.title),
        summary.items.len(),
        failures
    );
    for item in &summary.items {
        xml.push_str(&format!("  <testcase name=\"{}\">", escape_xml(&item.name)));
        match item.status {
            ReportStatus::Fail => {
                xml.push_str(&format!(
                    "<failure message=\"{}\" />",
                    escape_xml(&item.message)
                ));
            }
            ReportStatus::Warn => {
                xml.push_str(&format!(
                    "<system-out>{}</system-out>",
                    escape_xml(&item.message)
                ));
            }
            ReportStatus::Ok => {}
        }
        xml.push_str("</testcase>\n");
    }
    xml.push_str("</testsuite>\n");
    xml
}

pub fn render_html(summary: &ReportSummary) -> String {
    let rows = summary
        .items
        .iter()
        .map(|item| {
            format!(
                "<tr><td>{}</td><td class=\"{}\">{}</td><td>{}</td></tr>",
                escape_html(&item.name),
                status_class(item.status),
                status_label(item.status),
                escape_html(&item.message)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title><style>body{{font-family:ui-sans-serif,system-ui,sans-serif;margin:2rem;}}table{{border-collapse:collapse;width:100%;}}th,td{{border:1px solid #ddd;padding:0.75rem;text-align:left;}}.ok{{color:#166534;}}.warn{{color:#a16207;}}.fail{{color:#b91c1c;}}</style></head><body><h1>{title}</h1><table><thead><tr><th>Check</th><th>Status</th><th>Message</th></tr></thead><tbody>{rows}</tbody></table></body></html>",
        title = escape_html(&summary.title),
        rows = rows
    )
}

pub fn render_report(summary: &ReportSummary, format: ReportFormat) -> Result<String> {
    match format {
        ReportFormat::Json => render_json(summary),
        ReportFormat::Md => Ok(render_markdown(summary)),
        ReportFormat::Junit => Ok(render_junit(summary)),
        ReportFormat::Html => Ok(render_html(summary)),
    }
}

pub fn write_report(path: &Path, format: ReportFormat, summary: &ReportSummary) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let content = render_report(summary, format)?;
    fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn status_label(status: ReportStatus) -> &'static str {
    match status {
        ReportStatus::Ok => "ok",
        ReportStatus::Warn => "warn",
        ReportStatus::Fail => "fail",
    }
}

fn status_class(status: ReportStatus) -> &'static str {
    match status {
        ReportStatus::Ok => "ok",
        ReportStatus::Warn => "warn",
        ReportStatus::Fail => "fail",
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn escape_html(value: &str) -> String {
    escape_xml(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_exit_code_is_zero_when_all_checks_pass() {
        let summary = ReportSummary {
            title: "doctor".to_string(),
            items: vec![ReportItem::ok("auth", "Token is available")],
        };

        assert_eq!(summary.exit_code(), 0);
    }

    #[test]
    fn summary_exit_code_is_one_when_required_checks_fail() {
        let summary = ReportSummary {
            title: "doctor".to_string(),
            items: vec![ReportItem::fail("auth", "Token is missing")],
        };

        assert_eq!(summary.exit_code(), 1);
    }

    #[test]
    fn markdown_report_contains_title_and_items() {
        let summary = ReportSummary {
            title: "doctor".to_string(),
            items: vec![
                ReportItem::ok("auth", "Token available"),
                ReportItem::warn("project", "fgm.toml not found"),
            ],
        };

        let markdown = render_markdown(&summary);

        assert!(markdown.contains("# doctor"));
        assert!(markdown.contains("auth"));
        assert!(markdown.contains("project"));
        assert!(markdown.contains("Token available"));
    }
}
