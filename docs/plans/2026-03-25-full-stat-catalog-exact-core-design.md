# Full Stat Catalog Exact-Core Design

**Дата:** 2026-03-25

## Проблема

Старый проект [MBR_Stats](/Users/cyberjam/Documents/coding/MBR_Stats) уже содержит полный legacy-каталог MBR stat modules, но новый backend в `Check_Mate` пока мигрировал только узкий seed-safe срез:

- `roi_pct`
- `avg_finish_place`
- `final_table_reach_percent`
- `total_ko_event_count`
- `avg_ko_event_per_tournament`
- `early_ft_ko_event_count`
- `early_ft_ko_event_per_tournament`

Все остальные legacy-модули всё ещё висят в `docs/stat_catalog/mbr_stats_spec_v1.yml` как `blocked` или `legacy_only`. Это означает, что новый exact-core уже существует, но полноценного stat engine поверх него пока нет.

Пользовательский scope для этого цикла:

- перевести **весь stat-каталог** со старых рельс на новый backend;
- оставить **только canonical new keys**;
- не делать API/UI;
- **не materialize-ить stat values в БД**;
- считать stat values **только по запросу**;
- старый `MBR_Stats` использовать как semantic reference, а не как источник истины для новых формул.

## Цель

Построить один query-time stat engine в `backend/crates/mbr_stats_runtime`, который покрывает весь legacy-каталог из `MBR_Stats/stats`, но считает его через новый exact-core `Check_Mate` и отдаёт только canonical stat keys из `docs/stat_catalog/mbr_stats_spec_v1.yml`.

## Зафиксированные ограничения

- Только backend migration, без web/API/UI слоя.
- Только canonical new keys, без compatibility-слоя старых имён.
- Значения stat values не пишутся в БД ни в какой materialized stat table.
- `analytics.stat_catalog`, `analytics.stat_dependencies` и документация остаются метаданными контракта, а не persisted cache значений.
- Каждая новая формула проходит через TDD: `RED -> GREEN -> docs/progress sync -> full verification`.

## Выбранный подход

Выбран **dependency-first query-time migration**.

Что это значит:

- идём не по старым файлам один за другим, а по зависимостям между группами stat-ов;
- строим один новый query-time kernel для canonical stat keys;
- старый `MBR_Stats` используем как reference по названию, intent и общему shape метрики;
- если старая формула конфликтует с новым exact-core, выигрывает новый canonical contract;
- для тяжёлых модулей не переносим старые shortcut-эвристики автоматически, а достраиваем новые exact-core-first правила.

Что не делаем:

- не строим отдельный stat API;
- не подключаем frontend;
- не сохраняем готовые stat values в БД;
- не держим одновременно старые и новые ключи.

## Архитектурные решения

## 1. Один query-time источник истины

Новый stat engine считается только в runtime по запросу.

Нужны два слоя:

- typed filter/coverage contract;
- stat computation kernel, который возвращает canonical values + exactness/coverage/nullability state.

`query_seed_stats(...)` остаётся либо как thin wrapper, либо как subset-view поверх нового общего engine, но не как отдельный параллельный источник формул.

## 2. Один semantic contract на stat

У каждого canonical stat key должен быть один источник смысла:

- frozen formula в `docs/stat_catalog/mbr_stats_spec_v1.yml`;
- один query-time implementation path в `mbr_stats_runtime`;
- одно правило `value | NULL | blocked`.

Нельзя держать две разные реализации одной формулы для “быстрого” и “полного” режима.

## 3. Legacy reference без legacy shortcuts

Старый `MBR_Stats` нужен как reference для:

- набора модулей;
- структуры карточек и названий пользовательского слоя;
- общего business intent метрики.

Но старые shortcut-алгоритмы не переносятся автоматически, если audit/spec уже признали их unsafe:

- greedy Big KO decomposition;
- proxy KO-money rules;
- players_count shortcuts вместо formal stage predicates;
- legacy reached-final-table flags вместо exact FT helper.

## 4. Query-time only не запрещает exact-core helper facts

Запрет касается только persisted stat values.

Если какой-то canonical stat честно требует нового exact-core helper layer, допустимо добавить helper facts в `derived`, но:

- helper rows должны быть domain facts, а не готовые stat values;
- stat остаётся query-time aggregation поверх этих fact rows.

## 5. Поэтапное снятие `blocked`-статусов

В этом цикле задача считается завершённой только если весь legacy stat catalog переведён на canonical query-time engine.

Это означает:

- `blocked` в spec нельзя оставлять “по привычке”;
- если модуль реально требует нового money/posterior contract, этот contract входит в scope текущего большого цикла;
- `legacy_only` допустим только там, где это действительно runtime/UI helper, но не для canonical public stat key.

## Dependency order

## Фаза A. Tournament / FT-helper / summary-money layer

Сначала закрываем stat-ы, которые опираются на tournament summary и FT helper:

- `avg_finish_place_ft`
- `avg_finish_place_no_ft`
- `avg_ft_initial_stack_chips`
- `avg_ft_initial_stack_bb`
- `incomplete_ft_percent`
- `itm_percent`
- `roi_on_ft_pct`
- `deep_ft_reach_percent`
- `deep_ft_avg_stack_chips`
- `deep_ft_avg_stack_bb`
- `deep_ft_roi_pct`
- `winnings_from_itm_percent`

Почему это первая фаза:

- у неё уже есть summary import;
- есть `derived.mbr_tournament_ft_helper`;
- нет зависимости от KO-money posterior;
- она даёт большую часть безопасных FT/stat-card основ.

## Фаза B. Stage / KO-event / conversion layer

После этого закрываем stage-aware event stats:

- `early_ft_bust_count`
- `early_ft_bust_per_tournament`
- `ko_stage_2_3_event_count`
- `ko_stage_3_4_event_count`
- `ko_stage_4_5_event_count`
- `ko_stage_5_6_event_count`
- `ko_stage_6_9_event_count`
- `ko_stage_7_9_event_count`
- `pre_ft_ko_count`
- `ft_stack_conversion`
- `avg_ko_attempts_per_ft`
- `ko_attempts_success_rate`
- `ft_stack_conversion_7_9`
- `ft_stack_conversion_7_9_attempts`
- `ft_stack_conversion_5_6`
- `ft_stack_conversion_5_6_attempts`
- `ft_stack_conversion_3_4`
- `ft_stack_conversion_3_4_attempts`

Почему это вторая фаза:

- она требует formal stage predicates и elimination facts;
- часть модулей требует новой exact “KO attempt” модели;
- лучше строить её уже поверх доказанных FT helper / stage foundations.

## Фаза C. KO-money / posterior / adjusted-money layer

Только после event-layer закрываем money-driven stats:

- `winnings_from_ko_percent`
- `ko_contribution_percent`
- `ko_contribution_adjusted_percent`
- `ko_luck_money_delta`
- `roi_adj_pct`
- `big_ko_x1_5_count`
- `big_ko_x2_count`
- `big_ko_x10_count`
- `big_ko_x100_count`
- `big_ko_x1000_count`
- `big_ko_x10000_count`
- `pre_ft_chipev`

Почему это последняя фаза:

- эти модули зависят от KO-money semantics, а не просто от KO events;
- им нужна честная money-share / posterior model, а не legacy greedy decomposition;
- здесь самый высокий риск подменить domain truth старой эвристикой.

## Тестовая стратегия

Каждая фаза живёт по одному циклу:

1. Заморозить canonical formula в spec и metadata.
2. Написать focused failing tests на query-time stat engine.
3. Подтвердить `RED`.
4. Довести минимальный exact-core implementation до `GREEN`.
5. Добавить/обновить parser_worker proof tests на real import path.
6. Синхронизировать `CLAUDE.md`, `RUNBOOK.md`, seeds и progress.
7. Прогнать full backend gate.

Типы тестов:

- unit tests на formula/nullability/coverage;
- query-time tests на stat groups в `mbr_stats_runtime`;
- ignored integration tests в `parser_worker` на живой PostgreSQL import path;
- regression tests на blocked-to-mapped transitions.

## Риски

- Самый тяжёлый риск — не event counts, а KO-money / posterior layer.
- Старые chart/widget helper-алгоритмы из `MBR_Stats` нельзя переносить “как есть”, даже если они визуально привычны.
- Статы по KO attempts и stack conversion могут потребовать нового exact-core helper surface, которого пока нет.
- Worktree уже грязный; при реализации нельзя трогать unrelated ветки и файлы.

## Критерии готовности

- Весь legacy stat catalog из `MBR_Stats/stats` покрыт canonical new keys в новом backend.
- Значения stat values считаются только query-time.
- `mbr_stats_runtime` является единственным источником формул.
- В `docs/stat_catalog/mbr_stats_spec_v1.yml` не остаётся старых `blocked`/`legacy_only` статусов для canonical stat modules текущего каталога.
- `CLAUDE.md`, `RUNBOOK.md`, seed metadata и tests отражают новый stat engine честно.
