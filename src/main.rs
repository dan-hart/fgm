mod api;
mod auth;
mod cli;
mod commands;
mod config;
mod output;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use output::{OutputFormat, Verbosity};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load config for defaults
    let mut config_error: Option<String> = None;
    let config = match config::Config::load() {
        Ok(cfg) => cfg,
        Err(err) => {
            config_error = Some(err.to_string());
            config::Config::default()
        }
    };

    let mut format_warning: Option<String> = None;
    // Determine output format (CLI override > config)
    let format = if cli.json {
        OutputFormat::Json
    } else if let Some(fmt) = cli.format {
        fmt
    } else {
        match config.defaults.output_format.to_lowercase().as_str() {
            "json" => OutputFormat::Json,
            "table" => OutputFormat::Table,
            other => {
                format_warning = Some(format!(
                    "Invalid output_format '{}' in config, using table",
                    other
                ));
                OutputFormat::Table
            }
        }
    };

    let verbosity = if cli.quiet {
        Verbosity::Quiet
    } else if cli.verbose {
        Verbosity::Verbose
    } else {
        Verbosity::Normal
    };

    output::init(format, verbosity, !cli.no_color);
    auth::set_keychain_enabled(!cli.no_keychain);

    if let Some(err) = config_error {
        output::print_warning(&format!(
            "Failed to load config ({}), using defaults",
            err
        ));
    }
    if let Some(warn) = format_warning {
        output::print_warning(&warn);
    }

    let result = match cli.command {
        Commands::Auth { command } => commands::auth::run(command).await,
        Commands::Files { command } => commands::files::run(command).await,
        Commands::Export { command } => commands::export::run(command).await,
        Commands::Compare(args) => commands::compare::run(args).await,
        Commands::CompareUrl(args) => commands::compare_url::run(args).await,
        Commands::Tokens { command } => commands::tokens::run(command).await,
        Commands::Components { command } => commands::components::run(command).await,
        Commands::Preview(args) => commands::preview::run(args).await,
        Commands::Snapshot { command } => commands::snapshot::run(command).await,
        Commands::Sync(args) => commands::sync::run(args).await,
        Commands::Map { command } => commands::map::run(command).await,
        Commands::Cache { command } => commands::cache::run(command).await,
        Commands::Config { command } => commands::config::run(command).await,
    };

    if let Err(err) = result {
        output::print_error(&err.to_string());
        std::process::exit(1);
    }

    Ok(())
}
