# Preflop Matrix Contract

## Статус

Этот документ фиксирует текущий exact preflop contract для starter-hand matrix по состоянию на 2026-03-28.

Он является каноническим reference для:

- `backend/crates/tracker_parser_core/src/preflop_starting_hands.rs`
- `backend/crates/parser_worker/src/local_import.rs`
- `backend/crates/mbr_stats_runtime/src/materializer.rs`
- `backend/crates/tracker_query_runtime/src/filters.rs`

## Проблема

Проекту нужен exact preflop filter surface, но его нельзя смешивать с postflop `street_strength`: префлоп не является made-hand/draw descriptor layer.

## Цель

Дать детерминированную фильтрацию по конкретным клеткам стартовой матрицы рук без ввода эвристического "hand strength" classifier.

## Scope v1

Текущий контракт покрывает только:

- canonical `starter_hand_class` (`AA`, `AKs`, `AKo`, `QJo` и т.д.);
- `certainty_state`;
- materialization для Hero и showdown-known opponents с exact-known двумя hole cards;
- runtime/query support через `street = 'preflop'`.

В текущий v1 не входят:

- `pair / suited / offsuit` как отдельные поля;
- gap count;
- connectivity;
- broadway count;
- wheel potential;
- любые strength buckets.

## Derived contract

Persisted source-of-truth surface:

- table: `derived.preflop_starting_hands`
- grain: одна строка на `(hand_id, seat_no)`
- fields:
  - `starter_hand_class`
  - `certainty_state`

Материализация происходит только когда hole cards известны точно:

- Hero через `Dealt to Hero [.. ..]`;
- opponent только если обе карты exact-known по showdown surface.

Unknown или partial opponents в этот слой не попадают.

## Canonicalization rules

- pair materialize-ится как rank-pair код без suited suffix: `AA`, `77`, `22`;
- non-pair always rank-ordered from high to low: `AK`, `QJ`, `T9`;
- suited hand получает suffix `s`;
- offsuit hand получает suffix `o`.

Примеры:

- `Ah Ad` -> `AA`
- `Kd Ah` -> `AKo`
- `Kh Ah` -> `AKs`
- `Jh Qc` -> `QJo`

## Analytics/runtime surface

Runtime materializes the preflop contract into:

- `analytics.player_street_enum_features`
- `street = 'preflop'`
- `feature_key = 'starter_hand_class'`

`certainty_state` materialize-ится туда же как обычный street enum feature.

## Query contract

Canonical filter form:

- `FeatureRef::Street { street: "preflop", feature_key: "starter_hand_class" }`
- `FilterOperator::In`
- `FilterValue::EnumList([...])`

Семантика:

- whitelist не может быть пустым;
- `In` допустим только для enum surface;
- для opponent group preflop filter должен матчиться на том же seat, что и остальные street/seat filters этого opponent group.

## Relationship to street_strength

- `street_strength` остаётся строго postflop-only (`flop` / `turn` / `river`);
- preflop matrix contract живёт отдельно;
- никаких `preflop_draw` или `preflop_made_hand_category` не существует.
