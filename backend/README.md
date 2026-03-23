# Backend Foundation

`Check_Mate` is being built as one integrated product: student cabinets, MBR Stats replacement, GG MBR parser/normalizer, and future tracker/filter/stat capabilities on one data model. This directory is the backend foundation for that unified core.

## Scope

- only `GG MBR`
- no `Chico` support in this project branch
- PostgreSQL is the source of truth
- real HH/TS fixtures from MBR exports are the golden parser pack
- parser architecture: `tracker_parser_core` + `parser_worker`

## Runtime Status (2026-03-23)

- `cargo`, `rustc`, `psql`, and PostgreSQL are installed locally
- local database `check_mate_dev` exists
- migration `0001_init_source_of_truth.sql` and seed `0001_reference_data.sql` were applied successfully
- Docker Desktop is installed, but Docker Linux engine is still blocked by disabled firmware virtualization (`AMD-V` / `SVM`) on this PC

## Layout

- `migrations/` - SQL schema for the source-of-truth database
- `seeds/` - reference seed scripts
- `fixtures/mbr/hh/` - raw GG MBR hand histories
- `fixtures/mbr/ts/` - raw GG MBR tournament summaries
- `crates/tracker_parser_core/` - parser and normalizer core
- `crates/parser_worker/` - local CLI/import smoke worker

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
- `core.parse_issues`
- `derived.hand_state_resolutions`
- `derived.hand_eliminations`
- `derived.mbr_stage_resolution`

Hand-child persistence is intentionally idempotent:

- `core.hands` is upserted by `(player_profile_id, external_hand_id)`
- child rows are replaced for the current hand before re-insert

## Testing

- `cargo test` covers fixture parsing and first normalizer invariants
- `parser_worker` has:
  - a unit test for canonical hand -> persistence row mapping
  - a unit test for normalized hand -> `hand_state_resolutions` mapping
  - a unit test for exact elimination extraction on an FT all-in hand
  - a unit test for FT/rush `mbr_stage_resolution` mapping
  - a regression test for repeated `collect` lines by the same player
  - an ignored integration test for real PostgreSQL writes

## Known Limits

- hand/tournament timestamps are still stored as `NULL` until GG MBR timezone normalization is specified exactly
- `derived.mbr_stage_resolution` now persists the exact `played_ft_hand` slice, exact `ft_table_size`, and the estimated last-rush boundary candidate hand
- `derived.hand_eliminations` now persists the first exact elimination slice
- pot-specific KO attribution fields in `derived.hand_eliminations` are still intentionally conservative:
  - `resolved_by_pot_no` is not resolved yet
  - split/side-pot hero-share flags are not derived yet
- boundary-zone and boundary-KO fields are still intentionally unresolved/uncertain beyond the boundary-candidate flag
- server FT stats and AST/filter engine come later in the plan

## Useful Commands

```bash
cargo test
cargo run -p parser_worker -- "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- import-local "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- import-local "fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"
```

`import-local` expects `CHECK_MATE_DATABASE_URL` in the environment.
