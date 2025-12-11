mod api;
mod auth;
mod cli;
mod commands;
mod config;
mod output;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
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
    }
}
