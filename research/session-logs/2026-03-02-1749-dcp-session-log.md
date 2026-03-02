# DCP Session Log

## Metadata
- Date: 2026-03-02
- Repository: fgm
- Branch: main
- Session Type: Feature implementation + release prep

## What Was Done
- Implemented LLM-first quick export mode: `fgm "<figma-url>"` now routes to default all-frame export flow.
- Added export enhancements for LLM workflows:
  - `--llm-pack` manifest generation with image/file metadata and telemetry
  - `--profile pixel-perfect` preset
  - resume/incremental export behavior via checksum index
- Improved API efficiency and rate-limit behavior:
  - adaptive export batching
  - bounded concurrent image downloads
  - canonicalized cache hash for export params
  - rate-limit telemetry collection surfaced in export output/manifest
- Hardened Figma URL parsing:
  - scheme-less URL support
  - fragment/query node-id normalization
  - stricter host validation to reject lookalike domains
- Updated docs for quick mode, llm-pack, profile behavior, and rate-limit strategy.
- Bumped crate version from `0.1.1` to `1.3.1`.

## Why It Was Done
- Primary goal: make fgm more usable by LLM-first workflows while reducing API pressure and still producing actionable image artifacts.
- Quick mode and llm-pack reduce command complexity and improve machine readability for downstream agents.
- Adaptive batching + cache normalization improve performance under API limits and reduce redundant calls.

## Verification Evidence
- `cargo test` -> pass (`34 passed; 0 failed`)
- `cargo check` -> pass
- ASP preflight + git-secrets will be executed on staged changes prior to commit.

## Knowledge Base Actions
- No KB updates applicable in this repository.
- Reason: this repo does not maintain a dedicated `knowledge/` documentation tree; release-facing guidance was added directly to `README.md`.

## Files of Interest
- `src/commands/export.rs`
- `src/cli.rs`
- `src/main.rs`
- `src/api/url.rs`
- `src/api/rate_limit.rs`
- `src/api/client.rs`
- `src/api/cache.rs`
- `README.md`
- `Cargo.toml`
- `Cargo.lock`
