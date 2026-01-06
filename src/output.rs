use colored::Colorize;
use serde::Serialize;
use std::sync::OnceLock;
use tabled::{Table, Tabled};

/// Output format for CLI results
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Table
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

impl Default for Verbosity {
    fn default() -> Self {
        Verbosity::Normal
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OutputSettings {
    pub format: OutputFormat,
    pub verbosity: Verbosity,
}

impl Default for OutputSettings {
    fn default() -> Self {
        OutputSettings {
            format: OutputFormat::Table,
            verbosity: Verbosity::Normal,
        }
    }
}

static SETTINGS: OnceLock<OutputSettings> = OnceLock::new();

/// Initialize global output settings
pub fn init(format: OutputFormat, verbosity: Verbosity, color: bool) {
    let _ = SETTINGS.set(OutputSettings {
        format,
        verbosity,
    });
    if !color {
        colored::control::set_override(false);
    }
}

fn settings() -> OutputSettings {
    SETTINGS.get().copied().unwrap_or_default()
}

pub fn format() -> OutputFormat {
    settings().format
}

pub fn verbosity() -> Verbosity {
    settings().verbosity
}

pub fn is_quiet() -> bool {
    matches!(settings().verbosity, Verbosity::Quiet)
}

pub fn is_verbose() -> bool {
    matches!(settings().verbosity, Verbosity::Verbose)
}

fn should_show_status() -> bool {
    if is_quiet() {
        return false;
    }
    if settings().format == OutputFormat::Json && !is_verbose() {
        return false;
    }
    true
}

/// Print data as a table
pub fn print_table<T: Tabled>(items: &[T]) {
    if items.is_empty() {
        println!("{}", "No results".dimmed());
        return;
    }
    let table = Table::new(items);
    println!("{}", table);
}

/// Print data as JSON
pub fn print_json<T: Serialize>(data: &T) -> Result<(), serde_json::Error> {
    let json = serde_json::to_string_pretty(data)?;
    println!("{}", json);
    Ok(())
}

/// Print a status/info line (suppressed in quiet mode or JSON mode unless verbose)
pub fn print_status(message: &str) {
    if should_show_status() {
        println!("{}", message);
    }
}

/// Print an error message
pub fn print_error(message: &str) {
    eprintln!("{}: {}", "error".red().bold(), message);
}

/// Print a warning message
pub fn print_warning(message: &str) {
    eprintln!("{}: {}", "warning".yellow().bold(), message);
}

/// Print a success message
pub fn print_success(message: &str) {
    if should_show_status() {
        println!("{}: {}", "success".green().bold(), message);
    }
}

/// Print an info message
pub fn print_info(message: &str) {
    if should_show_status() {
        println!("{}: {}", "info".blue().bold(), message);
    }
}

/// Print a verbose-only message
pub fn print_verbose(message: &str) {
    if is_verbose() {
        println!("{}", message);
    }
}

/// Print primary output without suppression (e.g., config or raw output)
pub fn print_raw(message: &str) {
    println!("{}", message);
}
