use colored::Colorize;
use serde::Serialize;
use tabled::{Table, Tabled};

/// Output format for CLI results
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Table,
    Json,
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Table
    }
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
    println!("{}: {}", "success".green().bold(), message);
}

/// Print an info message
pub fn print_info(message: &str) {
    println!("{}: {}", "info".blue().bold(), message);
}
