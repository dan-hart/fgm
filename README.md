# fgm

`fgm` is a Figma CLI for exporting screens/assets, comparing designs, and extracting tokens.

It is optimized for fast repeated runs from human or LLM workflows:
- URL-first usage (`fgm "<figma-url>"`)
- cache-first API behavior (disk + memory)
- low-rate export profile and delta skip mode

> Note: Independent hobby project (not affiliated with Figma/Adobe).
> Platform status: primarily tested on macOS.

## Install

### Homebrew

```bash
brew tap dan-hart/tap
brew install fgm
```

### From source

```bash
git clone https://github.com/dan-hart/fgm.git
cd fgm
cargo build --release
cp target/release/fgm ~/.local/bin/
```

### Check install

```bash
fgm --version
```

## Authentication

Get a Figma Personal Access Token:
https://www.figma.com/developers/api#access-tokens

Use one of these:

```bash
# Environment variable (highest priority)
export FIGMA_TOKEN="figd_your_token_here"

# Or store in config (default)
fgm auth login

# Or store in keychain (opt-in)
fgm auth login --keychain

# Verify
fgm auth status
```

Token resolution order:
1. `FIGMA_TOKEN`
2. config file
3. keychain

## Quick Start

```bash
# Export all top-level screens from a Figma URL (quick mode)
fgm "https://www.figma.com/design/abc123/MyFile"

# Write images + manifest.json for LLM use
fgm "https://www.figma.com/design/abc123/MyFile" --llm-pack -o ./llm-pack/

# Export one specific frame
fgm export file "https://www.figma.com/design/abc123/MyFile?node-id=1-2" -o ./out/
```

## Recommended LLM Workflow

### First run (build artifacts + metadata)

```bash
fgm "https://www.figma.com/design/abc123/MyFile" \
  --profile low-rate \
  --llm-pack \
  -o ./llm-pack/
```

### Follow-up runs (skip unchanged versions)

```bash
fgm "https://www.figma.com/design/abc123/MyFile" \
  --profile low-rate \
  --delta \
  -o ./llm-pack/
```

### Compare against implementation screenshot

```bash
fgm compare-url "https://www.figma.com/design/abc123/MyFile?node-id=1-2" app-screen.png --threshold 3
```

## Export Flags You Will Use Most

- `--llm-pack`: writes `manifest.json` with asset metadata + telemetry.
- `--profile pixel-perfect`: PNG-focused stable exports for visual checks.
- `--profile low-rate`: conservative batching + cache/rate-limit friendly behavior.
- `--delta`: skip export URL/image fetches if file version is unchanged.
- `--resume`: skip rewriting unchanged output files.
- `--format {png|svg|pdf|jpg}` and `--scale N`: output control.
- `-o, --output`: output directory.

## Other Useful Commands

```bash
# File inspection
fgm files get "https://www.figma.com/design/abc123/MyFile"
fgm files tree abc123 --depth 3
fgm files versions abc123 --limit 10

# Local image comparison
fgm compare design.png screenshot.png --threshold 5 --output diff.png

# Token export
fgm tokens export abc123 --format css -o tokens.css

# Terminal preview
fgm preview abc123 --node "1:2"

# Cache utilities
fgm cache status
fgm cache warmup abc123 --include-images
fgm cache clear --file abc123
```

## Current Rate-Limit Strategy (Built In)

`fgm` now defaults to a cache-first and low-churn approach:
- persistent disk+memory cache for API reads
- canonicalized cache keys for nodes/exports
- stale-while-revalidate cache usage
- singleflight request coalescing for duplicate inflight API calls
- endpoint-aware throttling with adaptive pacing
- adaptive export batch sizing with retry/backoff behavior
- separate API vs download concurrency control

For machine workflows, `--llm-pack` includes telemetry fields like:
- `api_calls`
- `export_batches`
- `cache_hits`
- `cache_misses`
- rate-limit counters

## Config

```bash
fgm config path
fgm config show
fgm config get defaults.output_format
fgm config set export.default_scale 2
```

## Troubleshooting

```bash
# Auth problems
fgm auth debug

# Disable keychain prompts for a run
fgm --no-keychain auth status

# Get verbose logs
fgm --verbose export file abc123 --all-frames

# Inspect cache state
fgm cache status
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
