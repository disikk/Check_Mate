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
  - HH -> deduped `import.source_files`, synthetic `import.source_file_members`, `import.import_jobs`, `import.job_attempts`, `import.file_fragments`, `core.hands`, `core.hand_seats`, `core.hand_hole_cards`, `core.hand_actions`, `core.hand_boards`, `core.hand_showdowns`, `core.hand_pots`, `core.hand_pot_eligibility`, `core.hand_pot_contributions`, `core.hand_pot_winners`, `core.hand_returns`, `core.parse_issues`, `derived.hand_state_resolutions`, `derived.hand_eliminations`, `derived.mbr_stage_resolution`, `derived.mbr_tournament_ft_helper`;
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
  - `tracker_parser_core::pot_resolution` now owns `pot construction -> pot settlement` as separate phases, instead of keeping reverse winner mapping inline inside `normalizer.rs`;
  - `normalize_hand` now exposes committed totals, exact final pot graph, explicit `pot_eligibilities`, return rows, resolved eliminations, and invariant results;
  - deterministic settlement now uses summary non-winner markers, summary shown cards, river showdown ranks, single-collector fallback, and aggregate `collect` totals only as evidence constraints; arbitrary reverse subset-search over candidate winners is no longer the primary mechanism;
  - odd-chip allocation is only attempted inside already-proven showdown ties and is then reconciled against aggregate collected totals; exact contradictory `collect` distributions now surface as `pot_settlement_collect_conflict:*` instead of silently downgrading to guessed winners;
  - hidden showdown / partial reveal gaps now stay `uncertain` through explicit `uncertain_reason_codes`, and guessed `core.hand_pot_winners` rows are intentionally never materialized for those hands;
  - `parser_worker import-local` persists the first exact derived row into `derived.hand_state_resolutions`;
  - persisted fields currently include `chip_conservation_ok`, `pot_conservation_ok`, parsed `rake_amount`, `final_stacks`, `invariant_errors`, and `uncertain_reason_codes`; pot eligibility rows persist separately in `core.hand_pot_eligibility` via migration `0009_hand_pot_eligibility_and_uncertain_codes.sql`.
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
  - **KO-credit semantics (CM-P0-01 fix)**: `build_elimination()` now computes KO-credit attribution from the highest `pot_no` among bust-relevant pots (`ko_credit_pot_no`), not from the union of all pots; this matches the official GG MBR rule: bounty goes to winners of the last side pot containing the busted player's chips;
  - `HandElimination` now carries `ko_credit_pot_no: Option<u8>` — the authoritative pot for KO-credit; `resolved_by_pot_nos` remains as a diagnostic trace of all pots the busted player contributed to;
  - `hero_involved`, `hero_share_fraction`, `ko_involved_winners`, `joint_ko`, `split_n`, `is_split_ko`, and `ko_involved_winner_count` are now computed exclusively from credit pot winners;
  - migration `backend/migrations/0016_ko_credit_pot_no.sql` adds the `ko_credit_pot_no` column to `derived.hand_eliminations`;
  - `parser_worker import-local` now persists `ko_credit_pot_no` into `derived.hand_eliminations`;
  - when settlement is ambiguous or inconsistent, elimination rows still keep the busted seat context but intentionally omit guessed exact winner attribution details.
- Current street-strength persistence behavior:
  - `tracker_parser_core` now exposes a pure `street_strength` evaluator over `CanonicalParsedHand`;
  - `parser_worker import-local` now persists exact `flop` / `turn` / `river` descriptors into `derived.street_hand_strength`;
  - rows are materialized for Hero and for opponents whose hole cards are exact-known by showdown, and showdown-known opponents are backfilled across all reached streets;
  - the active unversioned persisted contract is `best_hand_class`, `best_hand_rank_value`, `made_hand_category`, `draw_category`, `overcards_count`, `has_air`, `missed_flush_draw`, `missed_straight_draw`, `is_nut_hand`, `is_nut_draw`, and `certainty_state`;
  - legacy `pair_strength`, independent draw bits, `has_overcards`, `has_missed_draw_by_river`, and `descriptor_version` are no longer part of the active runtime surface;
  - straight-draw semantics are player-specific only; board-only straight completions do not raise canonical draw categories;
  - river missed draws are split into `missed_flush_draw` and `missed_straight_draw`, and still ignore backdoor-only history or river `two_pair+` improvements;
  - `is_nut_hand` and `is_nut_draw` are explicitly deferred under `STREET_HAND_STRENGTH_NUT_POLICY = deferred`; `NULL` here means unavailable, not computed `false`.
- Current canonical parser correction:
  - repeated GG `collected ... from pot` lines for the same player are now accumulated instead of overwritten;
  - this was required for exact multi-pot final stacks, pot conservation, and future side-pot/KO derivations.
- `backend/docs/exact_core_contract.md` is now the canonical exact-core contract for parser/normalizer/pot-resolution semantics and must be updated whenever those semantics change.
- Current phase0 exact-core proof pack:
  - `backend/fixtures/mbr/hh/GG20260325-phase0-exact-core-edge-matrix.txt` is the canonical 12-hand exact-core edge matrix for forced all-in, blinds/antes, dead blind, actor-order, uncalled-return, side-pot, odd-chip, and ambiguity regressions;
  - `tracker_parser_core/tests/phase0_exact_core_corpus.rs` now enforces manifest-style per-hand contracts over action stream facts (`seq`, `street`, `player_name`, `action_type`, `is_forced`, `is_all_in`, `all_in_reason`, `forced_all_in_preflop`) and normalization facts (`committed_total`, `returns`, `final_pots`, `pot_contributions`, `pot_eligibilities`, invariant/uncertainty codes);
  - new canonical rows `BRCM0404` and `BRCM0405` lock in `short BB forced all-in` and `dead blind + ante`; the 3-level side-pot ladder proof stays anchored on `BRSIDE1`.
- Current normalized-hand golden regression:
  - `tracker_parser_core/tests/normalized_hand_golden.rs` now snapshots the full serialized `NormalizedHand` output for the entire committed HH pack under `tracker_parser_core/tests/goldens/`;
  - goldens are stored per committed HH fixture file, not per hand;
  - ordinary test runs are read-only; refreshing goldens requires explicit `UPDATE_GOLDENS=1`.
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
  - `tracker_parser_core::betting_rules` now validates the canonical action stream before pot resolution and feeds reason-coded legality issues into `NormalizationInvariants.invariant_errors`;
  - the legality layer covers heads-up preflop/postflop order, legal actor order, illegal checks/calls/raises, short-all-in non-reopen, full-raise reopen, and premature street close;
  - `ReturnUncalled` now validates that the refund goes back only to an actual over-contributor, and forced `PostSb` / `PostBb` actors are checked against the computed blind seats;
  - blindless `0/0(ante)` preflop hands now use a clockwise-after-button opener order instead of blind-based preflop indexes, preventing false legality errors on ante-only committed fixtures;
  - legality issues are persisted downstream through the existing hand-state resolution `invariant_errors` JSON, without inventing guessed exact facts or a parallel temporary schema.
- Current forced-all-in / sit-out surface:
  - canonical seat rows now carry `is_sitting_out`, and sit-out seats are excluded consistently from position derivation, legality order, and normalizer live-seat initialization;
  - action parser now materializes `PostDead` and player-line `Muck` instead of leaving them in unreachable enum-only state;
  - canonical action rows now carry `all_in_reason = voluntary | call_exhausted | raise_exhausted | blind_exhausted | ante_exhausted` plus `forced_all_in_preflop`;
  - `parse_canonical_hand` annotates blind/ante exhaustion even when the room omits literal `and is all-in`, so forced posts that zero the stack no longer rely on downstream inference alone;
  - importer persists `all_in_reason` and `forced_all_in_preflop` into `core.hand_actions`, via migration `0007_hand_action_all_in_metadata.sql`.
- Current reveal-surface policy:
  - partial reveal showdown lines still parse as `Show`, but they now emit explicit reason-coded warnings (`partial_reveal_show_line` / `partial_reveal_summary_show_surface`) instead of disappearing into generic fallback handling;
  - explicit no-show lines are kept as structured parser warnings with code `unsupported_no_show_line`;
  - committed GG fixture `43b06066: shows [5d] (a pair of Fives)` is now a documented allowed explicit warning, not an unexpected parser failure.
- Current stat runtime foundation:
  - `backend/crates/mbr_stats_runtime` now owns the first backend-only stat runtime slice;
  - `FEATURE_VERSION = mbr_runtime_v1`;
  - `GG_MBR_FT_MAX_PLAYERS = 9` — единая константа для FT detection; заменяет все magic `9` хардкоды в Rust-коде parser_worker и mbr_stats_runtime;
  - `parser_worker import-local` now calls the runtime materializer inside the same PostgreSQL transaction after TS/HH persistence and full-refreshes analytics features for the affected `player_profile_id`;
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
  - `mbr_stats_runtime::filters` now provides the first typed runtime filter substrate over both hand-grain and street-grain features:
    - `hero_filters` evaluate on Hero rows;
    - `opponent_filters` require one showdown-known opponent seat to satisfy the full opponent group;
    - hand-grain predicates can be combined with street-grain predicates in the same filter set;
    - runtime filters now also read sparse exact-core descriptors directly from `core/derived` without routing them through guessed analytics backfills:
      - hand-level presence keys `has_uncertain_reason_code:*`, `has_action_legality_issue:*`, `has_invariant_error_code:*` come from `derived.hand_state_resolutions`;
      - synthetic participant facet `street = seat` exposes seat-level exact facts from `core.hand_positions`, `core.hand_actions`, `core.hand_summary_results`, and `derived.hand_eliminations`;
      - the seat facet now publishes `position_label` as enum and `position_index`, `preflop_act_order_index`, `postflop_act_order_index` as numeric facts from `core.hand_positions`;
      - missing sparse exact-core presence facts evaluate as honest `false`, not as a fatal runtime filter error;
    - `is_nut_hand` / `is_nut_draw` remain honest `unsupported`, not silent `false`;
  - `mbr_stats_runtime::street_buckets` now exposes a runtime/UI-only projection `best | good | weak | trash` over exact street descriptors; this bucket layer is heuristic aggregation and is never written back into analytics tables or canonical exact tables;
  - `played_ft_hand` is materialized only from `derived.mbr_stage_resolution.played_ft_hand = true` with `played_ft_hand_state = exact`;
  - `derived.mbr_stage_resolution` now also persists the canonical hand-grain stage predicate surface:
    - `is_ft_hand`, `ft_players_remaining_exact`, `is_stage_2`, `is_stage_3_4`, `is_stage_4_5`, `is_stage_5_6`, `is_stage_6_9`, `is_boundary_hand`;
    - `played_ft_hand`, `entered_boundary_zone`, and `ft_table_size` remain compatibility/debug surfaces, but stage-aware logic must prefer the formal predicate fields;
  - KO event features are materialized only from `derived.hand_eliminations` rows where `hero_involved = true` and `certainty_state = exact`; split/sidepot subsets count eliminated players, not winner shares;
  - `derived.hand_eliminations` now also persists explicit KO money-share contract columns:
    - `ko_pot_resolution_type`;
    - `money_share_model_state`;
    - `money_share_exact_fraction`;
    - `money_share_estimated_min_fraction`, `money_share_estimated_ev_fraction`, `money_share_estimated_max_fraction`;
  - current money-share contract is intentionally conservative: single-winner exact events may surface `exact_single_winner`, while split or uncertain cases remain blocked/null until later phases formalize KO-money semantics;
  - `docs/architecture/ko_split_bounty_rounding_policy.md` is the canonical split-bounty rounding reference for ugly-cent KO splits;
  - `mbr_stats_runtime::split_bounty::project_split_bounty_share` now maps split money projections into either `exact_integral` cents or a conservative `floor/ceil` candidate interval, and `mbr_stats_runtime::big_ko` uses that adapter to avoid false `Infeasible` outcomes on valid ugly-cent split cases;
  - this split-bounty adapter is still intentionally conservative: it does not prove a room-specific odd-cent rule, but the canonical stat runtime can now use its exact-share outputs together with official envelope frequencies for estimated KO-money stats;
  - the runtime query library now exposes the canonical query-time stat surface through `mbr_stats_runtime::query_canonical_stats(...)`, using `CanonicalStatSnapshot` / `CanonicalStatPoint` as the single stat-surface contract for the full catalog migration;
  - the currently mapped canonical query-time stats are:
    - seed-safe base metrics: `roi_pct`, `avg_finish_place`, `final_table_reach_percent`, `total_ko_event_count`, `avg_ko_event_per_tournament`, `early_ft_ko_event_count`, `early_ft_ko_event_per_tournament`;
    - Phase A tournament / FT-helper / summary-money metrics: `avg_finish_place_ft`, `avg_finish_place_no_ft`, `avg_ft_initial_stack_chips`, `avg_ft_initial_stack_bb`, `incomplete_ft_percent`, `itm_percent`, `roi_on_ft_pct`, `winnings_from_itm`, `deep_ft_reach_percent`, `deep_ft_avg_stack_chips`, `deep_ft_avg_stack_bb`, `deep_ft_roi_pct`;
    - Phase B stage / conversion metrics: `early_ft_bust_count`, `early_ft_bust_per_tournament`, `ko_stage_2_3_event_count`, `ko_stage_2_3_attempts_per_tournament`, `ko_stage_3_4_event_count`, `ko_stage_3_4_attempts_per_tournament`, `ko_stage_4_5_event_count`, `ko_stage_4_5_attempts_per_tournament`, `ko_stage_5_6_event_count`, `ko_stage_5_6_attempts_per_tournament`, `ko_stage_6_9_event_count`, `ko_stage_7_9_event_count`, `ko_stage_7_9_attempts_per_tournament`, `pre_ft_ko_count`, `ft_stack_conversion`, `avg_ko_attempts_per_ft`, `ko_attempts_success_rate`, `ft_stack_conversion_7_9`, `ft_stack_conversion_7_9_attempts`, `ft_stack_conversion_5_6`, `ft_stack_conversion_5_6_attempts`, `ft_stack_conversion_3_4`, `ft_stack_conversion_3_4_attempts`;
    - Phase C query-time KO-money / adjusted metrics: `winnings_from_ko_total`, `ko_contribution_percent`, `ko_contribution_adjusted_percent`, `ko_luck_money_delta`, `roi_adj_pct`, `ko_stage_2_3_money_total`, `ko_stage_3_4_money_total`, `ko_stage_4_5_money_total`, `ko_stage_5_6_money_total`, `ko_stage_6_9_money_total`, `ko_stage_7_9_money_total`, `pre_ft_chipev`, `big_ko_x1_5_count`, `big_ko_x2_count`, `big_ko_x10_count`, `big_ko_x100_count`, `big_ko_x1000_count`, `big_ko_x10000_count`;
  - buy-in filtering for query-time stats now resolves the allowed tournament set from `core.tournaments.buyin_total`, so HH-covered denominators no longer accidentally collapse to summary-covered tournaments when a tournament has HH but no TS row;
  - `avg_finish_place`-family metrics now exclude `NULL finish_place` from both numerator and denominator while still preserving the broader summary coverage counter;
  - the current Phase B attempt model is query-time and exact-core-first:
    - an exact KO attempt is counted per `(hand_id, target_seat)` when the target has an explicit all-in action, Hero covers the target by starting stack, and Hero plus target still share at least one eligible pot;
    - the runtime derives attempts directly from `core.hand_actions`, `core.hand_pot_eligibility`, `core.hand_seats.starting_stack`, and formal stage predicates, without persisted attempt stats or legacy `players_count` shortcuts;
    - `pre_ft_ko_count` still excludes boundary-zone hands and keys off `derived.mbr_tournament_ft_helper.first_ft_hand_started_local`;
    - tournament-level realized mystery totals are exposed query-time from `core.tournament_entries.mystery_money_total` and `core.tournament_entries.total_payout_money`;
    - per-event and adjusted KO-money surfaces are also query-time now, but they are explicitly `estimated`: the runtime combines official `ref.mbr_mystery_envelopes.frequency_per_100m` weights with supported exact Hero KO-share events, and it never backfills money from raw event counts or persists stat values in the database;
    - the true room-specific posterior reconstruction problem still remains deferred; current Big KO and adjusted-money stats are official-frequency-weighted estimates, not posterior-conditioned reconstructions of realized tournament mystery totals;
  - `SeedStatSnapshot` still exists as a backward-compatible narrow projection, and always carries both `summary_tournament_count` and `hand_tournament_count` so callers can see the coverage basis explicitly.
- Current stat-layer handoff artifact:
  - `docs/stat_catalog/mbr_stats_inventory.yml` inventories all 31 legacy `MBR_Stats` modules with `status: mapped` and active P0/P1 blocker references from the 2026-03-25 audit;
  - `docs/stat_catalog/mbr_stats_spec_v1.yml` is now the frozen semantic contract for the MBR stat layer: formulas, denominator rules, exactness classes, and canonical migration targets live there;
  - `docs/architecture/ko_semantics_glossary.md` now freezes KO event, KO money, uncertainty, and boundary/stage terminology for the next implementation phases;
  - `docs/architecture/ko_split_bounty_rounding_policy.md` freezes the current ugly-cent split KO rounding adapter and its explicit non-goals before the later posterior decoder rebuild;
  - `docs/stat_catalog/mbr_stats_inventory.yml` remains the inventory map, but no longer serves as the semantic source of truth by itself;
  - `docs/mbr.md` and `docs/mbr.yml` contain the 2026-03-25 independent audit with phased remediation plan (F0–F4) and 7 identified problems (2×P0, 5×P1).
- Current spec parity gate:
  - `backend/crates/mbr_stats_runtime/tests/spec_parity.rs` enforces that frozen spec `mbr_stats_spec_v1.yml` and runtime `CANONICAL_STAT_KEYS` match exactly: 31 modules, 60 unique keys, zero missing/extra;
  - `models.rs` exports `CANONICAL_STAT_KEYS`, `EXPECTED_MODULE_COUNT`, and `EXPECTED_KEY_COUNT` as the authoritative runtime key enumeration;
  - `backend/scripts/run_backend_checks.sh` now includes an explicit spec parity gate step.
- CM-P0-02 fix (`pre_ft_chipev` bias bug):
  - `load_pre_ft_chip_facts` in `queries.rs` no longer uses `COALESCE(..., 1000)` fallback; tournaments without an exact pre-FT snapshot are now excluded from the denominator via `AND pre_ft_snapshot.hero_final_stack IS NOT NULL` instead of synthesizing a zero chip-delta;
  - this ensures `pre_ft_chipev` only averages over tournaments with a real pre-FT hand snapshot.
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
  - Rust-side import ordering (boundary candidates, FT helper) still uses string sort at import time, which is then captured as the integer order.
- Synthetic fixture suite (F3-T1):
  - 11 new unit tests added across `parser_worker`, `mbr_stats_runtime/registry`, and `mbr_stats_runtime/split_bounty`;
  - covers: no-FT tournament, single-candidate exact boundary, multi-seat-count stage predicates, incomplete FT start, deepest FT tracking, split bounty zero/full/half/ugly-cent edge cases, FT stage bucket exhaustive boundaries.
- Golden canonical snapshot gate (F3-T2):
  - `tests/canonical_snapshot_golden.rs` verifies full 60-key `CanonicalStatSnapshot` against golden JSON file;
  - supports `UPDATE_GOLDENS=1` env var for intentional golden updates;
  - diff-friendly output shows old vs new values per key on mismatch;
  - added to `backend/scripts/run_backend_checks.sh`.
- Extended corpus infrastructure (F3-T3):
  - `backend/scripts/generate_uncertainty_report.sh` queries committed corpus for parse issues, resolution states, boundary ambiguities, FT helper coverage, and hand order coverage;
  - generates `docs/runtime_uncertainty_report.md` with structured counts and known limitations.
- Current intentional limitation:
  - canonical UTC timestamps are still left `NULL` in DB import until GG MBR timezone handling is fixed exactly, even though raw/local/provenance fields are now persisted;
  - date-range filters and session filters are still intentionally absent from the runtime query contract because timestamp normalization and session modeling are not exact yet;
  - street-strength exact descriptors are now materialized into runtime analytics rows, but nut-policy fields still remain deferred and unsupported in filters;
  - the public `best | good | weak | trash` street buckets are heuristic runtime/UI helpers only and must not be treated as solver truth or persisted exact facts;
  - FT reach and KO averages are currently defined over tournaments with imported HH coverage, not summary-only tournaments;
  - `hero_exact_ko_event_count` remains a per-hand event-count proxy and must not be treated as KO money or as the public source for aggregate KO seed stats;
  - `ft_stage_bucket` remains an auxiliary/debug bucket and must not be used as the canonical substrate for stage-aware stat formulas;
  - boundary resolution, tournament-grain FT helper data, and formal hand-grain stage predicates are now persisted and fully consumed by the query-time canonical stat engine; stat values themselves are still never materialized into analytics tables;
  - `big_ko` is decoded in a pure runtime helper and surfaced only through query-time canonical stats; it is not materialized into analytics feature rows;
  - economics reference data currently covers the buy-ins listed on the official GG public payouts page; adding future buy-ins still requires explicit ref-table updates;
  - timezone-normalized timestamps and the final stat-layer schema remain explicitly out of scope for the current phase.
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
