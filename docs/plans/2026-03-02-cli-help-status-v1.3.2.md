# CLI Help + Export Progress UX (v1.3.2) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `fgm` clearer and more responsive by improving help/docs examples and adding progress/status output so exports and long-running actions don’t appear stalled.

**Architecture:** Improve CLI copy and examples in `clap` help + README, then add deterministic progress message helpers in export flow and wire them into URL-resolution and download phases. Keep behavior unchanged except for user-facing messaging.

**Tech Stack:** Rust, clap, tokio, indicatif, existing `output` module.

---

### Task 1: Add tests for updated CLI help examples

**Files:**
- Modify: `src/cli.rs`

**Steps:**
1. Add failing tests asserting top-level help includes quick-export URL examples and default output behavior.
2. Add failing tests asserting `export file --help` includes LLM-first examples (`--llm-pack`, `--profile pixel-perfect`).
3. Run: `cargo test cli::tests::` and confirm failures before implementation.

### Task 2: Add tests for export progress/status message helpers

**Files:**
- Modify: `src/commands/export.rs`

**Steps:**
1. Add pure-function tests for progress line formatting (`resolved x/y`, `downloaded x/y`).
2. Add failing tests first.
3. Run: `cargo test commands::export::tests::` and confirm failures.

### Task 3: Implement help/docs improvements

**Files:**
- Modify: `src/cli.rs`
- Modify: `README.md`

**Steps:**
1. Update top-level `after_help` with clearer quick-mode defaults and examples.
2. Update export help examples for LLM-first workflows.
3. Add README section showing concrete commands and expected status output.
4. Re-run `cargo test cli::tests::`.

### Task 4: Implement export status/progress improvements

**Files:**
- Modify: `src/commands/export.rs`

**Steps:**
1. Add status lines for URL resolution phase (batch-level progress).
2. Add status lines for download phase (periodic progress + completion).
3. Keep quiet/json behavior consistent through `output::print_status`.
4. Re-run `cargo test commands::export::tests::`.

### Task 5: Verify and release

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `research/session-logs/YYYY-MM-DD-HHMM-dcp-session-log.md`

**Steps:**
1. Bump version to `1.3.2`.
2. Run verification: `cargo test` and `cargo check`.
3. DCP: create session log, preflight, commit, tag `v1.3.2`, push.
4. Update Homebrew tap formula to `v1.3.2` + new sha256, push.
5. Verify local install reports `fgm 1.3.2`.
