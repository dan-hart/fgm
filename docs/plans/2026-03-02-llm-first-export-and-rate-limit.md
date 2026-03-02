# LLM-First URL Export and Rate-Limit Optimization Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a first-class LLM-first workflow where `fgm "<figma-url>"` exports usable screen images by default, with stronger URL handling, lower API pressure, and richer machine-readable outputs.

**Architecture:** Extend CLI parsing to accept bare positional URL/file-key as a quick-export path routed to export logic. Enhance export pipeline with adaptive batching, bounded concurrent downloads, and optional `--llm-pack` manifests plus incremental file skipping. Improve shared URL/cache/rate-limit internals with deterministic keys, telemetry, and profile presets.

**Tech Stack:** Rust, clap, tokio, reqwest, serde/serde_json, image, existing fgm modules.

---

### Task 1: Add bare input quick-export command path

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Test: `src/cli.rs`

**Step 1: Write failing tests**
- Add tests asserting CLI accepts `fgm <url>` and `fgm <file_key>` and routes into quick-export args.

**Step 2: Run targeted tests (expect fail)**
- Run: `cargo test cli::tests::`.

**Step 3: Implement minimal parser changes**
- Add optional top-level positional `input` with `subcommand` precedence.
- Add `to_export_request()` helper returning normalized quick-export request.

**Step 4: Route in main**
- If `command` is present, keep existing behavior.
- If only `input` is present, dispatch to quick export (`all_frames=true`, default PNG).

**Step 5: Re-run tests**
- Run: `cargo test cli::tests::`.

### Task 2: Add LLM pack output mode and manifest

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/commands/export.rs`
- Create/Modify tests in `src/commands/export.rs`

**Step 1: Write failing tests**
- Add unit tests for llm-pack manifest filename/path metadata generation.

**Step 2: Run test (expect fail)**
- Run: `cargo test commands::export::tests::`.

**Step 3: Implement llm-pack**
- Add `--llm-pack` + optional `--manifest-name` to export file args and quick mode.
- Emit `manifest.json` with node metadata + image paths + dimensions + source URL.

**Step 4: Re-run tests**
- Run: `cargo test commands::export::tests::`.

### Task 3: Improve URL parsing and normalization

**Files:**
- Modify: `src/api/url.rs`
- Test: `src/api/url.rs`

**Step 1: Write failing tests**
- Add cases for extra query params, encoded node-id, board/proto/design/file variants, and no-protocol host shortcuts.

**Step 2: Run test (expect fail)**
- Run: `cargo test api::url::tests::`.

**Step 3: Implement parser hardening**
- Normalize host + path parsing.
- Normalize node IDs (`-`/`%3A` to `:`), trim fragments.

**Step 4: Re-run tests**
- Run: `cargo test api::url::tests::`.

### Task 4: Use cheaper frame discovery path

**Files:**
- Modify: `src/api/files.rs`
- Modify: `src/commands/export.rs`
- Test: `src/commands/export.rs`

**Step 1: Write failing tests**
- Add tests for frame extraction from lightweight metadata tree payload.

**Step 2: Run test (expect fail)**
- Run: `cargo test commands::export::tests::`.

**Step 3: Implement**
- Add lightweight frame listing path (`/files/{key}/meta` based fallback) and prefer it for `--all-frames`.

**Step 4: Re-run tests**
- Run: `cargo test commands::export::tests::`.

### Task 5: Bounded concurrent downloads

**Files:**
- Modify: `src/commands/export.rs`
- Test: `src/commands/export.rs`

**Step 1: Write failing tests**
- Add unit tests for filename stability/order with concurrent scheduling helpers.

**Step 2: Run test (expect fail)**
- Run: `cargo test commands::export::tests::`.

**Step 3: Implement**
- Download export URLs concurrently with semaphore (configurable cap), preserve deterministic output naming.

**Step 4: Re-run tests**
- Run: `cargo test commands::export::tests::`.

### Task 6: Adaptive batch sizing for export URL requests

**Files:**
- Modify: `src/commands/export.rs`
- Modify: `src/api/client.rs` and/or `src/api/rate_limit.rs`
- Test: `src/commands/export.rs` and `src/api/rate_limit.rs`

**Step 1: Write failing tests**
- Add pure-function tests for adaptive batch adjustment behavior.

**Step 2: Run tests (expect fail)**
- Run: `cargo test api::rate_limit::tests:: commands::export::tests::`.

**Step 3: Implement**
- Start from current batch size; shrink on 429/rate-limit signals; cautiously grow on stable success.

**Step 4: Re-run tests**
- Run: `cargo test api::rate_limit::tests:: commands::export::tests::`.

### Task 7: Canonicalize cache keys for image exports

**Files:**
- Modify: `src/api/cache.rs`
- Test: `src/api/cache.rs`

**Step 1: Write failing tests**
- Assert identical IDs in different order produce same hash.

**Step 2: Run tests (expect fail)**
- Run: `cargo test api::cache::tests::`.

**Step 3: Implement**
- Sort/dedupe IDs before hashing in `hash_export_params`.

**Step 4: Re-run tests**
- Run: `cargo test api::cache::tests::`.

### Task 8: Add machine-readable telemetry for LLM automation

**Files:**
- Modify: `src/api/rate_limit.rs`
- Modify: `src/api/client.rs`
- Modify: `src/commands/export.rs`
- Test: `src/api/rate_limit.rs`

**Step 1: Write failing tests**
- Add tests for counter increments/reset and summary serialization shape.

**Step 2: Run tests (expect fail)**
- Run: `cargo test api::rate_limit::tests::`.

**Step 3: Implement**
- Track API calls, retries, throttle delays, cache hits.
- Emit telemetry summary in JSON output or llm-pack manifest.

**Step 4: Re-run tests**
- Run: `cargo test api::rate_limit::tests::`.

### Task 9: Add incremental/resume behavior

**Files:**
- Modify: `src/commands/export.rs`
- Test: `src/commands/export.rs`

**Step 1: Write failing tests**
- Add tests for skip-if-unchanged decision helper and checksum index format.

**Step 2: Run tests (expect fail)**
- Run: `cargo test commands::export::tests::`.

**Step 3: Implement**
- Add `--resume` (default true for quick mode) with checksum index in output dir; skip writing unchanged assets.

**Step 4: Re-run tests**
- Run: `cargo test commands::export::tests::`.

### Task 10: Add pixel-perfect export profile preset

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/commands/export.rs`
- Test: `src/commands/export.rs` and `src/cli.rs`

**Step 1: Write failing tests**
- Add tests for profile-to-options resolution and precedence with explicit flags.

**Step 2: Run tests (expect fail)**
- Run: `cargo test cli::tests:: commands::export::tests::`.

**Step 3: Implement**
- Add `--profile pixel-perfect` preset (`png`, recommended `scale`, deterministic naming, llm metadata defaults).

**Step 4: Re-run tests**
- Run: `cargo test cli::tests:: commands::export::tests::`.

### Task 11: Documentation and version bump prep

**Files:**
- Modify: `README.md`
- Modify: `Cargo.toml`
- Update lockfile: `Cargo.lock`

**Step 1: Update docs**
- Add quick mode examples: `fgm "<url>"`.
- Add llm-pack and profile docs + telemetry notes.

**Step 2: Bump version**
- Set package version to `1.3.1`.
- Update user-agent to use `env!("CARGO_PKG_VERSION")`.

**Step 3: Regenerate lockfile metadata**
- Run: `cargo check`.

### Task 12: Full verification and release-ready status (without commit/push)

**Files:**
- Verify entire workspace

**Step 1: Run full test suite**
- Run: `cargo test`

**Step 2: Run build check**
- Run: `cargo check`

**Step 3: Capture release commands for later approval**
- Prepare (do not execute yet):
  - `git add ...`
  - `git commit -m "feat: add llm-first quick export and rate-limit optimizations"`
  - `git tag v1.3.1`
  - `git push && git push --tags`
