# MBR Core Handoff

## Purpose

This is the committed cross-machine handoff for the current `Check_Mate` MBR-core buildout.
Read this first when resuming on another machine or in another chat session.

## Product framing

- `Check_Mate` is one unified product.
- Student cabinets, MBR Stats replacement, GG MBR parser/normalizer, and future tracker/filter/stat engine must share one canonical source-of-truth model.
- Parser quality bar is replay-grade: normalized hands must be reconstructable for a replayer and must support arbitrary future stats via filters.
- Architectural reference remains `D:\coding\poker-ev-tracker`, adapted for GG MBR rather than Chico.

## Pushed commits

- `4098a7e` - `Add GG MBR parser foundation and derived groundwork`
- `ed7491d` - `Ignore local backend fixtures and planning artifacts`

## Current backend state

### Database

- PostgreSQL source-of-truth foundation exists in `backend/migrations/0001_init_source_of_truth.sql`.
- Reference seed exists in `backend/seeds/0001_reference_data.sql`.
- Schemas are already laid out for `auth`, `org`, `import`, `core`, `derived`, `analytics`.

### Parser / normalizer

- Rust workspace lives in `backend/`.
- `tracker_parser_core` currently parses:
  - tournament summaries
  - GG MBR hand headers
  - seats
  - action events
  - hero hole cards
  - showdown hands
  - collected amounts
  - board runout
  - parse warnings
- `normalize_hand` currently derives:
  - terminal all-in snapshot
  - committed totals
  - final stacks
  - chip / pot invariants
  - exact elimination rows for players who finish the hand with stack `0`

### Import persistence

`parser_worker import-local` currently writes:

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

## Important exactness findings already fixed

### 1. FT stage logic

- `played_ft_hand` is exact and based on real `9-max` hands.
- The last chronological `5-max` hand before the first chronological `9-max` hand is persisted as the boundary candidate:
  - `entered_boundary_zone = true`
  - `entered_boundary_zone_state = estimated`

Verified examples:

- `BR1064986938` -> first chronological FT hand, `ft_table_size = 9`
- `BR1064987693` -> later short-handed FT hand, `ft_table_size = 2`
- `BR1065004819` -> boundary candidate rush hand

### 2. Exact eliminations

- `derived.hand_eliminations` is now persisted.
- Current exact rule:
  - player started hand with positive stack
  - player finished hand with stack `0`
- Current intentionally conservative limits:
  - `resolved_by_pot_no` is not resolved yet
  - `hero_involved`, split-KO attribution, hero share fraction, and side-pot attribution are not exact yet

Verified example:

- `BR1064987693` -> eliminated seat `3`, player `f02e54a6`, `ko_involved_winner_count = 1`

### 3. Critical canonical parser bug fixed

- GG HH can contain repeated lines `player collected X from pot` for the same player in one hand.
- This was previously overwriting instead of accumulating in `collected_amounts`.
- It is now fixed.

Verified example:

- `BR1064987148` -> `aaab99dd` collects `4,764` and `2,136`, total must be `6,900`
- After re-import:
  - `pot_conservation_ok = true`
  - `final_stacks['aaab99dd'] = 7572`

## Next honest dependency

Do not jump straight to `boundary_ko_ev`.

The next dependency is exact:

1. `hero_involved`
2. split-KO attribution
3. side-pot-based KO attribution

inside `derived.hand_eliminations`.

Only after that should `boundary_ko_ev` be implemented from the formula in `mbr_ev_tracker_tz.md`.

## Local-only things that do NOT travel through git

These are intentionally ignored:

- `backend/fixtures` is now repo-tracked as the committed sanitized golden GG MBR pack
- `docs/plans`
- `.claude`

Implication:

- the repo is enough to recover architecture/context on another machine
- but fixture-driven tests and real `import-local` smoke runs need the fixture pack copied manually

## What to copy manually to the Mac if you want runnable tests/imports

Copy these local directories from the Windows machine:

- `backend/fixtures/mbr/hh`
- `backend/fixtures/mbr/ts`

Without them:

- fixture-driven Rust tests will not run as-is
- `import-local` smoke scenarios on real GG MBR examples will not be reproducible

## Useful commands on a new machine

```bash
cd backend
cargo test

export CHECK_MATE_DATABASE_URL="host=localhost port=5432 user=postgres password=checkmate_dev_postgres_2026 dbname=check_mate_dev"

cargo run -p parser_worker -- import-local "fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"
```

## Where to look first when resuming

- `CLAUDE.md`
- `docs/architecture/2026-03-23-mbr-hh-ts-notes.md`
- this file
- `backend/crates/tracker_parser_core/src/models.rs`
- `backend/crates/tracker_parser_core/src/normalizer.rs`
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs`
- `backend/crates/parser_worker/src/local_import.rs`
