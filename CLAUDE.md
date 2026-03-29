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
- `src/data/ftAnalyticsConfig.js` — статический UI-config для live FT dashboard (card rows, chart metadata, palettes)
- `src/services/ftDashboardApi.js` — реальный client для `GET /api/ft/dashboard`
- `src/services/ftDashboardState.js` — pure adapter `FT dashboard snapshot -> UI state`
- `src/data/ftAnalyticsMock.js` — legacy mock-only reference file, больше не используется FT страницей
- `src/services/uploadApi.js` — реальный upload/session/WebSocket client для `tracker_web_api`
- `src/services/uploadState.js` — pure adapter `bundle snapshot / event -> UI state`
- `backend/crates/tracker_web_api` — axum HTTP server для real upload/status slice
- `backend/crates/tracker_ingest_runner` — отдельный process-style runner поверх `tracker_ingest_runtime`
- `backend/fixtures/mbr/*` — реальный sample-pack GG MBR для новой parser/db ветки

`upload` и `FT analytics` уже подключены к реальному backend ingest/API flow. Dashboard overview / errors и будущий hand explorer пока остаются вне live derived integration.

## Code Conventions

- Язык UI: русский (навигация, подписи, placeholder-тексты)
- Компоненты: один файл = один default export, PascalCase
- Стили: только CSS variables из `:root` / `[data-theme="dark"]`; новые цвета — через переменные, не хардкод
- Layout: desktop — fixed sidebar (`--sidebar-width: 240px`) + topbar (`--topbar-height: 56px`); mobile — stacked со sticky topbar

## Gotchas

- Нет роутера — навигация через `activeSection` state в `App.jsx`. Новый раздел = добавить в `sections.js` + `sectionComponents` в `App.jsx`.
- `index.css` — монолит ~1500+ строк; стили для разных страниц живут в одном файле.
- `UploadHandsPage` больше не сидит на `mockHandUpload.js`: реальный vertical slice идёт через `uploadApi.js` + `uploadState.js` + `tracker_web_api` WebSocket snapshot/event contract.
- Vite dev server теперь проксирует `/api` и WebSocket upgrade на `http://127.0.0.1:3001`, поэтому local frontend/backend handoff ожидает поднятый `tracker_web_api`.
- Upload v1 intentionally does **not** support true server-side cancel; UI не должен обещать остановку backend processing.
- FT-раздел повторяет структуру `MBR_Stats`, но намеренно без player selector и aggregate-режима (student-only view); live backend contract идёт через `GET /api/ft/dashboard`, где `session = ingest bundle`, `buyin = buyin_total_cents`, а date-range фильтруется по `tournament local start`.
- FT frontend не пересчитывает MBR-метрики локально: `tracker_web_api` отдаёт page-specific snapshot, а `ftDashboardState.js` только адаптирует honest `ready / empty / partial / blocked` backend surface к текущему UI.
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
- `tracker_ingest_runtime` now owns the generic ingest orchestration layer:
  - DB-backed `ingest_bundles` + `ingest_bundle_files`;
  - `ingest_bundles.queue_order` is the explicit global queue contract for bundle scheduling, so cross-bundle `TS -> HH` file ordering no longer depends on timestamp or UUID accidents;
  - `import.import_jobs.depends_on_job_id` is now the explicit intra-bundle dependency contract for pair-aware execution, so HH member jobs are claimable only after successful TS completion instead of relying on `file_order_index`;
  - runnable `file_ingest` / `bundle_finalize` jobs in `import.import_jobs`;
  - attempt history in `import.job_attempts`;
  - claim/retry/status recomputation logic for `queued | running | succeeded | failed_retriable | failed_terminal` file jobs and `queued | running | finalizing | succeeded | partial_success | failed` bundles.
- Current local smoke import guarantees:
  - TS -> deduped `import.source_files`, synthetic `import.source_file_members`, `import.import_jobs`, `import.job_attempts`, `import.file_fragments`, `core.tournaments`, `core.tournament_entries`;
  - HH -> deduped `import.source_files`, synthetic `import.source_file_members`, `import.import_jobs`, `import.job_attempts`, `import.file_fragments`, `core.hands`, `core.hand_seats`, `core.hand_hole_cards`, `core.hand_actions`, `core.hand_boards`, `core.hand_showdowns`, `core.hand_pots`, `core.hand_pot_eligibility`, `core.hand_pot_contributions`, `core.hand_pot_winners`, `core.hand_returns`, `core.parse_issues`, `derived.hand_state_resolutions`, `derived.hand_eliminations`, `derived.mbr_stage_resolution`, `derived.mbr_tournament_ft_helper`;
  - post-import runtime refresh -> `analytics.player_hand_bool_features`, `analytics.player_hand_num_features`, `analytics.player_hand_enum_features` for the imported `player_profile_id` and current runtime version.
- `parser_worker import-local` is now a thin local shell over `tracker_ingest_runtime`: it enqueues a one-file local bundle, runs the local worker loop to terminal bundle status, executes exact TS/HH persistence inside file jobs, and runs one bundle-level scoped materialization refresh in finalize stage with per-stage timing captured in `stage_profile`.
- Current persistence behavior:
  - `backend/migrations/0004_exact_core_schema_v2.sql` now hardens the exact-core schema with `core.player_aliases`, `import.source_file_members`, `import.job_attempts`, `analytics.feature_catalog`, `analytics.stat_catalog`, `analytics.stat_dependencies`, and `analytics.materialization_policies`;
  - `backend/migrations/0021_ingest_runtime_runner.sql` adds the ingest runtime contract: `import.ingest_bundles`, `import.ingest_bundle_files`, richer job/attempt statuses, `job_kind`, bundle/file links, claim metadata, and finalize/file-job uniqueness guards;
  - `backend/migrations/0022_web_upload_member_ingest.sql` adds member-aware ingest bundle/file/event persistence for the real upload pipeline;
  - `backend/migrations/0023_file_fragments_member_uniqueness.sql` switches fragment uniqueness to `(source_file_member_id, fragment_index)` for member-safe upserts;
  - `backend/migrations/0024_user_timezone_and_gg_timestamp_contract.sql` adds `auth.users.timezone_name` and strict GG timestamp provenance values (`gg_user_timezone | gg_user_timezone_missing`);
  - `backend/migrations/0025_ingest_bundle_queue_order.sql` adds explicit ingest bundle queue order so multi-file and multi-bundle upload flows preserve stable bundle-level execution order;
  - `backend/seeds/0001_reference_data.sql` now seeds the minimal analytics catalog slice required by the current runtime materializer and seed stat query layer;
  - `core.hands` is upserted by `(player_profile_id, external_hand_id)`;
  - `import.source_files` is deduped by `(player_profile_id, room, file_kind, sha256)`, `import.file_fragments` by `(source_file_id, sha256)`, `import.source_file_members` by both `(source_file_id, member_index)` and `(source_file_id, sha256)`, and `import.job_attempts` by `(import_job_id, attempt_no)`;
  - flat local HH/TS imports now always materialize one synthetic `import.source_file_members` row so the member contract is exercised before ZIP/archive ingestion exists;
  - `parser_worker import-local` now requires explicit `--player-profile-id <uuid>`; production import context is derived from `core.player_profiles` and no longer auto-creates `Hero` / `Check Mate Dev Org`;
  - seat ownership is resolved only when the imported profile matches the GG seat by `screen_name` or one of its `core.player_aliases`; non-matching seats stay unowned instead of creating synthetic profiles;
  - `core.tournaments` and `core.hands` now persist `*_raw`, `*_local`, and `*_tz_provenance` alongside nullable canonical UTC timestamps;
  - for GG HH/TS imports, owner-user `auth.users.timezone_name` is the authoritative IANA timezone: canonical UTC is materialized with `tz_provenance = gg_user_timezone` when present, otherwise UTC stays `NULL` with `tz_provenance = gg_user_timezone_missing`;
  - `parser_worker set-user-timezone --user-id <uuid> --timezone <iana>` and `clear-user-timezone --user-id <uuid>` recompute historical GG `core.tournaments.started_at` / `core.hands.hand_started_at` for all profiles owned by that user and refresh runtime materializations in the same backend flow;
  - child canonical rows are replaced for the current hand before re-insert, so repeated local imports stay idempotent at the hand layer.
- Current normalized persistence behavior:
  - `normalize_hand` now runs through an internal replay ledger instead of relying on `collect` line order;
  - `tracker_parser_core::pot_resolution` now owns `pot construction -> pot settlement` as separate phases, instead of keeping reverse winner mapping inline inside `normalizer.rs`;
  - `normalize_hand` now exposes committed totals, exact final pot graph, explicit `pot_eligibilities`, return rows, resolved eliminations, and invariant results;
  - `NormalizedHand.actual` now splits observed and exact money surfaces explicitly: `observed_winner_collections` + `stacks_after_observed` are always the observed-layer projection, while `exact_selected_payout_totals` + `stacks_after_exact` materialize only when settlement is truly `Exact`; legacy names `winner_collections` / `stacks_after_actual` are no longer part of the contract;
  - deterministic settlement now uses summary non-winner markers, summary shown cards, river showdown ranks, single-collector fallback, and observed payout totals (`collect` first, otherwise summary `won/collected`) as evidence constraints; arbitrary reverse subset-search over candidate winners is no longer the primary mechanism;
  - odd-chip allocation is only attempted inside already-proven showdown ties; observed odd-chip payouts from either `collect` or summary can prove `exact`, unresolved odd-chip stays `uncertain`, and contradictory `collect` vs summary payout totals surface as `pot_settlement_collect_conflict:*` instead of silently downgrading to guessed winners;
  - hidden showdown / partial reveal gaps now stay `uncertain` through explicit `uncertain_reason_codes`, and guessed `core.hand_pot_winners` rows are intentionally never materialized for those hands;
  - `parser_worker import-local` persists the first exact derived row into `derived.hand_state_resolutions`;
  - persisted fields currently include `chip_conservation_ok`, `pot_conservation_ok`, parsed `rake_amount`, `final_stacks`, `invariant_errors`, and `uncertain_reason_codes`; until a future schema rename, `final_stacks` stores the observed-layer projection (`stacks_after_observed`), not a pseudo-exact alias; pot eligibility rows persist separately in `core.hand_pot_eligibility` via migration `0009_hand_pot_eligibility_and_uncertain_codes.sql`.
- Current MBR stage persistence behavior:
  - `derived.mbr_stage_resolution` now persists the exact `played_ft_hand` fact;
  - `ft_table_size` is persisted exactly for 9-max FT hands from the observed seat count;
  - boundary resolution no longer depends on `5-max`; importer now finds the last non-FT candidate set before the first exact FT hand on the ordered timeline;
  - `derived.mbr_stage_resolution` now also persists `boundary_resolution_state`, `boundary_candidate_count`, `boundary_resolution_method`, and `boundary_confidence_class`;
  - candidate rows still use `entered_boundary_zone = true`; a single candidate yields `boundary_resolution_state = exact`, while multiple equally-late candidates yield `boundary_resolution_state = uncertain`;
  - boundary KO values are no longer backfilled from a fake `0.0` share placeholder; `boundary_ko_*` stays `NULL` unless the boundary candidate is exact and an exact Hero share is actually proven for that hand;
  - current method code is `timeline_last_non_ft_candidate_v2`; this fixes the old `legacy_pre_ft_candidate_v1` heuristic at the per-hand layer;
  - `derived.mbr_tournament_ft_helper` is now materialized during `parser_worker import-local` with one row per `(tournament_id, player_profile_id)` for HH-covered tournaments;
  - the helper contract stabilizes tournament-grain `reached_ft_exact`, `first_ft_hand_id`, `first_ft_hand_started_local`, `first_ft_table_size`, `ft_started_incomplete`, `deepest_ft_size_reached`, `hero_ft_entry_stack_chips`, `hero_ft_entry_stack_bb`, `entered_boundary_zone`, and `boundary_resolution_state`;
  - earliest exact FT selection uses importer chronology over per-hand stage facts, and tournaments without an exact FT hand still get a helper row with FT-specific fields left `NULL` instead of guessed.
- Current tournament economics behavior:
  - `backend/migrations/0003_mbr_stage_economics.sql` introduces explicit `ref.mbr_buyin_configs`, `ref.mbr_regular_prizes`, and `ref.mbr_mystery_envelopes`;
  - `backend/seeds/0001_reference_data.sql` now seeds the currently listed GG Mystery Battle Royale buy-ins `$0.25`, `$1`, `$3`, `$10`, `$25` from the official public payouts tables;
  - `tracker_parser_core::parsers::tournament_summary` no longer depends on the first six lines staying fixed in place; it now resolves required TS lines by meaning and accepts harmless extra lines;
  - the result line remains the primary exact source for `finish_place` and `payout_cents`;
  - tail lines `You finished the tournament in ...` and `You received a total of ...` are now parsed as a structured confirmation layer on `TournamentSummary`;
  - result-vs-tail conflicts are not silently reconciled: they surface as warning parse issues (`ts_tail_finish_place_mismatch`, `ts_tail_total_received_mismatch`) while `core.tournament_entries` keeps the primary result-line values;
  - `parser_worker import-local` now resolves `regular_prize_money` from reference tables and materializes both `regular_prize_money` and `mystery_money_total` into `core.tournament_entries`;
  - `mystery_money_total` is computed as `total_payout_money - regular_prize_money` and negative remainders are rejected as import errors.
- Current Big KO foundation:
  - `backend/crates/mbr_stats_runtime/src/big_ko.rs` now provides a pure non-greedy decoder over `mystery_money_total`, exact Hero KO shares, and seeded mystery envelope tiers;
  - decoder output is typed as `Exact`, `Ambiguous`, `Infeasible`, or `ZeroMystery`;
  - decoder is deterministic and search-based, with no greedy fallback path.
- Current elimination persistence behavior:
  - `normalize_hand` now derives eliminations for players whose starting stack was positive and whose final stack after the hand is zero;
  - `HandElimination` is now `v2`: it stores `pots_participated_by_busted`, `pots_causing_bust`, `last_busting_pot_no`, `ko_winner_set`, `ko_share_fraction_by_winner`, `elimination_certainty_state`, and `ko_certainty_state`;
  - KO-credit is derived only from `last_busting_pot_no`, not from the union of all bust-relevant pots;
  - split KO shares are proportional to actual busting-pot `share_amount / pot_amount`;
  - elimination remains exact even when KO stays `uncertain` or `inconsistent`;
  - migration `backend/migrations/0020_hand_eliminations_v2.sql` adds the canonical v2 persisted surface to `derived.hand_eliminations`;
  - `parser_worker import-local` now persists only the canonical v2 elimination facts, and runtime derives split/sidepot/Hero-share behavior from those facts instead of legacy convenience flags.
- Current street-strength persistence behavior:
  - `tracker_parser_core` now exposes a pure `street_strength` evaluator over `CanonicalParsedHand`;
  - `parser_worker import-local` now persists exact `flop` / `turn` / `river` descriptors into `derived.street_hand_strength`;
  - current product direction intentionally does **not** add a structural preflop descriptor layer into `street_strength`; preflop filtering now lives separately through `derived.preflop_starting_hands` and an explicit starter-hand matrix contract;
  - `tracker_parser_core` now also exposes a pure preflop canonicalizer/evaluator in `preflop_starting_hands`, which materializes exact `starter_hand_class` rows for Hero and showdown-known opponents only;
  - `parser_worker import-local` now persists exact preflop starter rows into `derived.preflop_starting_hands` via migration `backend/migrations/0026_preflop_matrix_filters.sql`;
  - runtime materializes `starter_hand_class` and `certainty_state` into `analytics.player_street_enum_features` with `street = 'preflop'`;
  - `tracker_query_runtime` now supports enum whitelist membership through `FilterOperator::In` + `FilterValue::EnumList`, which is the canonical query shape for preflop matrix filters;
  - `backend/docs/preflop_matrix_contract.md` is now the canonical exact contract for the preflop starter-hand matrix layer and must be updated whenever its semantics change;
  - rows are materialized for Hero and for opponents whose hole cards are exact-known by showdown, and showdown-known opponents are backfilled across all reached streets;
  - the active unversioned persisted contract is `best_hand_class`, `best_hand_rank_value`, `made_hand_category`, `draw_category`, `overcards_count`, `has_air`, `missed_flush_draw`, `missed_straight_draw`, `is_nut_hand`, `is_nut_draw`, and `certainty_state`;
  - legacy `pair_strength`, independent draw bits, `has_overcards`, `has_missed_draw_by_river`, and `descriptor_version` are no longer part of the active runtime surface;
  - ordinary `draw_category` semantics are now improvement-aware: `gutshot` / `open_ended` / `double_gutshot` / `flush_draw` / `combo_draw` are derived only from legal unseen next cards that raise `best_hand_class`;
  - ordinary straight and flush draws count only exact `Straight` / exact `Flush` next-card improvements that still use at least one hole card in the resulting best hand; board-only completions and pure redraws to `StraightFlush` are intentionally excluded from ordinary draw categories;
  - `backdoor_flush_only` is a separate flop-only runner-runner flush-family descriptor and may include paths that finish as `StraightFlush`;
  - river missed draws are split into `missed_flush_draw` and `missed_straight_draw`, and are now pure historical flags: they are built only from the same improvement-aware ordinary draw history, ignore backdoor-only history until it promotes into an ordinary family, exclude redraw history, and are no longer suppressed by the final river made hand;
  - `is_nut_hand` is now an active exact postflop field under `STREET_HAND_STRENGTH_NUT_POLICY = hand_and_draw`: it is computed board-relatively from the player's hole cards plus all legal opponent hole-card combinations from the remaining deck, with shared nuts/chops counted as `true`;
  - known showdown hole cards of opponents are intentionally ignored as dead cards for `is_nut_hand`, so the flag stays stable независимо от reveal surface конкретной раздачи;
  - `is_nut_draw` is now an active exact postflop field under the same policy: it is derived only from ordinary improving next-card draw families, uses strict family-level nutness instead of one lucky out, treats `combo_draw` as `true` when at least one active ordinary family is nut, and materializes `Some(false)` for river / no-draw / backdoor-only rows.
  - the proof surface for `street_strength` is now three-layered: synthetic acceptance coverage (`tests/street_hand_strength.rs`), independent reference/differential coverage (`tests/street_strength_reference.rs`), and corpus-backed golden coverage (`tests/street_strength_corpus_golden.rs`);
  - the corpus-backed layer snapshots the full active row contract for `Hero` and showdown-known opponents in two formats: curated raw real-hand goldens and an aggregated full-pack coverage sweep over committed HH fixtures; refresh remains explicit via `UPDATE_GOLDENS=1`.
- Current real web slices:
- `tracker_web_api` is the active HTTP/WebSocket transport for v1 upload flow and exposes `GET /api/session`, `POST /api/ingest/bundles`, `GET /api/ingest/bundles/{bundle_id}`, and `GET /api/ingest/bundles/{bundle_id}/ws`;
- the same crate now also exposes `GET /api/ft/dashboard` as the page-specific MBR/FT snapshot endpoint over `mbr_stats_runtime`;
- upload accepts `.txt`, `.hh`, and `.zip`, spools raw files to local disk, and now uses a split contract:
- a single flat `.txt/.hh` upload stays on the legacy debug-friendly path, so TS-only bundles can still produce honest `partial` FT coverage states;
- `.zip` uploads and multipart batches with several files go through shared `tracker_ingest_prepare`, enqueue only valid HH+TS pairs, and persist reject diagnostics into `import.ingest_events` through the web upload path before runtime execution;
- pair-first web uploads now materialize into dependency-aware member jobs: TS is queued first, HH depends on that TS through `depends_on_job_id`, and terminal TS failure auto-fails the dependent HH with `dependency_failed`;
- `backend/crates/tracker_ingest_prepare` is now the shared phase-1 prepare-layer for directory/ZIP scan, quick `source_kind`/`tournament_id` sniffing, HH+TS pairing, duplicate collapse, and reject-report generation;
- the same prepare-layer now treats non-UTF8 files or ZIP members as ordinary `unsupported_source` rejects, so one bad artifact no longer aborts a whole directory import;
- `parser_worker` now exposes both `dir-import --prepare-only <path>` and `dir-import --player-profile-id <uuid> [--workers <n>] <path>`; the full dir-import path reuses `tracker_ingest_prepare`, materializes a synthetic pair-only archive, enqueues dependency-aware member jobs, drains them through the shared runner contract, and returns honest e2e telemetry (`rejected_by_reason`, `prep_elapsed_ms`, `runner_elapsed_ms`, `e2e_elapsed_ms`, `hands_per_minute_runner`, `hands_per_minute_e2e`, nested `e2e_profile`);
- `tracker_ingest_runner` is the separate runner process for this slice; it now supports `--workers <n>` plus `CHECK_MATE_INGEST_RUNNER_WORKERS`, defaults to `min(available_parallelism, 8)`, and parallelizes only the dependency-aware queue path;
- `parser_worker` remains the shared execution engine, resolves execution context from the job's `organization_id/player_profile_id` instead of hardcoded dev context, and now exposes `run_ingest_runner_parallel(...)` for the same dependency-aware queue contract;
- real-corpus `MIHA` profiling exposed a same-bundle parallel claim bottleneck: mutating `import.ingest_bundles` during `claim_next_job` serialized large multi-worker bundles on one row, so claim-time bundle/file event payloads now use a read-only derived snapshot and bundle status writes stay on actual status transitions instead of every claim;
- HH runtime profiling inside `parser_worker` is now split into `parse_ms`, `normalize_ms`, `derive_hand_local_ms`, `derive_tournament_ms`, `persist_db_ms`, `materialize_ms`, and `finalize_ms`; the legacy `stage_profile` is preserved as a compatibility aggregate where `persist_ms = derive_hand_local_ms + derive_tournament_ms + persist_db_ms`;
- the legacy flat `bulk_local_import` path stays serial and legacy-only: its file-first enqueue contract still lacks explicit HH-on-TS dependencies, so pair-first dev benchmarking and multi-worker verification should go through `dir-import`;
- `UploadHandsPage` is now wired to real backend snapshot/event flow;
- `FtAnalyticsPage` is now wired to real backend FT snapshot flow via `ftDashboardApi.js` + `ftDashboardState.js`, preserving the existing screen structure while rendering honest blocked/empty states from backend coverage.
- `backend/docs/street_strength_contract.md` is now the canonical exact contract for `tracker_parser_core::street_strength` and must be updated whenever its semantics change.
- Current canonical parser correction:
  - repeated GG `collected ... from pot` lines for the same player are now accumulated instead of overwritten;
  - this was required for exact multi-pot final stacks, pot conservation, and future side-pot/KO derivations.
- `backend/docs/exact_core_contract.md` is now the canonical exact-core contract for parser/normalizer/pot-resolution semantics and must be updated whenever those semantics change.
- Current wide-corpus triage contract:
  - `tracker_parser_core::wide_corpus_triage` is the canonical offline helper for broad HH/TS parser triage outside `parser_worker import-local`;
  - the helper always supports the committed sample at `backend/fixtures/mbr/quarantine_sample/{hh,ts}` and may additionally scan a local bulk root at `backend/.local/wide_corpus_quarantine/{hh,ts}` when present;
  - triage reports are allowlist-based: `allowed_issue_count` and `unexpected_issue_count` are split by stable `ParseIssueCode`, not by warning text;
  - syntax reporting is family-based, not raw-line-based: each entry carries `family_key`, `surface_kind`, `issue_code` or `parse_failure_kind`, `hit_count`, and representative `example_lines`;
  - `backend/scripts/run_wide_corpus_triage.sh` is the canonical offline runner, and `docs/WIDE_CORPUS_TRIAGE.md` plus `docs/COMMITTED_PACK_SYNTAX_CATALOG.md` document the workflow and known family catalog.
- Current pot-math property/mutation contract:
  - `tracker_parser_core::pot_resolution` now owns the lower pure pot-math property layer as unit tests, so internal pot construction can be stress-tested without widening the crate's public API;
  - the lower layer is split into a deterministic ordinary run and an ignored `10k+` stress run over generated contribution/status ladders;
  - `tracker_parser_core/tests/pot_math_properties.rs` is the upper full-normalizer smoke/mutation layer: it compares a canonical pot/outcome projection instead of the full `NormalizedHand` JSON;
  - only `collect` lines and summary seat-result lines are treated as order-insensitive safe mutations; action order remains an explicit negative boundary and is not expected to preserve the result;
  - the smoke layer also guards `uncalled return` chip/pot conservation and non-negative final stacks on canonical mutation fixtures.
- Current typed parse issue contract:
  - `CanonicalParsedHand` and `TournamentSummary` now expose parser-level `parse_issues: Vec<ParseIssue>` instead of legacy string surfaces like `parse_warnings` / `validation_issue_codes`;
  - `ParseIssue` is the single canonical family for HH, TS, and import-boundary issues: it carries typed `severity`, stable external `code`, human-readable `message`, optional `raw_line`, and optional structured `payload`;
  - malformed `Dealt to ...` surfaces now materialize as parser errors with code `malformed_dealt_to_line`, instead of being silently swallowed by the hidden-dealt fallback;
  - `NormalizedHand` intentionally no longer mirrors parser-layer issues; parser diagnostics stay at the parser boundary and downstream exact-core logic reads factual hand state instead;
  - `parser_worker` now persists `core.parse_issues` as a direct projection of that typed contract, including `payload`, rather than reconstructing rows from warning-string prefixes.
- Current unified settlement contract:
  - `NormalizedHand` no longer exports top-level `final_pots`, `pot_contributions`, `pot_eligibilities`, or `pot_winners`; the single canonical pot-resolution surface is `settlement`;
  - `settlement` carries hand-level `certainty_state`, typed `issues`, evidence facts (`collect_events_seen`, `summary_outcomes_seen`, `show_hands_seen`), and per-pot facts (`contributions`, `eligibilities`, `contenders`, `candidate_allocations`, `selected_allocation`, `issues`);
  - `SettlementIssue::ReplayStateInvalid` is now the canonical reason-coded fail-safe downgrade for impossible money mutations; once present, settlement must stay `inconsistent` and exact `selected_allocation` rows must not materialize;
  - `invariants` now persist typed `issues` instead of string arrays, and downstream sparse feature layers must canonicalize enum codes explicitly instead of parsing human-readable messages.
- Current phase0 exact-core proof pack:
  - `backend/fixtures/mbr/hh/GG20260325-phase0-exact-core-edge-matrix.txt` is the canonical 12-hand exact-core edge matrix for forced all-in, blinds/antes, dead blind, actor-order, uncalled-return, side-pot, odd-chip, and ambiguity regressions;
  - `tracker_parser_core/tests/phase0_exact_core_corpus.rs` now enforces manifest-style per-hand contracts over action stream facts (`seq`, `street`, `player_name`, `action_type`, `is_forced`, `is_all_in`, `all_in_reason`, `forced_all_in_preflop`) and normalization facts (`committed_total`, `returns`, projected pots/contributions/eligibilities, and typed invariant/settlement issue codes via manifest materializers);
  - new canonical rows `BRCM0404` and `BRCM0405` lock in `short BB forced all-in` and `dead blind + ante`; the 3-level side-pot ladder proof stays anchored on `BRSIDE1`;
  - odd-chip proof is now guarded at three levels: edge-matrix exactness for `BRCM0503`, summary-only exact settlement in `hand_normalization.rs`, and aggregate ambiguous odd-chip uncertainty without a guessed winner.
  - `backend/fixtures/mbr/hh_synthetic/GG20260328-p0-money-state-safety.txt` is the dedicated malformed money-surface pack for overcall / overbet / overraise / bad refund fail-safe regressions, and `tests/state_safety_regressions.rs` is its canonical acceptance gate.
- Current normalized-hand golden regression:
  - `tracker_parser_core/tests/normalized_hand_golden.rs` now snapshots the full serialized `NormalizedHand` output for the entire committed HH pack under `tracker_parser_core/tests/goldens/`;
  - goldens are stored per committed HH fixture file, not per hand;
  - ordinary test runs are read-only; refreshing goldens requires explicit `UPDATE_GOLDENS=1`.
- Current money-state safety contract:
  - shared helper `money_state` is the single guard layer for debit/refund replay mutations across parser all-in annotation, legality replay, and normalizer replay;
  - impossible debit now surfaces `action_amount_exceeds_stack`;
  - impossible refund now surfaces `refund_exceeds_committed` and/or `refund_exceeds_betting_round_contrib`;
  - `uncalled_return_actor_mismatch` / `uncalled_return_amount_mismatch` still describe surface invalidity, but they no longer permit state mutation afterward;
  - fail-safe hands may still expose observed payouts and return rows, but they must not claim exact settlement winners.
- Current terminal all-in snapshot semantics:
  - `tracker_parser_core::normalizer` now captures `snapshot` by resolved table state, not by a narrow `Call | Check` event gate;
  - capture happens only after the current action leaves at least two contestants in `Live | AllIn`, with at least one `AllIn`, and no pending `Live` actors left on the street;
  - terminal closure through final `Fold` is valid for snapshot capture, and a later `ReturnUncalled` in short-all-in branches does not retroactively erase that snapshot;
  - uncontested `bet/fold + return uncalled` remains snapshot-free.
- Current canonical summary-result persistence:
  - summary seat-result prose in `*** SUMMARY ***` is now parsed into dedicated `core.hand_summary_results` rows instead of being silently ignored or mixed with action rows;
  - summary rows are validated against `core.hand_seats(hand_id, seat_no)` rather than being remapped by player name;
  - malformed summary seat lines now surface as structured parse issues with code `unparsed_summary_seat_line`;
  - summary seat lines whose `Seat N / player / marker` head is valid but whose tail grammar is still unsupported now surface as `unparsed_summary_seat_tail` instead of collapsing into the coarse malformed-line warning;
  - summary tails `showed [...] and collected (...)` are now normalized into the existing structured `showed_won` surface instead of being dropped as parser gaps;
  - summary outcomes whose seat number conflicts with the canonical seat map surface as `summary_seat_outcome_seat_mismatch`;
  - summary outcomes that cannot attach to a seat row surface as `summary_seat_outcome_missing_seat`.
- Current canonical position persistence:
  - `tracker_parser_core::positions` now owns the pure active-seat position engine;
  - position facts are persisted into dedicated `core.hand_positions` rows, separate from `core.hand_actions` and `core.hand_seats`;
  - persisted rows carry machine-safe `position_index`, human-readable `position_label`, `preflop_act_order_index`, and `postflop_act_order_index`;
  - the canonical label table now covers `2..=10` active players, including `UTG+2` and `MP+1` for future 10-max support;
  - heads-up stays compact as `BTN` / `BB`; the seat posting the small blind in HU still persists as `position_label = BTN`, without a dedicated `BTN_SB` code.
- Current betting legality engine:
  - `tracker_parser_core::betting_rules` now validates the canonical action stream before pot resolution and feeds typed legality issues into `HandInvariants.issues`;
  - the legality layer covers heads-up preflop/postflop order, legal actor order, illegal checks/calls/raises, short-all-in non-reopen, full-raise reopen, and premature street close;
  - `ReturnUncalled` now validates that the refund goes back only to an actual over-contributor, and forced `PostSb` / `PostBb` actors are checked against the computed blind seats;
  - blindless `0/0(ante)` preflop hands now use a clockwise-after-button opener order instead of blind-based preflop indexes, preventing false legality errors on ante-only committed fixtures;
  - legality issues are persisted downstream through `derived.hand_state_resolutions.invariant_issues`, while exact/uncertain settlement evidence lives in the same row under `settlement_state + settlement`.
- Current hand-state resolution persistence:
  - `tracker_parser_core::EXACT_CORE_RESOLUTION_VERSION = gg_mbr_v2` is the active persisted exact-core resolution version;
  - `parser_worker import-local` now stores canonical settlement state in `derived.hand_state_resolutions` as `settlement_state`, full `settlement` JSON, and typed `invariant_issues` JSON;
  - `core.hand_pots`, `core.hand_pot_eligibility`, `core.hand_pot_contributions`, and `core.hand_pot_winners` remain SQL projections derived from `settlement`, not independent sources of truth;
  - migration `0019_unified_settlement_contract.sql` upgrades the DB contract to this unified shape and removes the old `invariant_errors` / `uncertain_reason_codes` columns.
- Current forced-all-in / sit-out surface:
  - canonical seat rows now carry `is_sitting_out`, and sit-out seats are excluded consistently from position derivation, legality order, and normalizer live-seat initialization;
  - action parser now materializes `PostDead` and player-line `Muck` instead of leaving them in unreachable enum-only state;
  - canonical action rows now carry `all_in_reason = voluntary | call_exhausted | raise_exhausted | blind_exhausted | ante_exhausted` plus `forced_all_in_preflop`;
  - canonical action money fields now use `amount` as the delta surface, while `to_amount` is reserved for `RaiseTo`; `Call` rows intentionally publish `to_amount = null` to avoid pseudo-target semantics;
  - `parse_canonical_hand` annotates blind/ante exhaustion even when the room omits literal `and is all-in`, so forced posts that zero the stack no longer rely on downstream inference alone;
  - importer persists `all_in_reason` and `forced_all_in_preflop` into `core.hand_actions`, via migration `0007_hand_action_all_in_metadata.sql`.
- Current reveal-surface policy:
  - partial reveal showdown lines still parse as `Show`, but they now emit explicit reason-coded warnings (`partial_reveal_show_line` / `partial_reveal_summary_show_surface`) instead of disappearing into generic fallback handling;
  - explicit no-show lines are kept as structured parser warnings with code `unsupported_no_show_line`;
  - committed GG fixture `43b06066: shows [5d] (a pair of Fives)` is now a documented allowed explicit warning, not an unexpected parser failure.
- Current stat runtime foundation:
  - `backend/crates/mbr_stats_runtime` now owns the first backend-only stat runtime slice;
  - `FEATURE_VERSION = mbr_runtime_v2`;
  - `GG_MBR_FT_MAX_PLAYERS = 9` — единая константа для FT detection; заменяет все magic `9` хардкоды в Rust-коде parser_worker и mbr_stats_runtime;
  - `mbr_stats_runtime` now exposes scoped runtime entrypoints `materialize_player_hand_features_for_tournaments(...)` and `materialize_player_hand_features_for_bundle(...)` as the canonical public materialization API;
  - `parser_worker import-local` now calls the runtime materializer inside the same PostgreSQL transaction after TS/HH persistence and refreshes analytics features only for tournaments affected by the finalized bundle instead of full-refreshing the whole `player_profile_id`;
  - runtime analytics writes now use chunked multi-values `INSERT`s for hand-grain and street-grain feature rows, so scoped finalize no longer pays row-by-row insert overhead;
  - `parser_worker import-local` now returns `stage_profile { parse_ms, normalize_ms, persist_ms, materialize_ms, finalize_ms }` in its JSON contract, and `bulk_local_import` now also reports `file_jobs`, `finalize_jobs`, `runner_elapsed_ms`, `hands_per_minute`, and aggregated `stage_profile`;
  - the runtime materializes dense per-hand features for every imported hand:
    - bool: `played_ft_hand`, `is_ft_hand`, `is_stage_2`, `is_stage_3_4`, `is_stage_4_5`, `is_stage_5_6`, `is_stage_6_9`, `is_boundary_hand`, `has_exact_ko_event`, `has_split_ko_event`, `has_sidepot_ko_event`;
    - num: `ft_table_size`, `ft_players_remaining_exact`, `hero_exact_ko_event_count`, `hero_split_ko_event_count`, `hero_sidepot_ko_event_count`;
    - enum: `ft_stage_bucket` with `not_ft`, `ft_7_9`, `ft_5_6`, `ft_3_4`, `ft_2_3`;
  - the same runtime now also materializes a separate street-grain analytics layer into:
    - `analytics.player_street_bool_features`;
    - `analytics.player_street_num_features`;
    - `analytics.player_street_enum_features`;
  - street-grain rows are keyed by `(organization_id, player_profile_id, hand_id, seat_no, street, feature_key, feature_version)` and currently cover:
    - bool: `has_air`, `missed_flush_draw`, `missed_straight_draw`;
    - num: `best_hand_rank_value`, `overcards_count`;
    - enum: `best_hand_class`, `made_hand_category`, `draw_category`, `certainty_state`;
  - street-grain runtime rows are materialized only for Hero and for showdown-known opponents with exact-known cards; guessed/unknown opponents do not get persisted analytics rows;
  - `tracker_query_runtime` now owns the generic typed hand/street query substrate over both hand-grain and street-grain features:
    - `hero_filters` evaluate on Hero rows;
    - `opponent_filters` require one showdown-known opponent seat to satisfy the full opponent group;
    - hand-grain predicates can be combined with street-grain predicates in the same filter set;
    - the query runtime reads sparse exact-core descriptors directly from `core/derived` without routing them through guessed analytics backfills:
      - hand-level presence keys `has_uncertain_reason_code:*`, `has_action_legality_issue:*`, `has_invariant_error_code:*` come from `derived.hand_state_resolutions`;
      - synthetic participant facet `street = seat` exposes seat-level exact facts from `core.hand_positions`, `core.hand_actions`, `core.hand_summary_results`, and `derived.hand_eliminations`;
      - the seat facet now publishes `position_label` as enum and `position_index`, `preflop_act_order_index`, `postflop_act_order_index` as numeric facts from `core.hand_positions`;
      - missing sparse exact-core presence facts evaluate as honest `false`, not as a fatal runtime filter error;
    - `is_nut_hand` / `is_nut_draw` are active supported predicates in the same contract;
    - the public internal request/result surface is `HandQueryRequest -> HandQueryResult { hand_ids }`, with stable `Uuid` ordering and strict hard errors for unsupported features / invalid comparisons;
  - `mbr_stats_runtime` no longer owns or re-exports generic query filters; it stays focused on MBR-specific materialization and stat query logic;
  - `mbr_stats_runtime::street_buckets` now exposes a runtime/UI-only projection `best | good | weak | trash` over exact street descriptors; this bucket layer is heuristic aggregation and is never written back into analytics tables or canonical exact tables;
  - `played_ft_hand` is materialized only from `derived.mbr_stage_resolution.played_ft_hand = true` with `played_ft_hand_state = exact`;
  - `derived.mbr_stage_resolution` now also persists the canonical hand-grain stage predicate surface:
    - `is_ft_hand`, `ft_players_remaining_exact`, `is_stage_2`, `is_stage_3_4`, `is_stage_4_5`, `is_stage_5_6`, `is_stage_6_9`, `is_boundary_hand`;
    - `played_ft_hand`, `entered_boundary_zone`, and `ft_table_size` remain compatibility/debug surfaces, but stage-aware logic must prefer the formal predicate fields;
  - KO event features are materialized only from `derived.hand_eliminations` rows where `ko_certainty_state = exact` and Hero is present in `ko_winner_set`;
  - split KO and sidepot KO are derived runtime-side from canonical facts:
    - split = `array_length(ko_winner_set) > 1`;
    - sidepot-based = `last_busting_pot_no > 1`;
  - KO money/event queries read Hero share from `ko_share_fraction_by_winner` JSON, not from legacy `hero_*` convenience columns.
  - current money-share contract is intentionally conservative: single-winner exact events may surface `exact_single_winner`, while split or uncertain cases remain blocked/null until later phases formalize KO-money semantics;
  - `docs/architecture/ko_split_bounty_rounding_policy.md` is the canonical split-bounty rounding reference for ugly-cent KO splits;
  - `mbr_stats_runtime::split_bounty::project_split_bounty_share` now maps split money projections into either `exact_integral` cents or a conservative `floor/ceil` candidate interval, and `mbr_stats_runtime::big_ko` uses that adapter to avoid false `Infeasible` outcomes on valid ugly-cent split cases;
  - this split-bounty adapter is still intentionally conservative: it does not prove a room-specific odd-cent rule, but the canonical stat runtime can now use its exact-share outputs together with official envelope frequencies for estimated KO-money stats;
  - the runtime query library now exposes the canonical query-time stat surface through `mbr_stats_runtime::query_canonical_stats(...)`, using `CanonicalStatSnapshot` / `CanonicalStatPoint` as the single stat-surface contract for the full catalog migration;
  - the currently mapped canonical query-time stats are:
    - base metrics: `roi_pct`, `avg_finish_place`, `final_table_reach_percent`;
    - TS/FT helper metrics: `avg_finish_place_ft`, `avg_finish_place_no_ft`, `avg_ft_initial_stack_chips`, `avg_ft_initial_stack_bb`, `incomplete_ft_percent`, `itm_percent`, `roi_on_ft_pct`, `winnings_from_itm`, `winnings_from_ko`, `ko_contribution`, `deep_ft_reach_percent`, `deep_ft_avg_stack_chips`, `deep_ft_avg_stack_bb`, `deep_ft_roi_pct`;
    - share-based KO / transition metrics: `total_ko`, `avg_ko_per_tournament`, `early_ft_ko`, `early_ft_ko_per_tournament`, `pre_ft_ko`, `pre_ft_attempts`, `pre_ft_chips`, `early_ft_bust_count`, `early_ft_bust_per_tournament`;
    - stage FT KO / money metrics: `ko_stage_2_3`, `ko_stage_2_3_money_total`, `ko_stage_3_4`, `ko_stage_3_4_money_total`, `ko_stage_4_5`, `ko_stage_4_5_money_total`, `ko_stage_5_6`, `ko_stage_5_6_money_total`, `ko_stage_6_9`, `ko_stage_6_9_money_total`, `ko_stage_7_9`, `ko_stage_7_9_money_total`;
    - FT conversion family: `ft_stack_conversion`, `avg_ko_attempts_per_ft`, `ft_ko_success_rate`, `ft_stack_conversion_7_9`, `ko_attempts_per_ft_7_9`, `ko_success_rate_7_9`, `ft_stack_conversion_5_6`, `ko_attempts_per_ft_5_6`, `ko_success_rate_5_6`, `ft_stack_conversion_3_4`, `ko_attempts_per_ft_3_4`, `ko_success_rate_3_4`;
    - KO-money / adjusted metrics: `ko_contribution_adj`, `ko_luck`, `roi_adj`, `big_ko_x1_5_count`, `big_ko_x2_count`, `big_ko_x10_count`, `big_ko_x100_count`, `big_ko_x1000_count`, `big_ko_x10000_count`;
  - buy-in filtering for query-time stats now resolves the allowed tournament set from `core.tournaments.buyin_total`, so HH-covered denominators no longer accidentally collapse to summary-covered tournaments when a tournament has HH but no TS row;
  - `avg_finish_place`-family metrics now exclude `NULL finish_place` from both numerator and denominator while still preserving the broader summary coverage counter;
  - the current attempt model is deterministic and legacy-compatible, but now materialized as a formal per-hand surface:
    - canonical raw source-of-truth lives in `derived.hand_ko_attempts` and `derived.hand_ko_opportunities`, written by `parser_worker::local_import` during hand import;
    - runtime/materializer no longer re-derives attempt logic from `core.hand_actions` or pot-eligibility joins and only aggregates the canonical derived rows;
    - `hero_ko_attempt_count` / `hero_ko_opportunity_count` are derived per hand, per opponent, across all streets;
    - attempt requires a real confrontation line, Hero coverage by starting stack, and Hero staying live in the credit pot; forced auto all-ins count when Hero remains in line;
    - transition `pre_ft_*` stats are not raw hand facts: they are tournament-level weighted projections over the single rush->FT transition hand and use the frozen short-handed coefficient table keyed by the first exact FT table size;
    - tournament-level mystery totals still come from TS payout decomposition, while event-level adjusted KO-money and `big_ko` use exact Hero KO shares plus official envelope frequencies.
  - `SeedStatSnapshot` still exists as a backward-compatible narrow projection, and always carries both `summary_tournament_count` and `hand_tournament_count` so callers can see the coverage basis explicitly.
- Current stat-layer handoff artifact:
  - `docs/stat_catalog/mbr_stats_inventory.yml` inventories all 31 legacy `MBR_Stats` modules with `status: mapped` and active P0/P1 blocker references from the 2026-03-25 audit;
  - `docs/stat_catalog/mbr_stats_spec_v1.yml` is now the frozen semantic contract for the MBR stat layer: formulas, denominator rules, exactness classes, and canonical migration targets live there;
  - `docs/architecture/ko_semantics_glossary.md` now freezes KO event, KO money, uncertainty, and boundary/stage terminology for the next implementation phases;
  - `docs/architecture/ko_split_bounty_rounding_policy.md` freezes the current ugly-cent split KO rounding adapter and its explicit non-goals before the later posterior decoder rebuild;
  - `docs/stat_catalog/mbr_stats_inventory.yml` remains the inventory map, but no longer serves as the semantic source of truth by itself;
  - `docs/mbr.md` and `docs/mbr.yml` contain the 2026-03-25 independent audit with phased remediation plan (F0–F4) and 7 identified problems (2×P0, 5×P1).
- Current spec parity gate:
  - `backend/crates/mbr_stats_runtime/tests/spec_parity.rs` enforces that frozen spec `mbr_stats_spec_v1.yml` and runtime `CANONICAL_STAT_KEYS` match exactly: 31 modules, 59 unique keys, zero missing/extra;
  - `models.rs` exports `CANONICAL_STAT_KEYS`, `EXPECTED_MODULE_COUNT`, and `EXPECTED_KEY_COUNT` as the authoritative runtime key enumeration;
  - `backend/scripts/run_backend_checks.sh` now includes an explicit spec parity gate step.
- CM-P0-02 carry-forward (`pre_ft_chips` contract):
  - public key renamed from `pre_ft_chipev` to `pre_ft_chips`;
  - runtime no longer reads legacy exact pre-FT snapshot deltas for this card and instead preserves the frozen legacy formula `sum(ft_entry_stack_chips over FT tournaments) / count(HH-covered tournaments) - 1000`.
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
- Formalized KO attempt model (F1-T3):
  - `load_stage_attempt_facts` now implements a formal attempt contract: target all-in + hero covers + hero in credit pot (MAX pot_no) + hero did NOT fold;
  - hero fold exclusion: `NOT EXISTS (hand_actions WHERE seat_no = hero AND action_type = 'fold')` — ensures hero is live until resolution;
  - credit pot requirement: hero must be eligible for `MAX(pot_no)` of target, not just any shared pot;
  - frozen spec `mbr_stats_spec_v1.yml` updated with formal attempt definition and fold/credit-pot source dependencies.
- Stable ordering substrate (F2-T2):
  - `core.hands.tournament_hand_order INT` column provides deterministic integer ordering within a tournament;
  - migration `0017_tournament_hand_order.sql` adds the column, index, and backfills existing data;
  - `parser_worker import-local` now computes `tournament_hand_order` after persisting all hands for a tournament;
  - all stat-critical SQL in `queries.rs` now uses `tournament_hand_order` instead of raw `hand_started_at_local` string comparison;
  - importer-side FT helper ordering now also consumes persisted `tournament_hand_order`, so downstream FT chronology no longer depends on raw local timestamp strings.
- Synthetic fixture suite (F3-T1):
  - 11 new unit tests added across `parser_worker`, `mbr_stats_runtime/registry`, and `mbr_stats_runtime/split_bounty`;
  - covers: no-FT tournament, single-candidate exact boundary, multi-seat-count stage predicates, incomplete FT start, deepest FT tracking, split bounty zero/full/half/ugly-cent edge cases, FT stage bucket exhaustive boundaries.
- Golden canonical snapshot gate (F3-T2):
  - `tests/canonical_snapshot_golden.rs` verifies full 59-key `CanonicalStatSnapshot` against golden JSON file;
  - supports `UPDATE_GOLDENS=1` env var for intentional golden updates;
  - diff-friendly output shows old vs new values per key on mismatch;
  - added to `backend/scripts/run_backend_checks.sh`.
- Extended corpus infrastructure (F3-T3):
  - `backend/scripts/generate_uncertainty_report.sh` queries committed corpus for parse issues, resolution states, boundary ambiguities, FT helper coverage, and hand order coverage;
  - generates `docs/runtime_uncertainty_report.md` with structured counts and known limitations.
- Current intentional limitation:
  - canonical UTC for GG is exact only when the owner user has an IANA timezone configured; without that setting, raw/local timestamps are still persisted but canonical UTC remains `NULL`;
  - date-range filters and session filters are still intentionally absent from the runtime query contract because timestamp normalization and session modeling are not exact yet;
  - street-strength exact descriptors are now materialized into runtime analytics rows, and the generic query runtime already supports filtering on `is_nut_hand` / `is_nut_draw`; HTTP/API transport over that runtime still remains a later phase;
  - the public `best | good | weak | trash` street buckets are heuristic runtime/UI helpers only and must not be treated as solver truth or persisted exact facts;
  - FT reach and KO averages are currently defined over tournaments with imported HH coverage, not summary-only tournaments;
  - `hero_exact_ko_event_count` remains a per-hand event-count proxy and must not be treated as KO money or as the public source for aggregate KO seed stats;
  - `ft_stage_bucket` remains an auxiliary/debug bucket and must not be used as the canonical substrate for stage-aware stat formulas;
  - boundary resolution, tournament-grain FT helper data, and formal hand-grain stage predicates are now persisted and fully consumed by the query-time canonical stat engine; stat values themselves are still never materialized into analytics tables;
  - `big_ko` is decoded in a pure runtime helper and surfaced only through query-time canonical stats; it is not materialized into analytics feature rows;
  - economics reference data currently covers the buy-ins listed on the official GG public payouts page; adding future buy-ins still requires explicit ref-table updates;
  - timezone-normalized timestamps for non-GG sources and the final stat-layer schema remain explicitly out of scope for the current phase.
- Cross-machine continuation:
  - committed handoff lives in `docs/architecture/2026-03-23-mbr-handoff.md`;
  - `docs/plans` and `docs/progress` are tracked workflow artifacts in this repo;
  - `.claude` remains intentionally local-only.

<!-- repo-task-proof-loop:start -->
## Repo task proof loop

For substantial features, refactors, and bug fixes, use the repo-task-proof-loop workflow.

Required artifact path:
- Keep all task artifacts in `.agent/tasks/<TASK_ID>/` inside this repository.

Required sequence:
1. Freeze `.agent/tasks/<TASK_ID>/spec.md` before implementation.
2. Implement against explicit acceptance criteria (`AC1`, `AC2`, ...).
3. Create `evidence.md`, `evidence.json`, and raw artifacts.
4. Run a fresh verification pass against the current codebase and rerun checks.
5. If verification is not `PASS`, write `problems.md`, apply the smallest safe fix, and reverify.

Hard rules:
- Do not claim completion unless every acceptance criterion is `PASS`.
- Verifiers judge current code and current command results, not prior chat claims.
- Fixers should make the smallest defensible diff.

Installed workflow agents:
- `.claude/agents/task-spec-freezer.md`
- `.claude/agents/task-builder.md`
- `.claude/agents/task-verifier.md`
- `.claude/agents/task-fixer.md`
<!-- repo-task-proof-loop:end -->
