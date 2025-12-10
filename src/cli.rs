use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "fgm")]
#[command(author, version, about = "Figma CLI - Export assets, compare designs, extract tokens")]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage authentication
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },

    /// Browse and inspect Figma files
    Files {
        #[command(subcommand)]
        command: FilesCommands,
    },

    /// Export assets from Figma
    Export {
        #[command(subcommand)]
        command: ExportCommands,
    },

    /// Compare design images with screenshots
    Compare(CompareArgs),

    /// Extract design tokens
    Tokens {
        #[command(subcommand)]
        command: TokensCommands,
    },

    /// Browse published components
    Components {
        #[command(subcommand)]
        command: ComponentsCommands,
    },

    /// Preview a Figma frame in terminal (optional)
    Preview(PreviewArgs),
}

// Auth subcommands
#[derive(Subcommand)]
pub enum AuthCommands {
    /// Log in with a personal access token
    Login,
    /// Remove stored authentication
    Logout,
    /// Check authentication status
    Status,
}

// Files subcommands
#[derive(Subcommand)]
pub enum FilesCommands {
    /// List files in a project
    List {
        /// Project ID to list files from
        #[arg(short, long)]
        project: Option<String>,
        /// Team ID to list projects from
        #[arg(short, long)]
        team: Option<String>,
    },
    /// Get file metadata
    Get {
        /// Figma file key or URL (e.g., abc123 or https://figma.com/file/abc123/...)
        file_key_or_url: String,
    },
    /// Show node tree structure
    Tree {
        /// Figma file key or URL
        file_key_or_url: String,
        /// Maximum depth to display
        #[arg(short, long, default_value = "3")]
        depth: u32,
    },
    /// Show version history
    Versions {
        /// Figma file key or URL
        file_key_or_url: String,
        /// Number of versions to show
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
}

// Export subcommands
#[derive(Subcommand)]
pub enum ExportCommands {
    /// Export a single file or specific nodes
    File {
        /// Figma file key or URL (e.g., abc123 or https://figma.com/file/abc123/...?node-id=1-2)
        file_key_or_url: String,
        /// Node IDs to export (can be specified multiple times). If URL contains node-id, it will be used.
        #[arg(short, long)]
        node: Vec<String>,
        /// Export all top-level frames
        #[arg(long)]
        all_frames: bool,
        /// Export format
        #[arg(short, long, default_value = "png")]
        format: ExportFormat,
        /// Scale factor (1-4)
        #[arg(short, long, default_value = "2")]
        scale: u8,
        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
        /// Custom filename (without extension). Only works with single node export.
        #[arg(long)]
        name: Option<String>,
    },
    /// Batch export from a manifest file
    Batch {
        /// Path to manifest file (TOML)
        manifest: PathBuf,
    },
}

#[derive(Clone, clap::ValueEnum)]
pub enum ExportFormat {
    Png,
    Svg,
    Pdf,
    Jpg,
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportFormat::Png => write!(f, "png"),
            ExportFormat::Svg => write!(f, "svg"),
            ExportFormat::Pdf => write!(f, "pdf"),
            ExportFormat::Jpg => write!(f, "jpg"),
        }
    }
}

// Compare arguments
#[derive(clap::Args)]
pub struct CompareArgs {
    /// First image (typically the Figma export)
    pub image1: PathBuf,
    /// Second image (typically the dev screenshot)
    pub image2: PathBuf,
    /// Output path for diff image
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Acceptable difference threshold (percentage)
    #[arg(short, long, default_value = "5.0")]
    pub threshold: f32,
    /// Compare all images in directories
    #[arg(long)]
    pub batch: bool,
    /// Output report file (JSON)
    #[arg(short, long)]
    pub report: Option<PathBuf>,
}

// Tokens subcommands
#[derive(Subcommand)]
pub enum TokensCommands {
    /// Extract color styles
    Colors {
        /// Figma file key
        file_key: String,
    },
    /// Extract typography styles
    Typography {
        /// Figma file key
        file_key: String,
    },
    /// Extract spacing values from auto-layout
    Spacing {
        /// Figma file key
        file_key: String,
    },
    /// Export all design tokens
    Export {
        /// Figma file key
        file_key: String,
        /// Output format
        #[arg(short, long, default_value = "json")]
        format: TokenFormat,
        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, clap::ValueEnum)]
pub enum TokenFormat {
    Json,
    Css,
    Swift,
    Kotlin,
}

// Components subcommands
#[derive(Subcommand)]
pub enum ComponentsCommands {
    /// List published components in a team
    List {
        /// Team ID
        team_id: String,
    },
    /// Get component details
    Get {
        /// Component key
        component_key: String,
    },
}

// Preview arguments
#[derive(clap::Args)]
pub struct PreviewArgs {
    /// Figma file key
    pub file_key: String,
    /// Node ID to preview
    #[arg(short, long)]
    pub node: Option<String>,
    /// Width in terminal columns
    #[arg(short, long)]
    pub width: Option<u32>,
    /// Terminal image protocol
    #[arg(short, long, default_value = "auto")]
    pub protocol: ImageProtocol,
}

#[derive(Clone, clap::ValueEnum)]
pub enum ImageProtocol {
    Auto,
    Sixel,
    Iterm,
    Kitty,
}
