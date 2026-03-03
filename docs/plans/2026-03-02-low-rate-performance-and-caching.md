# Low-Rate Performance and Caching Improvements Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make fgm aggressively cache-first and rate-limit-resilient with request coalescing, endpoint-aware throttling, better concurrency controls, and delta exports.

**Architecture:** Introduce a client policy + limiter layer for global API behavior, enhance cache semantics (stale-while-revalidate + canonical keys), and wire export flow to use low-rate defaults and delta skip logic.

**Tech Stack:** Rust, tokio, reqwest, existing `api/{cache,client,rate_limit}` and `commands/export` modules.

---

### Task 1: Cache semantics and canonical keys
- Add cache lookup API that can return stale entries.
- Canonicalize node-id hashing (sort + dedupe + normalized separators).
- Add tests for canonicalization and stale lookup behavior.

### Task 2: Limiter and client policy
- Add endpoint-classified request budgeting in `RateLimiter`.
- Add client-side request coalescing for identical inflight keys.
- Add API/download concurrency semaphores and expose download concurrency getter.
- Add tests for endpoint budgeting and coalescing-safe key behavior.

### Task 3: API endpoint wiring
- Update `files` and `images` API methods to use singleflight + stale-while-revalidate + canonical IDs.
- Ensure all reads remain cache-first by default.

### Task 4: Export flow low-rate enhancements
- Add `low-rate` export profile.
- Add `--delta` mode and resume-index file-version checks to skip unchanged exports.
- Honor `Retry-After` during adaptive export retries.
- Use client download concurrency in export executor.
- Add tests for profile defaults and delta decision helpers.

### Task 5: Validation
- Run targeted tests for updated modules.
- Run full `cargo test` + `cargo check`.
