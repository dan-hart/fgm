# DCP Session Log

## Metadata
- Date: 2026-03-02
- Repository: fgm
- Branch: main
- Session Type: Performance and rate-limit hardening

## What Was Done
- Implemented cache-first and low-rate performance enhancements across API and export flows.
- Added stale-aware cache reads to enable stale-while-revalidate behavior:
  - cache can now return freshness metadata (fresh/stale)
  - stale entries can be served while background refresh runs
- Canonicalized node ID handling for cache keys and endpoint calls:
  - sort/dedupe IDs
  - normalize encoded/alternate separators (`%3A`, `-` to `:` where needed)
- Added request coalescing (singleflight) for identical inflight API requests.
- Added endpoint-aware throttling with request-class accounting and token-bucket pacing.
- Added API and download concurrency control in the client:
  - constrained API request parallelism
  - separate download concurrency path for image fetches
- Updated files and images API access paths to use shared cache-first/coalesced behavior.
- Enhanced export pipeline for low-rate workflows:
  - new `--profile low-rate`
  - new `--delta` mode to skip export API calls when file version is unchanged
  - retry delay now considers `Retry-After` telemetry where available
  - dynamic batch sizing behavior for low-rate mode
  - download concurrency sourced from client policy
- Updated CLI help/docs with low-rate and delta examples.
- Added implementation plan doc for this feature set.

## Why It Was Done
- Primary objective: reduce rapid Figma rate-limit exhaustion while preserving actionable outputs for LLM-first workflows.
- Cache-first + coalescing lowers duplicate API pressure during repeated/parallel CLI usage.
- Endpoint-aware budgeting and lower API concurrency reduce spikes that trigger 429 responses.
- Delta/version-based skipping avoids unnecessary export URL and download work when designs are unchanged.

## Verification Evidence
- `cargo test` -> pass (`44 passed; 0 failed`)
- `cargo check` -> pass
- New/updated tests include:
  - cache freshness behavior
  - canonical cache hashing semantics
  - request-class limiter telemetry
  - token-bucket wait behavior
  - low-rate profile parsing and option defaults
  - delta skip decision helper coverage

## Knowledge Base Actions
- No KB updates applicable in this repository.
- Reason: this repository does not maintain a dedicated `knowledge/` tree; reusable guidance was added to `README.md` and CLI help text.

## Files of Interest
- `src/api/cache.rs`
- `src/api/client.rs`
- `src/api/files.rs`
- `src/api/images.rs`
- `src/api/rate_limit.rs`
- `src/commands/export.rs`
- `src/cli.rs`
- `README.md`
- `docs/plans/2026-03-02-low-rate-performance-and-caching.md`
- `research/session-logs/2026-03-02-1825-dcp-session-log.md`
