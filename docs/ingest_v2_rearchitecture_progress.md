# Progress: ingest v2 rearchitecture

## Context

- Plan: `docs/ingest_v2_rearchitecture_plan.md`
- Started: `2026-03-29`
- Branch: `main`

## Current batch

- Current target: finish real-corpus profiling on `MIHA` after the multi-worker claim-path fix and decide whether the next wave should target long-lived HH transactions, `street_strength`, or DB persist.

## Checklist

- [x] Plan reviewed against current code.
- [x] Isolated worktree created on `codex/ingest-v2`.
- [x] Baseline verified with `npm test`.
- [x] Baseline verified with `bash scripts/backend_test.sh`.
- [x] Red: add failing tests for quick header sniffers.
- [x] Green: implement quick header sniffers in `tracker_parser_core`.
- [x] Add Phase 0 benchmark harness scripts and benchmark doc.
- [x] Update progress after first batch verification.
- [x] Merge previous batch back into `main` and stop using the temporary worktree.
- [x] Red: add failing tests for shared prepare-layer crate.
- [x] Green: implement `tracker_ingest_prepare` for directory/ZIP scan, pair and reject report.
- [x] Expose prepare-layer through `parser_worker dir-import --prepare-only`.
- [x] Sync `CLAUDE.md`, `backend/README.md` and benchmark docs with the new architecture.
- [x] Red: add failing tests for pair-first `tracker_web_api` ZIP and multi-file upload classification.
- [x] Green: reuse `tracker_ingest_prepare` in `tracker_web_api` for ZIP uploads and multipart batch pairing.
- [x] Preserve legacy single flat-file upload path for debug/partial TS-only scenarios.
- [x] Red: add failing runtime tests for pair-aware member dependencies, claim gating, and `dependency_failed` propagation.
- [x] Green: add `depends_on_job_id` queue contract in `tracker_ingest_runtime` and wire `tracker_web_api` pair-first uploads to it.
- [x] Fix member-level ingest schema drift by removing the legacy unique `(source_file_id, fragment_index)` index from `import.file_fragments`.
- [x] Add multi-worker runner execution on top of the dependency-aware queue.
- [x] Expose `tracker_ingest_runner --workers <n>` with default `min(available_parallelism, 8)`.
- [x] Keep legacy `bulk_local_import` serial until its enqueue contract becomes pair-first and dependency-aware.
- [x] Add real `parser_worker dir-import --player-profile-id <uuid> [--workers <n>] <path>` on top of shared prepare-layer and dependency-aware runtime queue.
- [x] Switch the happy-path ingest v2 bench harness from legacy `bulk_local_import` to pair-first `dir-import`.
- [x] Refactor HH ingest profiling so hand-local derive, tournament derive, and DB persist are measured separately.
- [x] Add honest dir-import e2e output contract with `rejected_by_reason`, `prep_elapsed_ms`, `runner_elapsed_ms`, `e2e_elapsed_ms`, and `hands_per_minute_runner/e2e`.
- [x] Harden prepare-layer so non-UTF8 files and ZIP members are rejected as `unsupported_source` instead of aborting the whole directory import.
- [x] Red: reproduce same-bundle multi-worker claim serialization with two concurrent DB transactions.
- [x] Green: remove bundle-row mutation from the claim hot path, switch claim-time event snapshots to read-only mode, and keep bundle-status writes only for real status transitions.

## Log

- `2026-03-29`: Reviewed the plan, created an isolated worktree, and confirmed a green baseline before code changes.
- `2026-03-29`: Added TDD-covered quick header sniffers for source kind and GG `tournament_id`, then introduced Phase 0 benchmark harness scripts plus `docs/INGEST_V2_BENCHMARKS.md`.
- `2026-03-29`: Verified `cargo test -p tracker_parser_core quick_ --test fixture_parsing`, `bash scripts/run_ingest_v2_mixed_scan.sh`, and the usage-path for `bash scripts/run_ingest_v2_bench.sh`.
- `2026-03-29`: Merged the first batch back into `main`, removed the temporary `codex/ingest-v2` worktree, and started the shared prepare-layer batch directly from the main project.
- `2026-03-29`: Added `tracker_ingest_prepare` with TDD coverage for directory scan, ZIP member scan, HH/TS pairing, duplicate collapse, conflict detection, and reject reporting.
- `2026-03-29`: Exposed the prepare-layer through `parser_worker dir-import --prepare-only`, switched `scripts/run_ingest_v2_mixed_scan.sh` to the new entrypoint, and re-verified `npm test` plus full `bash scripts/backend_test.sh`.
- `2026-03-29`: Switched `tracker_web_api` ZIP uploads to the shared prepare-layer, so only valid HH+TS pairs are enqueued while reject reasons are logged as ingest diagnostics.
- `2026-03-29`: Switched multipart multi-file uploads to batch-level pairing through `tracker_ingest_prepare`, while keeping the legacy single flat-file debug path for TS-only partial-state workflows.
- `2026-03-29`: Added pair-aware queue dependencies in `tracker_ingest_runtime` through `import.import_jobs.depends_on_job_id`, so HH member jobs are claimable only after successful TS completion and auto-fail with `dependency_failed` when the dependency terminally fails.
- `2026-03-29`: Switched pair-first multipart uploads to a synthetic archive contract, so real ZIP uploads and prepared multi-file batches now hit the same member-level dependency-aware runtime path.
- `2026-03-29`: Added schema cleanup migration for `import.file_fragments`, dropping the stale unique `(source_file_id, fragment_index)` index that broke multi-member archive ingest after successful TS jobs.
- `2026-03-29`: Added `run_ingest_runner_parallel(...)` to `parser_worker::local_import`, so dependency-aware ingest jobs can now drain through several PostgreSQL clients/threads instead of one serial runner loop.
- `2026-03-29`: Added `tracker_ingest_runner --workers <n>` and `CHECK_MATE_INGEST_RUNNER_WORKERS`, with default worker budget `min(available_parallelism, 8)` and DB-backed smoke coverage for a dependency-aware archive pair.
- `2026-03-29`: Left `bulk_local_import` intentionally serial because its flat file-first enqueue path still lacks explicit `TS -> HH` dependencies; instead, the new dev happy-path now goes through `parser_worker dir-import`.
- `2026-03-29`: Added real pair-first `parser_worker dir-import`, which reuses `tracker_ingest_prepare`, materializes a temporary pair-only archive, enqueues dependency-aware member jobs, and can drain them with `--workers <n>`.
- `2026-03-29`: Switched `scripts/run_ingest_v2_bench.sh` to the new dir-import path, so the happy-path benchmark no longer depends on legacy `bulk_local_import`.
- `2026-03-29`: Split HH runtime profiling into hand-local derive, tournament derive, and pure DB persist stages, while keeping the old aggregated `stage_profile` for compatibility.
- `2026-03-29`: Upgraded `parser_worker dir-import` output to a real e2e contract with `rejected_by_reason`, `prep_elapsed_ms`, `runner_elapsed_ms`, `e2e_elapsed_ms`, `hands_per_minute_runner`, `hands_per_minute_e2e`, and nested `e2e_profile`.
- `2026-03-29`: Hardened `tracker_ingest_prepare` so invalid non-UTF8 files or archive members are surfaced as `unsupported_source` reject diagnostics instead of aborting the whole scan on real corpora like `MIHA`.
- `2026-03-29`: Reproduced a real same-bundle multi-worker stall on `MIHA`: concurrent workers were serializing on `UPDATE import.ingest_bundles` during `claim_next_job`, so almost no real parallel HH work started on large bundles.
- `2026-03-29`: Fixed that runtime bottleneck with a DB-backed regression test: claim-time bundle/file events now read a derived snapshot without mutating `import.ingest_bundles`, and status row writes stay on actual status transitions instead of every claim.
- `2026-03-29`: Re-ran `MIHA` profiling after the claim-path fix. On the `sample-50` paired subset, `8 workers` improved from `554.28` to `2287.86 hands/min` (`4.13x`).
- `2026-03-29`: Stopped the fresh full `MIHA` run after the bottleneck picture became clear: at `1450s` it had already persisted `22142` hands / `850` tournaments with `1693` succeeded file jobs and zero terminal file failures, which confirms the queue/runtime fix works on the real corpus too.
- `2026-03-29`: The next likely hotspot is no longer bundle-row locking but expensive event/snapshot telemetry on large bundles: `file_updated/bundle_updated` still build payloads through full bundle snapshot reads, which is now the main candidate for the next acceleration wave together with separating HH compute from long transactions.
