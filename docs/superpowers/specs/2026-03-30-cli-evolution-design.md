# FGM CLI Evolution Design

## Goal

Implement the three approved roadmap phases for `fgm`:

1. Foundation
2. Workflow UX
3. Intelligence and scale

The result should stay script-first, preserve the current command-line ergonomics, and add optional interactive behavior only when a user explicitly asks for it with flags such as `--interactive` or `--pick`.

## Current State

`fgm` already has a strong command-oriented shape:

- [src/cli.rs](../../../src/cli.rs) defines a clear Clap-driven interface.
- [src/main.rs](../../../src/main.rs) dispatches each command module cleanly.
- [src/config.rs](../../../src/config.rs) persists defaults and auth fallback state.
- Command modules such as [src/commands/export.rs](../../../src/commands/export.rs), [src/commands/compare.rs](../../../src/commands/compare.rs), [src/commands/sync.rs](../../../src/commands/sync.rs), and [src/commands/map.rs](../../../src/commands/map.rs) already cover the core Figma workflows.

The missing piece is not “more commands” by itself. It is a reusable workflow layer that lets new features share:

- consistent environment diagnosis
- repeatable project bootstrap rules
- target discovery and optional selection
- watch/re-run behavior
- structured report emission
- stable exit code semantics

## Recommended Architecture

Add a small shared internal workflow layer rather than growing each command independently.

### New Internal Modules

Create a focused set of new modules:

- `src/commands/doctor.rs`
  Runs environment and project diagnostics.
- `src/commands/init.rs`
  Bootstraps local project files and directories.
- `src/project.rs`
  Shared project bootstrap, manifest discovery, output path, and workspace helpers.
- `src/reporting.rs`
  Shared report models and writers for JSON, Markdown, JUnit, and HTML.
- `src/select.rs`
  Node and component selection helpers, including optional interactive pickers.
- `src/watch.rs`
  Polling-based watch loop with stable re-run semantics.

These modules support the existing command set instead of replacing it.

## Guiding Principles

### Script First

The default experience remains non-interactive and automation-friendly:

- commands should work in CI and shell scripts
- flags always override config
- commands emit predictable output
- exit codes reflect success, drift, and user-fixable misconfiguration

### Optional Interactivity

Interactive behavior is opt-in:

- `--interactive` for setup flows
- `--pick` for node/component selection when a TTY is available
- interactive prompts never become the default path

### Backward Compatibility

Existing commands and common flags should keep working:

- existing export, compare, snapshot, sync, tokens, and map flows remain valid
- new report formats and metadata should extend existing output, not silently break it
- LLM pack changes should add fields while preserving currently documented keys

## Phase 1: Foundation

Phase 1 improves first-run and repeat-run experience on a local machine.

### 1. `fgm doctor`

Add a new top-level `doctor` command that checks:

- token availability and active token source
- token validity against the Figma API
- config path resolution and parse health
- cache directory existence and read/write access
- current workspace heuristics
  - repo detected or not
  - `fgm.toml` present or not
  - snapshot and report directories present or not
- output directory writability when supplied
- optional fixable issues such as missing local directories

Output behavior:

- human-readable status table by default
- JSON when `--json` or `--format json` is active
- stable per-check statuses: `ok`, `warn`, `fail`

Exit code behavior:

- `0`: all required checks passed
- `1`: one or more required checks failed
- `2`: usage or local input error

### 2. `fgm init`

Add a new top-level `init` command that bootstraps a local `fgm` workspace.

Default script-first behavior:

- writes `fgm.toml`
- creates `.fgm/`
- creates `.fgm/reports/`
- creates `.fgm/snapshots/`
- optionally writes starter manifests for sync and map workflows

Suggested generated files:

- `fgm.toml`
  Project-level defaults for export, reports, snapshots, compare thresholds, and optional Figma source.
- `.fgm/sync.toml`
  Starter sync manifest.
- `.fgm/components.toml`
  Starter component map path for `map` workflows.

Flag model:

- `--interactive`
- `--force`
- `--output-dir <path>`
- `--file <path>` for custom config location
- `--figma <url-or-key>` to seed project metadata
- `--yes` to suppress confirmations if any are needed in interactive mode

### 3. Shared Report and Exit Code Rules

Phase 1 establishes cross-command conventions used by later phases.

Add shared report generation for:

- `doctor`
- `compare`
- `compare-url`
- `snapshot diff`
- `sync`

Formats in Phase 1:

- JSON
- Markdown
- JUnit XML for CI-friendly pass/fail summaries

Commands should be able to write reports with a consistent flag family:

- `--report <path>`
- `--report-format <json|md|junit|html>`

Phase 1 ships JSON, Markdown, and JUnit. HTML becomes fully realized in Phase 2.

## Phase 2: Workflow UX

Phase 2 improves daily iteration speed without changing the script-first stance.

### 4. Watch Mode

Add watch support through shared polling utilities.

Initial watch targets:

- `export file --watch`
- `compare-url --watch`
- `snapshot create --watch`

Behavior:

- poll Figma file version on a configurable interval
- when the version changes, re-run the underlying operation
- reuse existing `--delta` and cache behavior when available
- support `--watch-interval <seconds>`
- write reports after each run when requested

### 5. Optional Pickers

Add `--pick` for commands that benefit from node or component selection:

- `export file --pick`
- `preview --pick`
- `snapshot create --pick`
- `map init --pick` for scoping a generated map

Picker behavior:

- in non-interactive mode, commands continue to require explicit node IDs or use existing defaults
- in interactive mode, users get a numbered list backed by the already-fetched Figma tree
- `--pick` is rejected cleanly when stdout/stdin is not a TTY

### 6. HTML Reports

Add HTML report generation for:

- `compare`
- `compare-url`
- `snapshot diff`
- `sync`
- `doctor`

The HTML report should be local-file friendly and dependency-light:

- inline CSS
- optional thumbnail references to already-generated images
- summary cards
- per-item tables
- clear pass/fail status

This report is for local dev inspection first, not a hosted dashboard.

## Phase 3: Intelligence and Scale

Phase 3 extends the CLI from local utility into a stronger team and automation tool.

### 7. Richer Token Export Targets

Extend token export formats beyond the current set.

Keep:

- JSON
- CSS
- Swift
- Kotlin

Add:

- Tailwind config fragment
- Style Dictionary JSON
- Android XML resources

The export layer should normalize shared token data once, then render format-specific outputs through dedicated formatter functions.

### 8. `fgm map verify`

Add a new `map verify` command that audits a component map for:

- missing code paths
- broken file links
- duplicate code targets
- components present in Figma but missing in the map
- components present in the map but removed from Figma

This builds on the current `map coverage` and `map update` features without replacing them.

### 9. Richer `--llm-pack`

Extend the export manifest with additive metadata:

- page name
- node path
- export source type
- original node type
- export options used
- cache and delta behavior
- telemetry per run

The manifest should remain compatible with current consumers by keeping existing fields and adding a `schema_version`.

### 10. Batch Orchestration

Add a shared orchestration entry point for repeatable multi-job runs.

Recommended command:

- `fgm run <manifest>`

Manifest capabilities:

- export jobs
- compare jobs
- compare-url jobs
- sync jobs
- snapshot jobs

Design goals:

- deterministic ordering
- partial failure reporting
- resume-friendly output
- one summary report for the whole run

This should coexist with `export batch` and existing sync manifests rather than immediately replacing them.

## Command Surface Changes

### New Top-Level Commands

- `doctor`
- `init`
- `run`

### Existing Command Enhancements

- `export file`: `--watch`, `--watch-interval`, `--pick`, richer `--llm-pack`
- `compare`: `--report-format`
- `compare-url`: `--watch`, `--watch-interval`, `--report-format`
- `snapshot create`: `--watch`, `--pick`
- `snapshot diff`: `--report-format`
- `sync`: `--report-format`
- `map`: add `verify`
- `tokens export`: more output formats

## Data and File Strategy

### Project File

`fgm.toml` becomes the project-local anchor for defaults, but stays optional.

Proposed sections:

- `[project]`
- `[export]`
- `[compare]`
- `[snapshot]`
- `[reports]`
- `[figma]`

This project file is distinct from the existing user config in the OS config directory:

- user config remains machine-level
- `fgm.toml` is workspace-level

### Generated Workspace Layout

`fgm init` should default to:

- `fgm.toml`
- `.fgm/`
- `.fgm/reports/`
- `.fgm/snapshots/`
- `.fgm/sync.toml`
- `.fgm/components.toml`

## Error Handling

All new shared layers should return typed, user-facing errors through `anyhow` at the command boundary.

Requirements:

- configuration errors should name the file path
- invalid user input should suggest the next useful command
- watch mode should keep running on transient API failures unless a fatal config error occurs
- report writing failures should fail the command clearly when the report path was explicitly requested

## Testing Strategy

Use unit tests first for the new shared layers, then command tests for CLI parsing and behavior.

Coverage targets:

- doctor check aggregation and exit-code mapping
- init file generation and force behavior
- report writers for JSON, Markdown, JUnit, and HTML
- watch loop rerun decisions
- picker eligibility and node selection
- token format renderers
- map verify diagnostics
- batch manifest parsing and orchestration summaries

## Recommended Delivery Order

Implement the phases in the approved order:

1. Foundation
2. Workflow UX
3. Intelligence and scale

Within each phase, land reusable shared pieces before command-specific polish.

## Risks

- Adding too many command-specific one-off options would fragment the UX.
- Making project config mandatory would hurt lightweight use cases.
- Rich HTML or interactive flows could bloat a fast CLI if they pull in heavy dependencies.
- Batch orchestration can become a second, inconsistent manifest system if it does not build on existing export/sync concepts.

## Recommendation

Proceed with the shared workflow layer approach:

- keep the current command model
- add project-local bootstrap and diagnostics
- unify reports and exit semantics
- add watch and pick as thin reusable extensions
- treat Phase 3 as additive intelligence on top of stable Phase 1 and 2 foundations
