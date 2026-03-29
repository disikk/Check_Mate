# Progress: ingest v2 rearchitecture

## Context

- Plan: `docs/ingest_v2_rearchitecture_plan.md`
- Started: `2026-03-29`
- Branch: `codex/ingest-v2`

## Current batch

- Phase 0 baseline and quality gates.
- First Phase 1 foundation block: fast header sniffers for source kind and `tournament_id`.

## Checklist

- [x] Plan reviewed against current code.
- [x] Isolated worktree created on `codex/ingest-v2`.
- [x] Baseline verified with `npm test`.
- [x] Baseline verified with `bash scripts/backend_test.sh`.
- [x] Red: add failing tests for quick header sniffers.
- [x] Green: implement quick header sniffers in `tracker_parser_core`.
- [x] Add Phase 0 benchmark harness scripts and benchmark doc.
- [x] Update progress after first batch verification.
- [ ] Next batch: move from baseline harness to shared prepare-layer (`scan/classify/pair/reject`).

## Log

- `2026-03-29`: Reviewed the plan, created an isolated worktree, and confirmed a green baseline before code changes.
- `2026-03-29`: Added TDD-covered quick header sniffers for source kind and GG `tournament_id`, then introduced Phase 0 benchmark harness scripts plus `docs/INGEST_V2_BENCHMARKS.md`.
- `2026-03-29`: Verified `cargo test -p tracker_parser_core quick_ --test fixture_parsing`, `bash scripts/run_ingest_v2_mixed_scan.sh`, and the usage-path for `bash scripts/run_ingest_v2_bench.sh`.
