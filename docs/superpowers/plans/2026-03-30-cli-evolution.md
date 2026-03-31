# FGM CLI Evolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the approved `fgm` CLI roadmap phases in order: Foundation, Workflow UX, then Intelligence and scale.

**Architecture:** Keep the existing subcommand-oriented CLI and add a shared internal workflow layer for project bootstrapping, diagnostics, reporting, selection, watching, and orchestration. New capabilities should extend existing commands rather than replacing them.

**Tech Stack:** Rust, Clap, Tokio, Serde, anyhow, existing `fgm` command modules

---

### Task 1: Add shared foundations

**Files:**
- Create: `src/project.rs`
- Create: `src/reporting.rs`
- Create: `src/watch.rs`
- Create: `src/select.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing unit tests for project and reporting helpers**

Add tests for:
- project file path resolution
- default workspace directory layout
- report serialization for JSON and Markdown
- exit-code mapping for success, warning, and failure summaries

- [ ] **Step 2: Run focused tests to verify they fail**

Run: `cargo test project reporting`
Expected: FAIL because modules and functions do not exist yet

- [ ] **Step 3: Implement minimal shared helper modules**

Add focused utilities for:
- locating `fgm.toml`
- creating `.fgm` directories
- report model structs and file writers
- watch loop configuration and polling helpers
- selection helpers for optional pick modes

- [ ] **Step 4: Run focused tests to verify they pass**

Run: `cargo test project reporting`
Expected: PASS

- [ ] **Step 5: Run formatter**

Run: `cargo fmt`
Expected: exit 0

### Task 2: Implement Phase 1 Foundation commands

**Files:**
- Create: `src/commands/doctor.rs`
- Create: `src/commands/init.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Modify: `src/config.rs`
- Test: `src/commands/doctor.rs`
- Test: `src/commands/init.rs`

- [ ] **Step 1: Write failing tests for CLI parsing and command behavior**

Cover:
- `fgm doctor`
- `fgm init`
- `doctor` report output model
- `init` force/no-force workspace creation rules

- [ ] **Step 2: Run focused tests to verify they fail**

Run: `cargo test doctor init`
Expected: FAIL because the commands are not wired yet

- [ ] **Step 3: Implement `doctor`**

Add checks for:
- config path
- auth token availability
- auth validation
- cache directory presence
- project file presence
- report and snapshot directories

- [ ] **Step 4: Implement `init`**

Generate:
- `fgm.toml`
- `.fgm/`
- `.fgm/reports/`
- `.fgm/snapshots/`
- starter manifest files

- [ ] **Step 5: Add consistent report writing and exit codes**

Add `--report` and `--report-format` support where Phase 1 requires it, using the shared reporting module.

- [ ] **Step 6: Re-run focused tests**

Run: `cargo test doctor init`
Expected: PASS

- [ ] **Step 7: Run broader regression tests**

Run: `cargo test`
Expected: PASS

### Task 3: Implement Phase 2 Workflow UX

**Files:**
- Modify: `src/commands/export.rs`
- Modify: `src/commands/compare.rs`
- Modify: `src/commands/compare_url.rs`
- Modify: `src/commands/snapshot.rs`
- Modify: `src/commands/map.rs`
- Modify: `src/cli.rs`
- Modify: `src/reporting.rs`
- Modify: `src/watch.rs`
- Modify: `src/select.rs`

- [ ] **Step 1: Write failing tests for watch and pick behavior**

Cover:
- `--watch` parsing
- `--watch-interval` validation
- `--pick` parsing
- HTML report rendering
- watch rerun decision logic

- [ ] **Step 2: Run focused tests to verify they fail**

Run: `cargo test watch pick html`
Expected: FAIL

- [ ] **Step 3: Implement watch mode**

Add shared polling-based rerun behavior for:
- `export file`
- `compare-url`
- `snapshot create`

- [ ] **Step 4: Implement `--pick`**

Use the existing Figma tree fetch path to present numbered choices only when users opt into interactive selection.

- [ ] **Step 5: Implement HTML reports**

Add local-file HTML rendering for:
- compare
- compare-url
- snapshot diff
- sync
- doctor

- [ ] **Step 6: Run focused tests**

Run: `cargo test watch pick html`
Expected: PASS

- [ ] **Step 7: Run full tests**

Run: `cargo test`
Expected: PASS

### Task 4: Implement Phase 3 Intelligence and scale

**Files:**
- Modify: `src/commands/tokens.rs`
- Modify: `src/commands/map.rs`
- Modify: `src/commands/export.rs`
- Create: `src/commands/run.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/cli.rs`
- Modify: `src/reporting.rs`

- [ ] **Step 1: Write failing tests for new token formats, `map verify`, LLM pack enrichment, and batch manifests**

Cover:
- token format parsing and render output
- map verification diagnostics
- enriched manifest fields
- orchestration manifest parsing
- batch summary aggregation

- [ ] **Step 2: Run focused tests to verify they fail**

Run: `cargo test tokens map run llm`
Expected: FAIL

- [ ] **Step 3: Extend token exporters**

Add:
- Tailwind
- Style Dictionary JSON
- Android XML

- [ ] **Step 4: Add `map verify`**

Report:
- missing links
- broken links
- stale components
- duplicate targets

- [ ] **Step 5: Enrich `--llm-pack`**

Add schema version and richer asset metadata while preserving current fields.

- [ ] **Step 6: Add `fgm run <manifest>`**

Support multi-job batch execution and one summary report.

- [ ] **Step 7: Run focused tests**

Run: `cargo test tokens map run llm`
Expected: PASS

- [ ] **Step 8: Run full regression suite**

Run: `cargo test`
Expected: PASS

### Task 5: Finish and verify

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/specs/2026-03-30-cli-evolution-design.md`
- Modify: `docs/superpowers/plans/2026-03-30-cli-evolution.md`

- [ ] **Step 1: Update user-facing docs**

Document:
- new commands
- new report formats
- watch mode
- pick mode
- project init workflow
- batch orchestration

- [ ] **Step 2: Run formatting and tests**

Run:
- `cargo fmt`
- `cargo test`

Expected:
- formatter exits 0
- all tests pass

- [ ] **Step 3: Review git diff for consistency**

Run: `git status --short` and `git diff --stat`
Expected: only intentional files changed

- [ ] **Step 4: Prepare completion handoff**

Summarize:
- implemented commands
- docs updated
- test evidence
- any follow-up cleanup still worth doing
