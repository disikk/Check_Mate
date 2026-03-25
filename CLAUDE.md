# Check Mate

Unified product context: `Check_Mate` is one integrated system, not a set of separate tools.
It combines student cabinets, the future MBR Stats replacement, a replay-grade GG MBR parser/normalizer, and tracker/stat/filter capabilities on one canonical source-of-truth model.

Frontend-прототип student cabinet для покерной аналитики с backend foundation под новое MBR-ядро (ошибки, FT-метрики, загрузка hand history).

## Commands

```bash
npm install        # установить зависимости
npm run dev        # dev-сервер (Vite)
npm run build      # production-сборка в dist/
npm run preview    # превью production-сборки
```

Линтера и тестов пока нет.

## Stack

- React 19 + Vite 6
- Recharts для графиков
- Единый `src/index.css` — plain CSS, CSS variables, glassmorphism
- `backend/` — foundation-контур под PostgreSQL source of truth, raw MBR fixtures и будущий parser/import layer

## Architecture

Текущая UI-часть — section-based SPA без роутера. Активная страница хранится в state `App.jsx`, переключается через `Sidebar`.

Backend foundation живёт в `backend/` и на текущем этапе включает:

- `backend/migrations/0001_init_source_of_truth.sql` — стартовую SQL-схему `auth/org/import/core/derived/analytics`;
- `backend/seeds/0001_reference_data.sql` — reference seed для `gg/mbr`;
- `backend/fixtures/mbr/hh` и `backend/fixtures/mbr/ts` — реальные GG MBR sample fixtures;
- `docs/architecture/2026-03-23-mbr-hh-ts-notes.md` — зафиксированные parser edge cases по HH/TS.

### Key files

| Файл | Роль |
|------|------|
| `src/App.jsx` | Root: theme, activeSection, layout (sidebar + topbar + main) |
| `src/navigation/sections.js` | Single source of truth для навигации и topbar metadata |
| `src/index.css` | Все стили; темы через `[data-theme="dark"]` |

### Sections

`dashboard` · `ftAnalytics` · `upload` · `errors` (placeholder) · `settings` (placeholder)

### Data

- `src/data/mockData.js` — error summary & trends
- `src/data/ftAnalyticsMock.js` — FT stat-cards, filters, chart configs, derived datasets
- `src/services/mockHandUpload.js` — upload pipeline с callback-контрактом (onBatchStart → onBatchComplete)
- `backend/fixtures/mbr/*` — реальный sample-pack GG MBR для новой parser/db ветки

Все данные mock. Backend-интеграция задумана как замена источника событий без переделки UI state model.

## Code Conventions

- Язык UI: русский (навигация, подписи, placeholder-тексты)
- Компоненты: один файл = один default export, PascalCase
- Стили: только CSS variables из `:root` / `[data-theme="dark"]`; новые цвета — через переменные, не хардкод
- Layout: desktop — fixed sidebar (`--sidebar-width: 240px`) + topbar (`--topbar-height: 56px`); mobile — stacked со sticky topbar

## Gotchas

- Нет роутера — навигация через `activeSection` state в `App.jsx`. Новый раздел = добавить в `sections.js` + `sectionComponents` в `App.jsx`.
- `index.css` — монолит ~1500+ строк; стили для разных страниц живут в одном файле.
- Upload pipeline callback-контракт в `mockHandUpload.js` спроектирован под замену на реальный backend — сохранять сигнатуры callbacks при изменениях.
- FT-раздел повторяет структуру `MBR_Stats`, но намеренно без player selector и aggregate-режима (student-only view).
- Внутри `Check_Mate` backend-скоуп текущего цикла ограничен только `GG MBR`; Chico для этого проекта не реализуется.
- Канонический onboarding path теперь Docker-first: root `docker-compose.yml`, root `scripts/` и `Makefile`.
- Канонический локальный DB contract для repo-level setup: `CHECK_MATE_DATABASE_URL="host=localhost port=5432 user=postgres password=postgres dbname=check_mate_dev"`.
- Legacy Homebrew `postgresql@16` на `localhost:5433` может оставаться как maintainer fallback на этом Mac, но не считается основным first-run path.

## Backend Update (2026-03-24)

- `backend/` now contains a Rust workspace with:
  - `crates/tracker_parser_core` for GG MBR parsing;
  - `crates/parser_worker` as a CLI smoke-test wrapper over parser core.
- The first parser batch is intentionally narrow and fixture-driven:
  - detect source kind (`hh` / `ts`);
  - parse tournament summaries;
  - split raw hand-history files into hands;
  - parse hand headers for tournament identity, blind structure, table size, and button seat.
- Local reproducible runtime is now expected from the project root:
  - `cp .env.example .env`;
  - `bash scripts/db_up.sh`;
  - `bash scripts/db_bootstrap.sh`;
  - `bash scripts/backend_test.sh`.
- Docker-first onboarding is the canonical path for this repository.
- A local Homebrew PostgreSQL 16 cluster can still be used for maintainer-only debugging on this Mac, but it is secondary to the root Docker flow.

## Canonical Parsing Update (2026-03-23)

- `tracker_parser_core` is no longer just a header/summary extractor.
- It now has the first canonical GG MBR hand model for replay-grade normalization:
  - seats;
  - canonical action vocabulary;
  - final board runout;
  - summary total/rake/board fields;
  - hero hole cards;
  - showdown hands;
  - collected amounts;
  - parse warnings only for still-unsupported lines.
- Design reference for the canonical/normalized split is `D:\coding\poker-ev-tracker`:
  - canonical parsed hand/event vocabulary first;
  - deterministic event-replay normalizer second;
  - derived stats and filters only after exact normalized state exists.
- `parser_worker` now supports:
  - summary smoke output for a single fixture file;
  - `import-local` smoke path into PostgreSQL using `CHECK_MATE_DATABASE_URL`.
- Current local smoke import guarantees:
  - TS -> deduped `import.source_files`, synthetic `import.source_file_members`, `import.import_jobs`, `import.job_attempts`, `import.file_fragments`, `core.tournaments`, `core.tournament_entries`;
  - HH -> deduped `import.source_files`, synthetic `import.source_file_members`, `import.import_jobs`, `import.job_attempts`, `import.file_fragments`, `core.hands`, `core.hand_seats`, `core.hand_hole_cards`, `core.hand_actions`, `core.hand_boards`, `core.hand_showdowns`, `core.hand_pots`, `core.hand_pot_contributions`, `core.hand_pot_winners`, `core.hand_returns`, `core.parse_issues`, `derived.hand_state_resolutions`, `derived.hand_eliminations`, `derived.mbr_stage_resolution`;
  - post-import runtime refresh -> `analytics.player_hand_bool_features`, `analytics.player_hand_num_features`, `analytics.player_hand_enum_features` for the current dev player and runtime version.
- Current persistence behavior:
  - `backend/migrations/0004_exact_core_schema_v2.sql` now hardens the exact-core schema with `core.player_aliases`, `import.source_file_members`, `import.job_attempts`, `analytics.feature_catalog`, `analytics.stat_catalog`, `analytics.stat_dependencies`, and `analytics.materialization_policies`;
  - `backend/seeds/0001_reference_data.sql` now seeds the minimal analytics catalog slice required by the current runtime materializer and seed stat query layer;
  - `core.hands` is upserted by `(player_profile_id, external_hand_id)`;
  - `import.source_files` is deduped by `(player_profile_id, room, file_kind, sha256)`, `import.file_fragments` by `(source_file_id, sha256)`, `import.source_file_members` by both `(source_file_id, member_index)` and `(source_file_id, sha256)`, and `import.job_attempts` by `(import_job_id, attempt_no)`;
  - flat local HH/TS imports now always materialize one synthetic `import.source_file_members` row so the member contract is exercised before ZIP/archive ingestion exists;
  - dev bootstrap/import now ensures a primary `core.player_aliases` row for `Hero` and uses alias lookup when attaching the Hero seat to `core.hand_seats.player_profile_id`;
  - `core.tournaments` and `core.hands` now persist `*_raw`, `*_local`, and `*_tz_provenance` alongside nullable canonical UTC timestamps;
  - current GG MBR timestamp policy is conservative: HH/TS text timestamps are persisted as raw text plus parsed local naive timestamps with `tz_provenance = gg_text_without_timezone`, while canonical UTC fields remain `NULL` until an exact timezone source exists;
  - child canonical rows are replaced for the current hand before re-insert, so repeated local imports stay idempotent at the hand layer.
- Current normalized persistence behavior:
  - `normalize_hand` now runs through an internal replay ledger instead of relying on `collect` line order;
  - `normalize_hand` now exposes committed totals, exact final pot graph, return rows, resolved eliminations, and invariant results;
  - final pot winner allocation is reconstructed from pot eligibility plus aggregate winner totals, so aggregated or reordered `collect` lines on the committed GG corpus no longer force `collect_mapping_amount_mismatch`;
  - when multiple valid pot-winner mappings exist, the hand now stays `uncertain` and guessed `core.hand_pot_winners` rows are intentionally not materialized; only exact mappings persist winner rows;
  - unsatisfied winner mappings now surface through `invariant_errors` without inventing fallback winners;
  - `parser_worker import-local` persists the first exact derived row into `derived.hand_state_resolutions`;
  - persisted fields currently include `chip_conservation_ok`, `pot_conservation_ok`, parsed `rake_amount`, `final_stacks`, and `invariant_errors`.
- Current MBR stage persistence behavior:
  - `derived.mbr_stage_resolution` now persists the exact `played_ft_hand` fact;
  - `ft_table_size` is persisted exactly for 9-max FT hands from the observed seat count;
  - the last chronological `5-max` hand before the first chronological `9-max` hand is now persisted as the boundary candidate with `entered_boundary_zone = true` and `entered_boundary_zone_state = estimated`;
  - boundary v1 now materializes `boundary_ko_min = boundary_ko_ev = boundary_ko_max` from the exact Hero KO share observed on that candidate hand;
  - boundary v1 persists `boundary_ko_method = legacy_pre_ft_candidate_v1`, `boundary_ko_certainty = estimated`, `boundary_ko_state = estimated`;
  - non-candidate hands intentionally keep `boundary_ko_* = NULL` with `boundary_ko_state = uncertain`.
- Current tournament economics behavior:
  - `backend/migrations/0003_mbr_stage_economics.sql` introduces explicit `ref.mbr_buyin_configs`, `ref.mbr_regular_prizes`, and `ref.mbr_mystery_envelopes`;
  - `backend/seeds/0001_reference_data.sql` now seeds the currently listed GG Mystery Battle Royale buy-ins `$0.25`, `$1`, `$3`, `$10`, `$25` from the official public payouts tables;
  - `parser_worker import-local` now resolves `regular_prize_money` from reference tables and materializes both `regular_prize_money` and `mystery_money_total` into `core.tournament_entries`;
  - `mystery_money_total` is computed as `total_payout_money - regular_prize_money` and negative remainders are rejected as import errors.
- Current Big KO foundation:
  - `backend/crates/mbr_stats_runtime/src/big_ko.rs` now provides a pure non-greedy decoder over `mystery_money_total`, exact Hero KO shares, and seeded mystery envelope tiers;
  - decoder output is typed as `Exact`, `Ambiguous`, `Infeasible`, or `ZeroMystery`;
  - decoder is deterministic and search-based, with no greedy fallback path.
- Current elimination persistence behavior:
  - `normalize_hand` now derives exact eliminations for players whose starting stack was positive and whose final stack after the hand is zero;
  - `parser_worker import-local` now persists those rows into `derived.hand_eliminations`;
  - current persisted slice now includes `resolved_by_pot_no`, `hero_involved`, `hero_share_fraction`, `is_split_ko`, `split_n`, `is_sidepot_based`, and `certainty_state`;
  - when winner mapping is ambiguous or unsatisfied, elimination rows still keep the busted seat/pot context but intentionally omit guessed winner attribution details;
  - `hero_involved = true` only when Hero receives a positive share of the pot that contains the eliminated player's last chips.
- Current street-strength persistence behavior:
  - `tracker_parser_core` now exposes a pure `street_strength` evaluator over `CanonicalParsedHand`;
  - `parser_worker import-local` now persists exact `flop` / `turn` / `river` descriptors into `derived.street_hand_strength`;
  - v1 materializes rows for Hero and for opponents whose hole cards are exact-known by showdown, and showdown-known opponents are backfilled across all reached streets;
  - v1 persists `best_hand_class`, `best_hand_rank_value`, `pair_strength`, draw flags, `has_overcards`, `has_air`, and `has_missed_draw_by_river` under `descriptor_version = gg_mbr_street_strength_v1`;
  - `is_nut_hand` and `is_nut_draw` remain intentionally `NULL` until a dedicated nut-policy batch is specified.
- Current canonical parser correction:
  - repeated GG `collected ... from pot` lines for the same player are now accumulated instead of overwritten;
  - this was required for exact multi-pot final stacks, pot conservation, and future side-pot/KO derivations.
- Current stat runtime foundation:
  - `backend/crates/mbr_stats_runtime` now owns the first backend-only stat runtime slice;
  - `FEATURE_VERSION = mbr_runtime_v1`;
  - `parser_worker import-local` now calls the runtime materializer inside the same PostgreSQL transaction after TS/HH persistence and full-refreshes analytics features for the affected `player_profile_id`;
  - the runtime materializes dense per-hand features for every imported hand:
    - bool: `played_ft_hand`, `has_exact_ko`, `has_split_ko`, `has_sidepot_ko`;
    - num: `ft_table_size`, `hero_exact_ko_count`, `hero_split_ko_count`, `hero_sidepot_ko_count`;
    - enum: `ft_stage_bucket` with `not_ft`, `ft_7_9`, `ft_5_6`, `ft_3_4`, `ft_2_3`;
  - `played_ft_hand` is materialized only from `derived.mbr_stage_resolution.played_ft_hand = true` with `played_ft_hand_state = exact`;
  - KO features are materialized only from `derived.hand_eliminations` rows where `hero_involved = true` and `certainty_state = exact`; split/sidepot subsets count eliminated players, not winner shares;
  - the runtime query library currently exposes only seed exact-safe aggregates:
    - `roi_pct`, `avg_finish_place` from tournament-summary coverage;
    - `final_table_reach_percent`, `total_ko`, `avg_ko_per_tournament` from hand-covered tournament coverage;
    - `SeedStatSnapshot` always carries both `summary_tournament_count` and `hand_tournament_count` so callers can see the coverage basis explicitly.
- Current stat-layer handoff artifact:
  - `docs/stat_catalog/mbr_stats_inventory.yml` inventories all 31 legacy `MBR_Stats` modules as a dependency map for future stat-layer redesign;
  - this file is inventory-only and intentionally does not yet introduce the new stat taxonomy or renamed stat families.
- Current reproducibility gate:
  - `backend/fixtures/mbr/hh` and `backend/fixtures/mbr/ts` are now committed sanitized golden fixtures, not local-only artifacts;
  - `tracker_parser_core` now contains a full-pack HH/TS sweep over the committed `9 HH + 9 TS` GG corpus;
  - the committed syntax surface is now explicitly documented in `docs/COMMITTED_PACK_SYNTAX_CATALOG.md`;
  - parser-worker now persists structured parse issues with `severity = warning|error` instead of a warning-only fallback;
  - canonical repo-level setup is `bash scripts/db_up.sh` + `bash scripts/db_bootstrap.sh`;
  - `bash scripts/db_bootstrap.sh` now also re-syncs the PostgreSQL role password to the current `.env` contract so reused Docker volumes do not keep stale auth state;
  - canonical repo-level backend verification is `bash scripts/backend_test.sh`;
  - `backend/scripts/bootstrap_backend_dev.sh` and `backend/scripts/run_backend_checks.sh` remain backend-focused helper gates;
  - backend checks now include an ignored PostgreSQL full-pack import smoke for zero parse issues, zero invariant mismatches, and idempotent hand-child persistence on that committed corpus;
  - GitHub Actions backend gate lives in `.github/workflows/backend-foundation.yml` and is intentionally backend-only.
- Current intentional limitation:
  - canonical UTC timestamps are still left `NULL` in DB import until GG MBR timezone handling is fixed exactly, even though raw/local/provenance fields are now persisted;
  - date-range filters and session filters are still intentionally absent from the runtime query contract because timestamp normalization and session modeling are not exact yet;
  - street-strength v1 intentionally stops at exact descriptors in `derived.street_hand_strength` and does not yet materialize runtime features or nut-policy fields;
  - FT reach and KO averages are currently defined over tournaments with imported HH coverage, not summary-only tournaments;
  - boundary KO persistence is currently only boundary v1 point-estimate materialization, not a full uncertainty/range model;
  - `big_ko` is decoded in a pure runtime helper but is not yet materialized into analytics feature rows or final stat cards;
  - economics reference data currently covers the buy-ins listed on the official GG public payouts page; adding future buy-ins still requires explicit ref-table updates;
  - timezone-normalized timestamps and the final stat-layer schema remain explicitly out of scope for the current phase.
- Cross-machine continuation:
  - committed handoff lives in `docs/architecture/2026-03-23-mbr-handoff.md`;
  - `docs/plans` and `docs/progress` are tracked workflow artifacts in this repo;
  - `.claude` remains intentionally local-only.
