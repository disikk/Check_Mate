# Street Strength Runtime Buckets Design

**Дата:** 2026-03-25

## Проблема

После `TASK-SH-001..009` точный postflop descriptor слой уже существует в `derived.street_hand_strength`, но он пока не подключен к рабочему слою признаков и фильтров.

Сейчас в репозитории есть только hand-grain runtime:

- `analytics.player_hand_bool_features`
- `analytics.player_hand_num_features`
- `analytics.player_hand_enum_features`

Эти таблицы и текущий `mbr_stats_runtime` считают признаки на всю раздачу целиком. Для `street_strength` этого недостаточно, потому что здесь важны:

- конкретная улица (`flop / turn / river`);
- конкретный участник раздачи, а не только раздача в целом;
- отдельные условия для Hero и для соперника;
- отдельный UI bucket layer, который не должен подменять exact-core.

Если пытаться “впихнуть” это в текущий hand-grain слой через названия вида `flop_made_hand_category`, `turn_draw_category`, то фильтры быстро станут грязными и недекларативными.

## Цель

Закрыть `TASK-SH-010..011` как следующий слой поверх уже готового `street_strength`:

1. добавить street-grain analytics substrate для точных признаков `street_strength`;
2. сделать общий каркас фильтров, который умеет работать и со старыми hand-grain признаками, и с новыми street-grain признаками;
3. ввести отдельные группы условий:
   - фильтры Hero;
   - фильтры соперника;
4. добавить per-street public bucket layer `best / good / weak / trash` как runtime/UI-проекцию поверх точных признаков, а не как persisted “истину”.

## Зафиксированные решения

## 1. Новый street-grain слой нужен отдельно

Для `street_strength` добавляется отдельный analytics слой:

- `analytics.player_street_bool_features`
- `analytics.player_street_num_features`
- `analytics.player_street_enum_features`

Он не заменяет существующий hand-grain runtime, а дополняет его.

### Почему это нужно

Для одной раздачи может существовать несколько relevant street rows:

- Hero flop / turn / river;
- showdown-known opponent flop / turn / river.

Следовательно, ключ должен различать не только раздачу, но и участника с улицей.

## 2. Ключ street-feature rows

Рекомендуемый grain:

- `organization_id`
- `player_profile_id` — владелец выборки, то есть чей кабинет/данные мы строим;
- `hand_id`
- `seat_no` — конкретный участник раздачи, для которого описывается street row;
- `street`
- `feature_key`
- `feature_version`

Это позволяет:

- отдельно фильтровать Hero и соперников;
- не дублировать схему отдельными “hero/opponent” таблицами;
- не превращать улицу в часть имени фичи.

## 3. Источник истины остается exact layer

Exact source of truth остается прежним:

- `derived.street_hand_strength`
- `core.hand_seats`

Новый analytics street-layer является materialized runtime substrate, а не новой canonical truth-table.

Это следует уже существующему принципу репозитория: materialized features — ускорение и удобный substrate, а не первичный источник истины.

## 4. Hero и opponent фильтры — это разные группы условий

В runtime/query контракте должны существовать две отдельные группы:

- `hero_filters`
- `opponent_filters`

### Семантика

- `hero_filters`: должны матчиться на street-row Hero.
- `opponent_filters`: должны матчиться хотя бы на одном сопернике, для которого hole cards exact-known.
- Итоговая раздача проходит только если выполняются обе группы, если обе заданы.

Для opponent группы не допускаются guessed или partially revealed rows.

## 5. Общий каркас фильтров строим сразу шире, но scope остается контролируемым

Пользователь выбрал не локальный ad-hoc фильтр только под `street_strength`, а общий каркас фильтров.

Поэтому в этом цикле:

- делаем общий filter substrate;
- через него прогоняем:
  - существующие hand-grain features;
  - новые street-grain features.

Но при этом мы **не** заходим в:

- полноценный stat engine из `TASK-CORE-012+`;
- web/API integration;
- frontend wiring beyond runtime/UI bucket contract.

## 6. Bucket layer отделен от exact-core

Per-street bucket layer `best / good / weak / trash`:

- считается только как runtime/UI projection;
- не materialize-ится как persisted exact feature row;
- не записывается обратно в `derived.street_hand_strength`;
- не объявляется solver truth.

### Contract

Bucket layer должен быть явно помечен как:

- heuristic/UI aggregation;
- not range-relative exact solver evaluation.

Это напрямую следует из аудита `docs/сила руки.md`.

## 7. Осторожная bucket policy

Пользователь выбрал осторожную трактовку bucket’ов.

Базовое направление:

- `best` — только очень сильные made hands;
- `good` — нормальные made hands и сильные active draws;
- `weak` — слабые пары и слабые draws;
- `trash` — air / почти пустые руки.

Точный mapping должен быть зафиксирован в spec freeze и покрыт тестами.

## 8. Nut fields остаются deferred и не притворяются рабочими

Так как `is_nut_hand` и `is_nut_draw` сейчас остаются unavailable/deferred, в этом цикле нельзя silently сделать вид, что по ним уже есть честные фильтры.

Допустимый подход:

- registry знает об этих полях как о deferred/unavailable;
- filter contract либо явно отклоняет такие predicates,
- либо маркирует их как unsupported in current phase.

Но нельзя:

- silently materialize `false`;
- silently игнорировать запрос;
- выдавать эти bucket/filters как exact-ready.

## Какие exact street-features входят в этот цикл

В новый street-grain runtime должны войти:

- `best_hand_class`
- `best_hand_rank_value`
- `made_hand_category`
- `draw_category`
- `overcards_count`
- `has_air`
- `missed_flush_draw`
- `missed_straight_draw`
- `certainty_state`

Nut fields входят только как deferred contract surface, а не как рабочие computed filters.

## Архитектурный план изменений

## 1. Схема

Добавляется новая migration, которая создает:

- `analytics.player_street_bool_features`
- `analytics.player_street_num_features`
- `analytics.player_street_enum_features`

и нужные индексы/PK на `player_profile_id + hand_id + seat_no + street + feature_key + feature_version`.

## 2. Runtime models / registry

`mbr_stats_runtime` расширяется:

- новыми street-grain model structs;
- расширенным registry, который описывает:
  - hand-grain features;
  - street-grain features;
- materializer-ом, который:
  - продолжает full-refresh hand-grain layer;
  - дополнительно full-refresh street-grain layer.

## 3. Query / filter substrate

Появляется общий filter/query слой, который умеет описывать:

- hand feature predicates;
- street feature predicates;
- Hero group;
- opponent group.

Этот слой нужен как runtime substrate, даже если на этом этапе он будет покрыт только smoke tests и локальными query helpers.

## 4. Bucket projection

Bucket layer реализуется отдельной runtime-функцией поверх exact street row:

- input: canonical exact street descriptor;
- output: `best | good | weak | trash` + explicit `heuristic` contract.

Persisted bucket rows в analytics не создаются.

## Тестовая стратегия

## 1. Schema / materialization

- RED tests на новые analytics street tables;
- RED tests на materializer, который должен писать Hero и showdown-known opponent rows;
- проверки, что guessed opponents не materialize-ятся.

## 2. Filter engine

- RED tests на общий filter substrate:
  - hand-grain predicates;
  - street-grain predicates;
  - hero/opponent group semantics.

## 3. Bucket mapping

- RED tests на conservative mapping:
  - strong made hand -> `best`;
  - normal made hand / strong draw -> `good`;
  - weak pair / weak draw -> `weak`;
  - air -> `trash`.

## 4. Operational checks

- docs + `CLAUDE.md` должны явно различать:
  - exact street features;
  - heuristic bucket layer.

## Риски

- Это уже не локальный patch в одном модуле, а маленький foundation-step в сторону feature/filter engine.
- Если слишком широко развернуть “общий filter engine”, цикл расползется в `TASK-CORE-012+`.
- Так как bucket layer heuristic, очень важно не дать ему утечь в persisted exact contract.
- Так как текущий репозиторий грязный по другим backend-работам, builder должен трогать только:
  - street analytics feature slice;
  - filter substrate slice;
  - связанные docs/runtime tests.

## Критерии готовности

- Есть отдельный street-grain analytics substrate.
- Exact street features materialize-ятся для Hero и showdown-known opponents.
- Есть общий filter substrate, который работает и с hand-grain, и с street-grain features.
- Hero/opponent filter groups работают раздельно и предсказуемо.
- Bucket layer считается отдельно как runtime heuristic projection и не смешивается с exact-core.
- `CLAUDE.md` и `RUNBOOK.md` отражают новую архитектуру.
