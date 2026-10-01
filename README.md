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

## Mobile And Agent Workflows

Project defaults are discovered from the nearest parent `fgm.toml`. Relative
export and snapshot directories resolve against that file, not the current shell
directory. Explicit command-line values win, including an explicit threshold of 5.

```toml
[project]
name = "example-app"
[figma]
source = "https://www.figma.com/design/abc123/Example"
[figma.aliases]
settings-dark = "https://www.figma.com/design/abc123/Example?node-id=1-2"
[export]
output_dir = "designs"
scale = 2
[compare]
threshold = 3
[snapshot]
dir = ".fgm/snapshots"
```

```bash
# Export the saved source without repeating configuration
fgm export
fgm export file settings-dark

# Find frames/components; saving an alias requires exactly one match
fgm find Settings --page Mobile --exact --save-as settings-dark

# Capture a booted simulator or an authorized Android device
fgm capture --simulator booted -o screenshot.png
fgm capture --android connected -o screenshot.png
fgm compare-url settings-dark --simulator booted

# Crop screenshot safe areas, ignore volatile regions, and write a review bundle
fgm review design.png --screenshot screenshot.png --crop-top 48 --crop-bottom 34 \
  --mask 0,0,100,30 --output review
fgm review --source settings-dark --android connected --normalize --open

# Scope an agent pack to matching screens or a node URL/alias
fgm pack --query Settings --page Mobile --output agent-pack --changed-only

# Resolve variables and aliases in every mode, with semantic native identifiers
fgm variables --source abc123 --target swift -o Variables.swift
fgm variables --import variables.json --target kotlin -o Variables.kt

# Offline source coverage; paths are relative to the project or map directory
fgm check-map .fgm/components.toml --root .
fgm map link Settings src/SettingsView.swift --symbol SettingsView

# Credentials, settings, cache and keychain entries isolated by account
fgm --account personal auth login --keychain
fgm --account team export file abc123 --all-frames
```

`review` writes `index.html`, `report.json`, design, screenshot, overlay and diff
PNGs. The HTML uses only relative assets and works offline. Difference percentages
exclude masked pixels; overlapping masks are counted once. Masks are applied to
the saved images, not just the score. `--normalize` is opt-in because resizing can
hide size/layout errors. Without it, dimension mismatches always fail, as they now
also do in `compare`, batch comparison and `compare-url`.

Use `review --share` to omit Figma source URLs and version metadata. This is **not
automatic privacy certification**: inspect the images for names, notifications,
proprietary designs and other sensitive content before sharing. Default generated
directories and `fgm.toml` are gitignored in this repository; custom output paths
must be ignored separately.

Packs contain a contact sheet and full selected node trees, including layout
measurements, typography and component properties supplied by Figma. Manifest
entries include contact-sheet indexes and source links but no signed download
URLs. `--changed-only` skips unchanged-version exports and otherwise checks fresh
image content before rewriting assets. A file-version change triggers a fresh
render to account for external style/variable dependencies. Pack metadata is local
design data and must also be reviewed before sharing.

Local variables JSON must contain `meta.variables` and `meta.variableCollections`
(or those objects directly at the root), matching the
[Figma Variables REST response](https://developers.figma.com/docs/rest-api/variables-endpoints/).
Live access depends on plan, seat, token scopes and file permissions. Missing alias
targets and cycles fail rather than silently producing partial tokens. Cross-
collection aliases use the target collection's default mode when no matching mode
ID exists; extended collections are rejected explicitly. Swift, Kotlin and CSS
exports group tokens by collection and mode. Existing `tokens export` native output
also preserves paint opacity and includes typography helpers. Compose typography
retains font-family metadata; callers must resolve custom font resources themselves.

Maps may add `symbol = "SettingsView"` alongside `code_path`. The offline checker
recognizes common Swift, Kotlin and JavaScript/TypeScript declarations and ignores
comments/string literals. It is a source-presence check, not a compiler, inheritance
resolver or proof that a UI renders correctly.

Account names use letters, digits, hyphens and underscores. The existing export
`--profile` remains a preset, not an account. `FIGMA_TOKEN` still has highest
priority for all accounts; unset it to use account-specific stored credentials.

## Authentication Setup

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
# Diagnose local setup
fgm doctor

# Bootstrap a local workspace
fgm init .

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
# Local workspace setup
fgm init . --figma "https://www.figma.com/design/abc123/MyFile"
fgm doctor --report ./.fgm/reports/doctor.html --report-format html

# File inspection
fgm files get "https://www.figma.com/design/abc123/MyFile"
fgm files tree abc123 --depth 3
fgm files versions abc123 --limit 10

# Local image comparison
fgm compare design.png screenshot.png --threshold 5 --output diff.png
fgm compare design.png screenshot.png --report compare.md --report-format md

# Token export
fgm tokens export abc123 --format css -o tokens.css
fgm tokens export abc123 --format tailwind -o tailwind.tokens.js
fgm tokens export abc123 --format style-dictionary -o tokens.sd.json
fgm tokens export abc123 --format android-xml -o values/fgm_tokens.xml

# Terminal preview
fgm preview abc123 --node "1:2"
fgm preview abc123 --pick

# Cache utilities
fgm cache status
fgm cache warmup abc123 --include-images
fgm cache clear --file abc123

# Watch mode
fgm export file abc123 --pick --watch -o ./exports/
fgm compare-url "https://www.figma.com/design/abc123/MyFile?node-id=1-2" screenshot.png --watch

# Mapping and orchestration
fgm map verify -m .fgm/components.toml --report ./.fgm/reports/map.html --report-format html
fgm run jobs.toml --report ./.fgm/reports/run.json
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
