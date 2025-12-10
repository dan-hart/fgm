# fgm - Figma CLI

A fast, cross-platform command-line interface for Figma. Export assets, compare designs, extract tokens, and preview screens directly in your terminal.

## Features

- **Export** - Download PNG, SVG, PDF, JPG from any Figma file or node
- **Compare** - Pixel-diff designs against dev screenshots with threshold-based CI pass/fail
- **Tokens** - Extract colors and typography to JSON, CSS, Swift, or Kotlin
- **Preview** - View Figma frames directly in terminal (iTerm2, Kitty, Sixel)
- **Browse** - Navigate files, versions, and document tree structure
- **Components** - List and inspect published library components

## Installation

### From Source

```bash
# Clone and build
git clone https://github.com/danhart/fgm.git
cd fgm
cargo build --release

# Install to PATH
cp target/release/fgm ~/.local/bin/
```

### Requirements

- Rust 1.70+ (for building)
- Figma Personal Access Token (PAT)

## Quick Start

```bash
# 1. Set your Figma token
export FIGMA_TOKEN="figd_your_token_here"

# Or store it securely in your keychain
fgm auth login

# 2. Get file info from a Figma URL
fgm files get "https://www.figma.com/design/abc123/MyFile"

# 3. Export a frame
fgm export file "https://www.figma.com/design/abc123/MyFile?node-id=1-2" -o ./exports/
```

## Authentication

fgm uses Figma Personal Access Tokens (PATs). Get yours at:
https://www.figma.com/developers/api#access-tokens

### Token Storage (Priority Order)

1. `FIGMA_TOKEN` environment variable
2. System keychain (macOS Keychain / Linux Secret Service)
3. Config file `~/.config/fgm/config.toml`

```bash
# Store token in keychain
fgm auth login

# Check authentication status
fgm auth status

# Remove stored token
fgm auth logout
```

## Commands

### Export Assets

```bash
# Export a specific node from URL
fgm export file "https://www.figma.com/design/abc123/File?node-id=1-2" -o ./out/

# Export multiple nodes
fgm export file abc123 --node "1:2" --node "1:3" -o ./out/

# Export all top-level frames
fgm export file abc123 --all-frames -o ./out/

# Export as SVG at 1x scale
fgm export file abc123 --node "1:2" --format svg --scale 1 -o ./out/

# Batch export from manifest
fgm export batch manifest.toml
```

**Manifest file example (`manifest.toml`):**

```toml
[[exports]]
file = "abc123"
node = "1:2"
name = "login-screen"
format = "png"
scale = 2
output = "./designs/"

[[exports]]
file = "abc123"
node = "1:3"
name = "dashboard"
format = "svg"
```

### Compare Designs

```bash
# Compare two images
fgm compare design.png screenshot.png

# Set threshold for CI (exit 1 if diff > 5%)
fgm compare design.png screenshot.png --threshold 5

# Generate visual diff image
fgm compare design.png screenshot.png --output diff.png

# Batch compare directories
fgm compare ./designs/ ./screenshots/ --batch --report report.json
```

**JSON Report Output:**

```json
{
  "total": 10,
  "passed": 8,
  "failed": 2,
  "threshold": 5.0,
  "results": [
    {"file": "login.png", "diff_percent": 0.5, "passed": true},
    {"file": "header.png", "diff_percent": 12.3, "passed": false}
  ]
}
```

### Extract Design Tokens

```bash
# List color styles
fgm tokens colors abc123

# List typography styles
fgm tokens typography abc123

# Export all tokens to JSON
fgm tokens export abc123 --format json -o tokens.json

# Export to CSS variables
fgm tokens export abc123 --format css -o tokens.css

# Export to Swift
fgm tokens export abc123 --format swift -o FigmaTokens.swift

# Export to Kotlin
fgm tokens export abc123 --format kotlin -o FigmaColors.kt
```

### Preview in Terminal

```bash
# Preview first frame of a file
fgm preview abc123

# Preview specific node
fgm preview abc123 --node "1:2"

# Force specific protocol
fgm preview abc123 --protocol iterm
fgm preview abc123 --protocol kitty
fgm preview abc123 --protocol sixel

# Set terminal width
fgm preview abc123 --width 80
```

**Supported terminals:** iTerm2, Kitty, WezTerm, Ghostty, terminals with Sixel support

### Browse Files

```bash
# Get file metadata
fgm files get "https://www.figma.com/design/abc123/File"

# Show document tree structure
fgm files tree abc123 --depth 3

# Show version history
fgm files versions abc123 --limit 10

# List projects in a team
fgm files list --team 123456

# List files in a project
fgm files list --project 789012
```

### Published Components

```bash
# List components in a team library
fgm components list TEAM_ID

# Get component details
fgm components get COMPONENT_KEY
```

## URL Support

fgm accepts Figma URLs anywhere a file key is expected:

```bash
# All these work
fgm files get abc123
fgm files get "https://www.figma.com/file/abc123/Name"
fgm files get "https://www.figma.com/design/abc123/Name?node-id=1-2"

# Node ID is automatically extracted from URL
fgm export file "https://www.figma.com/design/abc123/Name?node-id=1-2"
# Equivalent to:
fgm export file abc123 --node "1:2"
```

## Configuration

Optional config file at `~/.config/fgm/config.toml`:

```toml
[defaults]
team_id = "123456789"
output_format = "table"

[export]
default_format = "png"
default_scale = 2
output_dir = "~/Downloads/figma"
```

## Rate Limits

The Figma API has rate limits. fgm handles this automatically:
- Batches exports in chunks of 20 nodes
- Adds delays between batches
- Retries with backoff on rate limit errors

For large exports, consider using `--all-frames` with a smaller file or exporting in batches.

## Security

- Tokens are stored securely in your system keychain
- `git-secrets` hooks prevent accidental credential commits
- `.gitignore` patterns block common credential files

## Development

```bash
# Run tests
cargo test

# Build debug version
cargo build

# Run with debug output
RUST_LOG=debug cargo run -- files get abc123
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Credits

Built with:
- [clap](https://github.com/clap-rs/clap) - CLI framework
- [reqwest](https://github.com/seanmonstar/reqwest) - HTTP client
- [viuer](https://github.com/atanunq/viuer) - Terminal image display
- [image](https://github.com/image-rs/image) - Image processing

Figma API documentation: https://www.figma.com/developers/api
