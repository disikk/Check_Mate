# Progress: nested ZIP ingest

## Context

- Design: `docs/plans/2026-03-30-nested-zip-ingest-design.md`
- Plan: `docs/plans/2026-03-30-nested-zip-ingest-plan.md`
- Started: `2026-03-30`
- Branch: `main`

## Checklist

- [x] Scope agreed: support nested ZIP in the shared ingest layer, not only in local CLI import.
- [x] Design approved.
- [x] Red: nested ZIP prepare tests added and failing.
- [x] Red: nested ZIP runtime read test added and failing.
- [x] Green: shared archive locator codec added.
- [x] Green: recursive prepare scan + recursive hash implemented.
- [x] Green: recursive runtime archive reading implemented.
- [x] Green: invalid leaf guard for NUL-containing text implemented.
- [x] Docs updated (`CLAUDE.md`, ingest progress).
- [x] Focused tests green.
- [x] Full backend checks green.
- [x] KONFA `Orange` import clean benchmark with exact timing and throughput metrics.

## Log

- `2026-03-30`: Confirmed the new requirement: ZIP files must be recursively expanded in the shared ingest layer, valid HH/TS leaf-files must be imported, and invalid files must be dropped.
- `2026-03-30`: Chose the no-migration design: keep a typed archive locator in code and serialize nested archive chains into the existing `member_path` contract.
- `2026-03-30`: Added red coverage for nested ZIP pair discovery in `tracker_ingest_prepare`, nested archive-member runtime reads in `parser_worker`, and NUL-containing archive text rejection.
- `2026-03-30`: Implemented recursive ZIP traversal/hashing in `tracker_ingest_prepare`, recursive archive-member reads in `parser_worker`, and a regression upload-classification test in `tracker_web_api` for nested ZIP pair ordering.
- `2026-03-30`: Focused tests are green: full `tracker_ingest_prepare/tests/prepare_report.rs`, targeted `parser_worker` nested read/NUL guard tests, and `tracker_web_api` nested ZIP upload classification.
- `2026-03-30`: Full `bash backend/scripts/run_backend_checks.sh` passes on a clean temporary database `check_mate_nested_zip_20260330`; the reused local `check_mate_dev` instance was already polluted by duplicate `auth.users.email` fixture data and is not a trustworthy gate target right now.
- `2026-03-30`: While profiling the real `KONFA/Orange` corpus, found that `parser_worker` was reopening and reparsing the same archive for every nested/member read. Added a red/green regression test and switched archive-member reads to a per-worker in-memory cache so repeated reads from the same ZIP stay on the cached archive structure instead of reloading from disk.
- `2026-03-30`: Discovered and fixed a critical `materialize_refresh` query bottleneck: `load_bundle_tournament_ids` joined `import_jobs` to `core.hands` via `source_file_id`, but for member-aware archives all 16K jobs share one parent `source_file_id` (prepared-pairs.zip) creating a 16K × 220K cross product. Fixed by joining through `source_file_member_id → file_fragments → hands.raw_fragment_id`. Added migration `0030_hands_source_file_index.sql`.
- `2026-03-30`: Clean `Orange` benchmark results — 220,679 hands, 8,288 tournaments, 8 workers: **E2E 782.5s (13.0 min), 16,920 hands/min e2e, 282 hands/sec e2e**. Prepare 57.3s, runner 725.2s, finalize (materialize) 305.7s, peak RSS 3.4 GB.
- `2026-03-30`: Re-ran the clean `Orange` benchmark on the current workspace state and captured the full JSON report at `/tmp/checkmate-orange-bench/orange_w8_fresh.json`. Verified result: **220,679 hands, 8,288 tournaments, 8 workers, 1,441.4s e2e (24m 01.4s), 9,185.76 hands/min e2e, 153.10 hands/sec e2e**. Prepare wall time was `71.3s` (`4.95%` of e2e), runner wall time `1,370.1s`, and finalize/materialize tail `399.9s` aggregated. This measured rerun does **not** match the earlier `782.5s / 16,920 hands/min` note above, so the benchmark history now needs an explicit environment/commit reconciliation before using the older figure for decisions.
