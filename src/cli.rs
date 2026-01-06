use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::output::OutputFormat;

#[derive(Parser)]
#[command(name = "fgm")]
#[command(author, version)]
#[command(about = "Figma CLI - Export assets, compare designs, extract tokens")]
#[command(long_about = "A fast, cross-platform command-line interface for Figma.

Export assets, compare designs against screenshots, extract design tokens,
and preview screens directly in your terminal.

Requires a Figma Personal Access Token (PAT). Get one at:
https://www.figma.com/developers/api#access-tokens

Set via FIGMA_TOKEN environment variable or run 'fgm auth login' to store
in your config file by default. Use --keychain to store in the system keychain.")]
#[command(after_help = "GETTING STARTED:
    fgm auth login                              Store your Figma token
    fgm files get <URL>                         Get file info from URL
    fgm export file <URL> -o ./out/             Export assets to directory

COMMON WORKFLOWS:
    fgm export file <URL> --platform ios        Export for iOS (@1x, @2x, @3x)
    fgm compare-url <URL> screenshot.png        Compare Figma to screenshot
    fgm tokens export <key> --format css        Export design tokens to CSS

Learn more: https://github.com/dan-hart/fgm")]
#[command(propagate_version = true)]
pub struct Cli {
    /// Output format (table or json)
    #[arg(long, global = true, value_enum, help = "Output format")]
    pub format: Option<OutputFormat>,
    /// Output JSON (alias for --format json)
    #[arg(long, global = true, conflicts_with = "format", help = "Output JSON")]
    pub json: bool,
    /// Suppress non-error output
    #[arg(short, long, global = true, conflicts_with = "verbose", help = "Quiet mode")]
    pub quiet: bool,
    /// Enable verbose output
    #[arg(short, long, global = true, conflicts_with = "quiet", help = "Verbose mode")]
    pub verbose: bool,
    /// Disable colored output
    #[arg(long, global = true, help = "Disable colored output")]
    pub no_color: bool,
    /// Disable all keychain access (avoid macOS prompts)
    #[arg(long, global = true, help = "Disable all keychain access")]
    pub no_keychain: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage authentication (login, logout, status)
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },

    /// Browse and inspect Figma files
    Files {
        #[command(subcommand)]
        command: FilesCommands,
    },

    /// Export assets from Figma (PNG, SVG, PDF, JPG)
    Export {
        #[command(subcommand)]
        command: ExportCommands,
    },

    /// Compare local design images with screenshots
    #[command(long_about = "Compare two local image files pixel-by-pixel.

Calculates the percentage of pixels that differ between images.
Useful for visual regression testing and design-to-code verification.

Exit code is 1 if difference exceeds threshold (for CI integration).")]
    #[command(after_help = "EXAMPLES:
    fgm compare design.png screenshot.png
    fgm compare design.png screenshot.png --threshold 3
    fgm compare design.png screenshot.png --output diff.png
    fgm compare ./designs/ ./screenshots/ --batch --report report.json")]
    Compare(CompareArgs),

    /// Export from Figma URL and compare against a screenshot in one step
    #[command(name = "compare-url")]
    #[command(long_about = "Export a Figma frame and compare it against a local screenshot.

This is a convenience command that combines 'export' and 'compare' into one step.
The Figma URL must include a node-id parameter to specify which frame to export.

Exit code is 1 if difference exceeds threshold (for CI integration).")]
    #[command(after_help = "EXAMPLES:
    fgm compare-url \"https://figma.com/design/abc?node-id=1-2\" screenshot.png
    fgm compare-url \"https://figma.com/design/abc?node-id=1-2\" dev.png --threshold 3
    fgm compare-url \"https://figma.com/design/abc?node-id=1-2\" dev.png -o diff.png -s 3")]
    CompareUrl(CompareUrlArgs),

    /// Extract design tokens (colors, typography, spacing)
    Tokens {
        #[command(subcommand)]
        command: TokensCommands,
    },

    /// Browse published library components
    Components {
        #[command(subcommand)]
        command: ComponentsCommands,
    },

    /// Preview a Figma frame directly in terminal
    #[command(long_about = "Display a Figma frame as an image directly in your terminal.

Supports iTerm2, Kitty, WezTerm, Ghostty, and terminals with Sixel support.
Protocol is auto-detected, or can be forced with --protocol.")]
    #[command(after_help = "EXAMPLES:
    fgm preview abc123
    fgm preview abc123 --node \"1:2\"
    fgm preview abc123 --protocol kitty --width 80")]
    Preview(PreviewArgs),

    /// Snapshot and diff design versions over time
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommands,
    },

    /// Sync assets declaratively from a TOML manifest
    #[command(long_about = "Download and sync assets defined in a TOML manifest file.

The manifest defines which Figma frames to export and where to save them.
Useful for keeping local assets in sync with Figma designs.")]
    #[command(after_help = "EXAMPLES:
    fgm sync figma-assets.toml
    fgm sync figma-assets.toml --dry-run
    fgm sync figma-assets.toml --force

MANIFEST FORMAT:
    [project]
    name = \"MyApp\"
    output_dir = \"./assets\"

    [assets.icon]
    figma = \"https://figma.com/design/abc?node-id=1-2\"
    output = \"icon.png\"
    scale = 2")]
    Sync(SyncArgs),

    /// Track Figma component implementation in code
    Map {
        #[command(subcommand)]
        command: MapCommands,
    },

    /// Manage API response cache (warmup, status, clear)
    Cache {
        #[command(subcommand)]
        command: CacheCommands,
    },

    /// Manage CLI configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

// Auth subcommands
#[derive(Subcommand)]
pub enum AuthCommands {
    /// Store your Figma Personal Access Token securely
    #[command(long_about = "Store your Figma Personal Access Token in the config file by default.

Opens your browser to the Figma token creation page, then prompts you to
paste your token. By default, the token is stored in:
  - ~/.config/fgm/config.toml (plaintext)

Use --keychain to store it securely in:
  - macOS: Keychain
  - Linux: Secret Service (GNOME Keyring, KWallet)

Token priority: FIGMA_TOKEN env var > Config file > Keychain")]
    Login {
        /// Store token in keychain (secure)
        #[arg(long, help = "Store token in keychain (secure)")]
        keychain: bool,
    },

    /// Remove stored authentication token
    #[command(long_about = "Remove your Figma token from the system keychain.

This does not revoke the token on Figma's side. To fully revoke access,
visit: https://www.figma.com/settings → Personal access tokens")]
    Logout,

    /// Check current authentication status
    #[command(long_about = "Verify your Figma authentication is working.

Checks for a valid token and tests it against the Figma API.
Shows which token source is being used (env var, keychain, or config).")]
    Status,

    /// Debug authentication issues (keychain, env vars, config)
    #[command(long_about = "Diagnose authentication problems.

Tests each token source individually and reports detailed information:
  - Environment variable (FIGMA_TOKEN)
  - System keychain status and accessibility
  - Config file token storage

Use this when 'fgm auth login' succeeds but 'fgm auth status' fails.")]
    Debug,
}

// Files subcommands
#[derive(Subcommand)]
pub enum FilesCommands {
    /// List files in a project or projects in a team
    #[command(long_about = "List files or projects from Figma.

Use --team to list all projects in a team.
Use --project to list all files in a specific project.")]
    #[command(group(
        clap::ArgGroup::new("scope")
            .required(true)
            .args(&["project", "team"])
            .multiple(false)
    ))]
    #[command(after_help = "EXAMPLES:
    fgm files list --team 123456789
    fgm files list --project 987654321")]
    List {
        /// Project ID to list files from
        #[arg(short, long, help = "List files in this project")]
        project: Option<String>,
        /// Team ID to list projects from
        #[arg(short, long, help = "List projects in this team")]
        team: Option<String>,
    },

    /// Get file metadata and info
    #[command(long_about = "Retrieve metadata for a Figma file.

Shows file name, last modified date, version, and summary of contents.
Accepts either a file key (e.g., abc123) or a full Figma URL.")]
    #[command(after_help = "EXAMPLES:
    fgm files get abc123
    fgm files get \"https://www.figma.com/design/abc123/MyFile\"")]
    Get {
        /// Figma file key or URL
        #[arg(help = "File key (abc123) or full Figma URL")]
        file_key_or_url: String,
    },

    /// Show document structure as a tree
    #[command(long_about = "Display the hierarchical structure of a Figma file.

Shows pages, frames, and nested elements as a tree view.
Use --depth to control how deep into the tree to display.")]
    #[command(after_help = "EXAMPLES:
    fgm files tree abc123
    fgm files tree abc123 --depth 5
    fgm files tree \"https://www.figma.com/design/abc123/File\"")]
    Tree {
        /// Figma file key or URL
        file_key_or_url: String,
        /// Maximum depth to display (default: 3)
        #[arg(short, long, default_value = "3", help = "How many levels deep to show")]
        depth: u32,
    },

    /// Show file version history
    #[command(long_about = "List recent versions of a Figma file.

Shows version ID, timestamp, creator, and optional version name/description.
Useful for tracking design changes over time.")]
    #[command(after_help = "EXAMPLES:
    fgm files versions abc123
    fgm files versions abc123 --limit 20")]
    Versions {
        /// Figma file key or URL
        file_key_or_url: String,
        /// Number of versions to show (default: 10)
        #[arg(short, long, default_value = "10", help = "How many versions to display")]
        limit: u32,
    },
}

// Export subcommands
#[derive(Subcommand)]
pub enum ExportCommands {
    /// Export specific nodes or frames from a Figma file
    #[command(long_about = "Export images from a Figma file.

You can export specific nodes by ID, or use --all-frames to export all
top-level frames in the file. Supports PNG, SVG, PDF, and JPG formats.

Use --platform to generate all required sizes for iOS, Android, or Web.")]
    #[command(after_help = "EXAMPLES:
    # Export a single node from URL
    fgm export file \"https://figma.com/design/abc?node-id=1-2\" -o ./out/

    # Export multiple nodes
    fgm export file abc123 --node \"1:2\" --node \"1:3\" -o ./out/

    # Export all frames
    fgm export file abc123 --all-frames -o ./out/

    # Export as SVG
    fgm export file abc123 --node \"1:2\" --format svg -o ./out/

    # Export for iOS (generates @1x, @2x, @3x)
    fgm export file abc123 --node \"1:2\" --platform ios -o ./ios/

    # Export for Android (generates drawable-mdpi through xxxhdpi)
    fgm export file abc123 --node \"1:2\" --platform android -o ./android/")]
    File {
        /// Figma file key or URL (node-id in URL will be used automatically)
        #[arg(help = "File key (abc123) or URL with optional ?node-id=")]
        file_key_or_url: String,
        /// Node IDs to export (can specify multiple: --node \"1:2\" --node \"1:3\")
        #[arg(short, long, conflicts_with = "all_frames", help = "Node ID to export (repeatable)")]
        node: Vec<String>,
        /// Export all top-level frames in the file
        #[arg(long, conflicts_with = "node", help = "Export every top-level frame")]
        all_frames: bool,
        /// Image format: png, svg, pdf, jpg
        #[arg(short, long, help = "Output format")]
        format: Option<ExportFormat>,
        /// Scale factor (1-4, default: 2)
        #[arg(
            short,
            long,
            value_parser = clap::value_parser!(f32),
            help = "Scale multiplier (1-4)"
        )]
        scale: Option<f32>,
        /// Output directory
        #[arg(short, long, help = "Where to save exported files")]
        output: Option<PathBuf>,
        /// Custom filename (without extension, single node only)
        #[arg(long, help = "Override the output filename")]
        name: Option<String>,
        /// Generate platform-specific sizes (ios, android, web)
        #[arg(long, help = "Export all sizes for platform")]
        platform: Option<Platform>,
    },

    /// Batch export from a TOML manifest file
    #[command(long_about = "Export multiple assets defined in a TOML manifest file.

The manifest file defines which nodes to export, their formats, and output paths.
Useful for automating recurring exports or managing multiple assets.")]
    #[command(after_help = "MANIFEST FORMAT:
    [[exports]]
    file = \"abc123\"
    node = \"1:2\"
    name = \"icon\"
    format = \"png\"
    scale = 2
    output = \"./icons/\"

    [[exports]]
    file = \"https://figma.com/design/abc?node-id=1-3\"
    name = \"logo\"
    format = \"svg\"
    output = \"./logos/\"

EXAMPLE:
    fgm export batch my-assets.toml")]
    Batch {
        /// Path to manifest file (TOML format)
        #[arg(help = "Path to the TOML manifest file")]
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

impl ExportFormat {
    pub fn from_config(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "png" => Some(ExportFormat::Png),
            "svg" => Some(ExportFormat::Svg),
            "pdf" => Some(ExportFormat::Pdf),
            "jpg" | "jpeg" => Some(ExportFormat::Jpg),
            _ => None,
        }
    }
}

// Compare arguments
#[derive(clap::Args)]
pub struct CompareArgs {
    /// First image file (design export)
    #[arg(help = "Path to first image (typically Figma export)")]
    pub image1: PathBuf,
    /// Second image file (screenshot/implementation)
    #[arg(help = "Path to second image (typically dev screenshot)")]
    pub image2: PathBuf,
    /// Save visual diff image to this path
    #[arg(short, long, help = "Save diff visualization to file")]
    pub output: Option<PathBuf>,
    /// Maximum acceptable difference (default: 5%)
    #[arg(
        short,
        long,
        default_value = "5.0",
        value_parser = clap::value_parser!(f32),
        help = "Pass/fail threshold percentage"
    )]
    pub threshold: f32,
    /// Pixel tolerance per channel (0-255)
    #[arg(
        long,
        default_value = "10",
        value_parser = clap::value_parser!(u8),
        help = "Per-channel pixel tolerance"
    )]
    pub tolerance: u8,
    /// Stop early once threshold is exceeded (faster, approximate diff)
    #[arg(long, help = "Stop early once threshold is exceeded (faster)")]
    pub fast: bool,
    /// Compare all images in two directories
    #[arg(long, help = "Treat paths as directories, compare matching filenames")]
    pub batch: bool,
    /// Save JSON report to this path
    #[arg(short, long, help = "Save comparison results as JSON")]
    pub report: Option<PathBuf>,
}

// Tokens subcommands
#[derive(Subcommand)]
pub enum TokensCommands {
    /// List color styles from a Figma file
    #[command(long_about = "Extract all color styles defined in the Figma file.

Shows color name, hex value, and RGB components.
Colors are extracted from published styles in the file.")]
    Colors {
        /// Figma file key or URL
        #[arg(help = "File key (abc123) or Figma URL")]
        file_key: String,
    },

    /// List typography styles from a Figma file
    #[command(long_about = "Extract all text/typography styles defined in the Figma file.

Shows font family, size, weight, and line height for each style.")]
    Typography {
        /// Figma file key or URL
        #[arg(help = "File key (abc123) or Figma URL")]
        file_key: String,
    },

    /// Extract spacing values from auto-layout frames
    #[command(long_about = "Analyze auto-layout frames to extract spacing values.

Finds consistent padding and gap values used throughout the file.
Useful for building a spacing scale.")]
    Spacing {
        /// Figma file key or URL
        #[arg(help = "File key (abc123) or Figma URL")]
        file_key: String,
    },

    /// Export all design tokens to a file
    #[command(long_about = "Export all design tokens to JSON, CSS, Swift, or Kotlin format.

Combines colors, typography, and spacing into a single output file.
Useful for syncing design tokens to code.")]
    #[command(after_help = "EXAMPLES:
    fgm tokens export abc123 --format json -o tokens.json
    fgm tokens export abc123 --format css -o tokens.css
    fgm tokens export abc123 --format swift -o DesignTokens.swift
    fgm tokens export abc123 --format kotlin -o DesignTokens.kt")]
    Export {
        /// Figma file key or URL
        #[arg(help = "File key (abc123) or Figma URL")]
        file_key: String,
        /// Output format: json, css, swift, kotlin
        #[arg(short, long, default_value = "json", help = "Token output format")]
        format: TokenFormat,
        /// Output file path (prints to stdout if not specified)
        #[arg(short, long, help = "Save to file instead of stdout")]
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
    /// List all published components in a team library
    #[command(long_about = "List all components published to a team's component library.

Requires a team with a published component library. Shows component name,
key, description, and metadata.

Find your team ID in your Figma URL: figma.com/files/team/TEAM_ID/...")]
    #[command(after_help = "EXAMPLE:
    fgm components list 123456789")]
    List {
        /// Figma Team ID
        #[arg(help = "Team ID (find in Figma URL)")]
        team_id: String,
    },

    /// Get detailed info about a specific component
    #[command(long_about = "Retrieve detailed information about a published component.

Shows component name, description, containing frame/page, and timestamps.
The component key can be found in the component panel or via 'components list'.")]
    Get {
        /// Component key from Figma
        #[arg(help = "Component key (from Figma or 'components list')")]
        component_key: String,
    },
}

// Preview arguments
#[derive(clap::Args)]
pub struct PreviewArgs {
    /// Figma file key or URL
    #[arg(help = "File key (abc123) or Figma URL")]
    pub file_key: String,
    /// Specific node to preview (defaults to first frame)
    #[arg(short, long, help = "Node ID to preview (e.g., \"1:2\")")]
    pub node: Option<String>,
    /// Terminal width in columns (auto-detected if not specified)
    #[arg(short, long, help = "Output width in terminal columns")]
    pub width: Option<u32>,
    /// Force a specific terminal image protocol
    #[arg(short, long, help = "Image protocol: auto, sixel, iterm, kitty")]
    pub protocol: Option<ImageProtocol>,
}

#[derive(Clone, clap::ValueEnum)]
pub enum ImageProtocol {
    Auto,
    Sixel,
    Iterm,
    Kitty,
}

impl ImageProtocol {
    pub fn from_config(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "auto" => Some(ImageProtocol::Auto),
            "sixel" => Some(ImageProtocol::Sixel),
            "iterm" => Some(ImageProtocol::Iterm),
            "kitty" => Some(ImageProtocol::Kitty),
            _ => None,
        }
    }
}

// Compare URL arguments - export from Figma and compare in one command
#[derive(clap::Args)]
pub struct CompareUrlArgs {
    /// Figma URL with node-id (required)
    #[arg(help = "Figma URL with ?node-id= parameter")]
    pub figma_url: String,
    /// Local screenshot to compare against
    #[arg(help = "Path to screenshot image")]
    pub screenshot: PathBuf,
    /// Save visual diff image to this path
    #[arg(short, long, help = "Save diff visualization to file")]
    pub output: Option<PathBuf>,
    /// Maximum acceptable difference (default: 5%)
    #[arg(
        short,
        long,
        default_value = "5.0",
        value_parser = clap::value_parser!(f32),
        help = "Pass/fail threshold percentage"
    )]
    pub threshold: f32,
    /// Export scale factor (default: 2)
    #[arg(
        short,
        long,
        value_parser = clap::value_parser!(f32),
        help = "Scale multiplier for Figma export (1-4)"
    )]
    pub scale: Option<f32>,
    /// Pixel tolerance per channel (0-255)
    #[arg(
        long,
        default_value = "10",
        value_parser = clap::value_parser!(u8),
        help = "Per-channel pixel tolerance"
    )]
    pub tolerance: u8,
    /// Stop early once threshold is exceeded (faster, approximate diff)
    #[arg(long, help = "Stop early once threshold is exceeded (faster)")]
    pub fast: bool,
}

// Snapshot subcommands
#[derive(Subcommand)]
pub enum SnapshotCommands {
    /// Capture current design state as a named snapshot
    #[command(long_about = "Export and save the current state of a Figma file as a snapshot.

Snapshots are saved to a directory with metadata and exported images.
Use snapshots to track design changes over time and compare versions.")]
    #[command(after_help = "EXAMPLES:
    fgm snapshot create abc123 --name v1.0
    fgm snapshot create abc123 --name sprint-5 --node \"1:2\" --node \"1:3\"
    fgm snapshot create \"https://figma.com/...\" --name release-1.0")]
    Create {
        /// Figma file key or URL
        #[arg(help = "File key (abc123) or Figma URL")]
        file_key_or_url: String,
        /// Name/tag for this snapshot (e.g., \"v1.0\", \"sprint-5\")
        #[arg(short, long, help = "Unique name for this snapshot")]
        name: String,
        /// Specific nodes to snapshot (defaults to all frames)
        #[arg(long, help = "Node IDs to include (repeatable)")]
        node: Vec<String>,
        /// Directory to store snapshots
        #[arg(short, long, default_value = ".fgm-snapshots", help = "Snapshot storage directory")]
        output: PathBuf,
    },

    /// List all existing snapshots
    #[command(long_about = "Show all snapshots in the snapshots directory.

Displays snapshot name, creation date, file info, and node count.")]
    List {
        /// Snapshots directory to scan
        #[arg(short, long, default_value = ".fgm-snapshots", help = "Snapshot storage directory")]
        dir: PathBuf,
    },

    /// Compare two snapshots and show differences
    #[command(long_about = "Compare two snapshots to identify design changes.

Shows which frames changed, were added, or were removed.
Optionally generates visual diff images for changed frames.")]
    #[command(after_help = "EXAMPLES:
    fgm snapshot diff v1.0 v2.0
    fgm snapshot diff sprint-4 sprint-5 --output ./diffs/")]
    Diff {
        /// First (older) snapshot name
        #[arg(help = "Baseline snapshot name")]
        from: String,
        /// Second (newer) snapshot name
        #[arg(help = "Comparison snapshot name")]
        to: String,
        /// Snapshots directory
        #[arg(short, long, default_value = ".fgm-snapshots", help = "Snapshot storage directory")]
        dir: PathBuf,
        /// Save diff images to this directory
        #[arg(short, long, help = "Generate visual diff images")]
        output: Option<PathBuf>,
    },
}

// Sync arguments - declarative asset management
#[derive(clap::Args)]
pub struct SyncArgs {
    /// Path to sync manifest file (TOML)
    #[arg(help = "Path to the TOML manifest defining assets to sync")]
    pub manifest: PathBuf,
    /// Dry run - show what would be synced without downloading
    #[arg(long, help = "Preview changes without downloading")]
    pub dry_run: bool,
    /// Force re-download even if files exist
    #[arg(long, help = "Re-download all assets, even if unchanged")]
    pub force: bool,
}

// Map subcommands - component to code mapping
#[derive(Subcommand)]
pub enum MapCommands {
    /// Initialize a component map file from Figma components
    #[command(long_about = "Create a new component map by extracting components from a Figma file.

The map tracks which Figma components have been implemented in code.
This is the first step in setting up design-to-code tracking.")]
    #[command(after_help = "EXAMPLE:
    fgm map init abc123
    fgm map init \"https://figma.com/design/abc123/File\" -o my-components.toml")]
    Init {
        /// Figma file key or URL
        #[arg(help = "File key (abc123) or Figma URL")]
        file_key_or_url: String,
        /// Output file path
        #[arg(short, long, default_value = "figma-components.toml", help = "Where to save the component map")]
        output: PathBuf,
    },

    /// Check implementation coverage of mapped components
    #[command(long_about = "Display a coverage report showing implementation status.

Shows percentage of components implemented, lists pending items,
and identifies components that may need updates.")]
    #[command(after_help = "EXAMPLE:
    fgm map coverage
    fgm map coverage -m ./design-system.toml")]
    Coverage {
        /// Component map file
        #[arg(short, long, default_value = "figma-components.toml", help = "Path to component map")]
        map: PathBuf,
    },

    /// Update component map with latest components from Figma
    #[command(long_about = "Sync the component map with the current state of the Figma file.

Adds new components, removes deleted ones, and flags components
that have been modified since last sync.")]
    #[command(after_help = "EXAMPLE:
    fgm map update
    fgm map update -m ./design-system.toml")]
    Update {
        /// Component map file
        #[arg(short, long, default_value = "figma-components.toml", help = "Path to component map")]
        map: PathBuf,
    },

    /// Link a Figma component to its code implementation
    #[command(long_about = "Mark a component as implemented by linking it to a code file.

This updates the component map to track which code file implements
the Figma component, enabling coverage tracking.")]
    #[command(after_help = "EXAMPLES:
    fgm map link \"Button/Primary\" ./src/components/Button.tsx
    fgm map link \"Icon/Search\" ./src/icons/SearchIcon.vue -m design.toml")]
    Link {
        /// Component key or name (from 'map coverage' output)
        #[arg(help = "Component name or key to link")]
        component: String,
        /// Path to the code file that implements this component
        #[arg(help = "Path to implementation file")]
        code_path: PathBuf,
        /// Component map file
        #[arg(short, long, default_value = "figma-components.toml", help = "Path to component map")]
        map: PathBuf,
    },
}

// Export subcommand additions for platform-specific export
#[derive(Clone, clap::ValueEnum)]
pub enum Platform {
    /// iOS asset catalog (@1x, @2x, @3x)
    Ios,
    /// Android drawable resources (mdpi, hdpi, xhdpi, xxhdpi, xxxhdpi)
    Android,
    /// Web (1x, 2x)
    Web,
}

// Cache subcommands
#[derive(Subcommand)]
pub enum CacheCommands {
    /// Prefetch all data for a Figma file (warm the cache)
    #[command(long_about = "Warm the cache by fetching all data for a Figma file upfront.

This downloads file metadata, version history, and optionally image URLs.
Run this before batch operations to minimize API calls and avoid rate limits.

Cache is stored in ~/.cache/fgm/ and persists between sessions.")]
    #[command(after_help = "EXAMPLES:
    fgm cache warmup abc123
    fgm cache warmup \"https://figma.com/design/abc123/File\"
    fgm cache warmup abc123 --include-images")]
    Warmup {
        /// Figma file key or URL
        #[arg(help = "File key (abc123) or Figma URL")]
        file_key_or_url: String,
        /// Also prefetch image export URLs for all frames
        #[arg(long, help = "Include image URLs in cache warmup (takes longer)")]
        include_images: bool,
    },

    /// Show cache statistics
    #[command(long_about = "Display information about the current cache state.

Shows memory and disk cache entry counts, size, and location.")]
    Status,

    /// Clear cached data
    #[command(long_about = "Clear cached Figma API responses.

Use --all to clear the entire cache, or --file to clear data for a specific file.
Clearing cache forces fresh API calls on next operation.")]
    #[command(after_help = "EXAMPLES:
    fgm cache clear --all
    fgm cache clear --file abc123")]
    Clear {
        /// Clear entire cache
        #[arg(long, help = "Clear all cached data")]
        all: bool,
        /// Clear cache for specific file key
        #[arg(long, help = "Clear cache for specific file")]
        file: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn help_includes_output_flags() {
        let mut cmd = Cli::command();
        let help = cmd.render_help().to_string();
        assert!(help.contains("--format"));
        assert!(help.contains("--json"));
        assert!(help.contains("--no-color"));
    }
}

// Config subcommands
#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show effective configuration
    Show,

    /// Print config file path
    Path,

    /// Get a specific configuration value
    Get {
        /// Config key (e.g., defaults.output_format)
        key: String,
    },

    /// Set a configuration value
    Set {
        /// Config key (e.g., defaults.output_format)
        key: String,
        /// Value to set (omit with --unset)
        #[arg(required_unless_present = "unset")]
        value: Option<String>,
        /// Remove/unset the value (for optional keys)
        #[arg(long)]
        unset: bool,
    },
}
