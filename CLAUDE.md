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
- В локальной среде Codex на этом Mac уже доступны `cargo`, `rustc`, `psql` и отдельный Homebrew `postgresql@16`.
- Локальный project runtime использует выделенный кластер PostgreSQL 16 на `localhost:5433`, чтобы не конфликтовать с уже существующим системным `PostgreSQL 12` на `5432`.

## Backend Update (2026-03-23)

- `backend/` now contains a Rust workspace with:
  - `crates/tracker_parser_core` for GG MBR parsing;
  - `crates/parser_worker` as a CLI smoke-test wrapper over parser core.
- The first parser batch is intentionally narrow and fixture-driven:
  - detect source kind (`hh` / `ts`);
  - parse tournament summaries;
  - split raw hand-history files into hands;
  - parse hand headers for tournament identity, blind structure, table size, and button seat.
- Local PostgreSQL runtime is already usable:
  - dedicated Homebrew `postgresql@16` runs on `localhost:5433`;
  - database `check_mate_dev` exists inside that cluster;
  - migration `backend/migrations/0001_init_source_of_truth.sql` was applied successfully;
  - seed `backend/seeds/0001_reference_data.sql` was applied successfully.
- On this Mac, Docker is intentionally not required for the current parser foundation workflow.

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
  - TS -> `import.source_files`, `import.import_jobs`, `import.file_fragments`, `core.tournaments`, `core.tournament_entries`;
  - HH -> `import.source_files`, `import.import_jobs`, `import.file_fragments`, `core.hands`, `core.hand_seats`, `core.hand_hole_cards`, `core.hand_actions`, `core.hand_boards`, `core.hand_showdowns`, `core.hand_pots`, `core.hand_pot_contributions`, `core.hand_pot_winners`, `core.hand_returns`, `core.parse_issues`, `derived.hand_state_resolutions`, `derived.hand_eliminations`, `derived.mbr_stage_resolution`;
  - post-import runtime refresh -> `analytics.player_hand_bool_features`, `analytics.player_hand_num_features`, `analytics.player_hand_enum_features` for the current dev player and runtime version.
- Current persistence behavior:
  - `core.hands` is upserted by `(player_profile_id, external_hand_id)`;
  - child canonical rows are replaced for the current hand before re-insert, so repeated local imports stay idempotent at the hand layer.
- Current normalized persistence behavior:
  - `normalize_hand` now exposes committed totals, exact final pot graph, return rows, resolved eliminations, and invariant results;
  - `parser_worker import-local` persists the first exact derived row into `derived.hand_state_resolutions`;
  - persisted fields currently include `chip_conservation_ok`, `pot_conservation_ok`, parsed `rake_amount`, `final_stacks`, and `invariant_errors`.
- Current MBR stage persistence behavior:
  - `derived.mbr_stage_resolution` now persists the exact `played_ft_hand` fact;
  - `ft_table_size` is persisted exactly for 9-max FT hands from the observed seat count;
  - the last chronological `5-max` hand before the first chronological `9-max` hand is now persisted as the boundary candidate with `entered_boundary_zone = true` and `entered_boundary_zone_state = estimated`;
  - `entered_boundary_zone` and `boundary_ko_*` remain intentionally unresolved/uncertain beyond that candidate flag until the dedicated boundary resolver batch is implemented.
- Current elimination persistence behavior:
  - `normalize_hand` now derives exact eliminations for players whose starting stack was positive and whose final stack after the hand is zero;
  - `parser_worker import-local` now persists those rows into `derived.hand_eliminations`;
  - current persisted slice now includes `resolved_by_pot_no`, `hero_involved`, `hero_share_fraction`, `is_split_ko`, `split_n`, `is_sidepot_based`, and `certainty_state`;
  - `hero_involved = true` only when Hero receives a positive share of the pot that contains the eliminated player's last chips.
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
  - canonical local setup is `backend/scripts/bootstrap_backend_dev.sh`;
  - canonical backend verification is `backend/scripts/run_backend_checks.sh`;
  - GitHub Actions backend gate lives in `.github/workflows/backend-foundation.yml` and is intentionally backend-only.
- Current intentional limitation:
  - timestamps are still left `NULL` in DB import until GG MBR timezone handling is fixed exactly;
  - date-range filters and session filters are still intentionally absent from the runtime query contract because timestamp normalization and session modeling are not exact yet;
  - FT reach and KO averages are currently defined over tournaments with imported HH coverage, not summary-only tournaments;
  - boundary KO metrics and timezone-normalized timestamps are still not persisted yet;
  - `boundary_ko_ev`, `big_ko` redesign, and the new stat-layer schema remain explicitly out of scope for the current phase.
- Cross-machine continuation:
  - committed handoff lives in `docs/architecture/2026-03-23-mbr-handoff.md`;
  - `docs/plans` and `.claude` are intentionally local-only and must be copied manually if needed on another machine.
