# Backend Foundation

`Check_Mate` is being built as one integrated product: student cabinets, MBR Stats replacement, GG MBR parser/normalizer, and future tracker/filter/stat capabilities on one data model. This directory is the backend foundation for that unified core.

## Scope

- only `GG MBR`
- no `Chico` support in this project branch
- PostgreSQL is the source of truth
- real HH/TS fixtures from MBR exports are the golden parser pack
- parser architecture: `tracker_parser_core` + `parser_worker`

## Runtime Status (2026-03-24)

- canonical repo-level onboarding is Docker-first from the project root
- root entrypoints are `docker-compose.yml`, `scripts/`, and `Makefile`
- canonical DB contract is `CHECK_MATE_DATABASE_URL="host=localhost port=5432 user=postgres password=postgres dbname=check_mate_dev"`
- migrations `0001_init_source_of_truth.sql`, `0002_exact_pot_ko_core.sql`, and seed `0001_reference_data.sql` are applied by `bash scripts/db_bootstrap.sh`
- `bash scripts/db_bootstrap.sh` also re-syncs the PostgreSQL role password to `.env`, so reused Docker volumes do not keep stale auth credentials
- backend-specific entrypoints remain:
  - `backend/scripts/bootstrap_backend_dev.sh`
  - `backend/scripts/run_backend_checks.sh`
- a legacy local Homebrew PostgreSQL 16 cluster may still exist on `localhost:5433` as a maintainer fallback on this Mac, but it is not the canonical first-run path
- GitHub Actions backend gate now lives in `.github/workflows/backend-foundation.yml`

## Layout

- `migrations/` - SQL schema for the source-of-truth database
- `seeds/` - reference seed scripts
- `fixtures/mbr/hh/` - committed golden GG MBR hand histories
- `fixtures/mbr/ts/` - committed golden GG MBR tournament summaries
- `crates/tracker_parser_core/` - parser and normalizer core
- `crates/parser_worker/` - local CLI/import smoke worker
- `crates/mbr_stats_runtime/` - typed feature registry, per-hand materializer, and seed aggregate queries

## Current Parser State

- file kind detection: `hh` / `ts`
- tournament summary parsing
- hand-history splitting into individual hands
- canonical GG MBR hand parsing:
  - seats
  - action events
  - final board runout
  - hero hole cards
  - showdown hands
  - collected amounts
  - parse warnings
- first replay-grade normalizer slice:
  - terminal all-in snapshot detection
  - committed-by-street tracking
  - actual final stacks/winner collections
  - repeated `collect` accumulation across main/side pots
  - exact elimination detection by end-of-hand stack resolution
  - invariant calculation for chip and pot conservation

## Current Import State

`parser_worker import-local` now writes:

- `import.source_files`
- `import.import_jobs`
- `import.file_fragments`
- `core.tournaments`
- `core.tournament_entries`
- `core.hands`
- `core.hand_seats`
- `core.hand_hole_cards`
- `core.hand_actions`
- `core.hand_boards`
- `core.hand_showdowns`
- `core.hand_pots`
- `core.hand_pot_contributions`
- `core.hand_pot_winners`
- `core.hand_returns`
- `core.parse_issues`
- `derived.hand_state_resolutions`
- `derived.hand_eliminations`
- `derived.street_hand_strength`
- `derived.mbr_stage_resolution`
- `analytics.player_hand_bool_features`
- `analytics.player_hand_num_features`
- `analytics.player_hand_enum_features`

Hand-child persistence is intentionally idempotent:

- `core.hands` is upserted by `(player_profile_id, external_hand_id)`
- child rows are replaced for the current hand before re-insert
- `derived.street_hand_strength` rows are replaced for the current `hand_id + descriptor_version` before re-insert
- post-import runtime features are full-refreshed for the current `player_profile_id` and `mbr_runtime_v1`

## Testing

- `cargo test` covers fixture parsing and first normalizer invariants
- `bash scripts/db_bootstrap.sh` is the canonical repo bootstrap
- `bash scripts/backend_test.sh` is the canonical root backend smoke gate
- `bash backend/scripts/bootstrap_backend_dev.sh` and `bash backend/scripts/run_backend_checks.sh` remain backend-focused helper gates
- `parser_worker` has:
  - a unit test for canonical hand -> persistence row mapping
  - a unit test for normalized hand -> `hand_state_resolutions` mapping
  - a unit test for exact elimination extraction on an FT all-in hand
  - a unit test for FT/rush `mbr_stage_resolution` mapping
  - a dedicated parser-core test suite for exact `street_hand_strength` descriptors
  - a regression test for repeated `collect` lines by the same player
  - an ignored integration test for real PostgreSQL writes
  - an ignored integration test for analytics refresh and seed runtime queries

## Known Limits

- hand/tournament timestamps are still stored as `NULL` until GG MBR timezone normalization is specified exactly
- `derived.mbr_stage_resolution` now persists the exact `played_ft_hand` slice, exact `ft_table_size`, and the estimated last-rush boundary candidate hand
- `derived.hand_eliminations` now persists exact pot-specific KO attribution including `resolved_by_pot_no`, `hero_involved`, `hero_share_fraction`, `is_split_ko`, `split_n`, `is_sidepot_based`, and `certainty_state`
- `derived.street_hand_strength` now persists exact street-level descriptors for Hero and showdown-known opponents, but `is_nut_hand` / `is_nut_draw` remain intentionally `NULL` in v1
- `mbr_stats_runtime` currently exposes only seed exact-safe stats and still intentionally excludes session/date filters
- boundary-zone and boundary-KO fields are still intentionally unresolved/uncertain beyond the boundary-candidate flag
- server FT stats and AST/filter engine come later in the plan

## Useful Commands

```bash
cd backend
bash scripts/bootstrap_backend_dev.sh
cargo test
bash scripts/run_backend_checks.sh
cargo run -p parser_worker -- "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- import-local "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- import-local "fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"
```

`import-local` expects `CHECK_MATE_DATABASE_URL` in the environment.

For a generic local setup, use the Docker PostgreSQL from the project root and export:

```bash
export CHECK_MATE_DATABASE_URL="host=localhost port=5432 user=postgres password=postgres dbname=check_mate_dev"
```
