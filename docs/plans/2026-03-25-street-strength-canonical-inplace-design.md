# Street Strength Canonical In-Place Design

**Дата:** 2026-03-25

## Проблема

После Phase 1 exact made-hand evaluator и ключевые P0 баги в draw/missed semantics уже вычищены, но persisted слой `derived.street_hand_strength` всё ещё хранит legacy-контракт:

- `pair_strength` вместо канонической made-hand категории;
- россыпь booleans для draw surface;
- бинарный `has_overcards`;
- грубый `has_missed_draw_by_river`;
- `descriptor_version` как часть persisted surface.

Такой контракт плохо подходит для фильтров и статов: он шумный, неполный и тащит historical naming, который пользователь больше не хочет сохранять.

## Цель

Закрыть `TASK-SH-006..009` как **жёсткую in-place замену** persisted-слоя `street_strength`, без параллельного legacy/v2-сосуществования и без новых публичных versioned имен.

Итоговый persisted contract должен стать каноническим и unversioned:

- `best_hand_class`
- `best_hand_rank_value`
- `made_hand_category`
- `draw_category`
- `overcards_count`
- `has_air`
- `missed_flush_draw`
- `missed_straight_draw`
- `is_nut_hand`
- `is_nut_draw`
- `certainty_state`

## Выбранный подход

Выбран вариант `hard in-place replacement`.

Что делаем:

- переписываем `tracker_parser_core::street_strength` на канонические категории;
- меняем схему `derived.street_hand_strength` append-only migration-ом;
- удаляем legacy persisted columns и технический `descriptor_version` из runtime-контракта;
- обновляем importer, tests, SQL snippets и `CLAUDE.md` в одном цикле, чтобы breaking change был явным, а не silent.

Что не делаем:

- не создаем новую таблицу;
- не держим legacy и canonical поля рядом;
- не делаем feature-registry/runtime integration из `TASK-SH-010+`.

## Канонический контракт

## 1. `made_hand_category`

Поле должно выражать board-aware / hole-card-aware made-hand category без старого `pair_strength`.

Минимальный целевой набор:

- `high_card`
- `board_pair_only`
- `underpair`
- `third_pair`
- `second_pair`
- `top_pair_weak`
- `top_pair_good`
- `top_pair_top`
- `overpair`
- `two_pair`
- `set`
- `trips`
- `straight`
- `flush`
- `full_house`
- `quads`
- `straight_flush`

## 2. `draw_category`

Поле должно быть канонической draw-иерархией, а не набором независимых битов.

Целевой порядок силы:

- `combo_draw`
- `flush_draw`
- `double_gutshot`
- `open_ended`
- `gutshot`
- `backdoor_flush_only`
- `none`

Board-only draw сюда не попадает.

## 3. `overcards_count` и `has_air`

- `overcards_count` материализуется как `0 | 1 | 2`;
- `has_air` вычисляется только после исключения made hands, player-specific draws и meaningful overcard cases.

## 4. River missed draws

Так как старый агрегированный бит удаляется, persisted surface должен хранить раздельно:

- `missed_flush_draw`
- `missed_straight_draw`

Это согласуется с уже зафиксированной в Phase 1 концептуальной моделью и не теряет информацию при удалении legacy-поля.

## 5. Nut fields

Phase 2 не меняет решение из Phase 1:

- `is_nut_hand` и `is_nut_draw` остаются explicit deferred / unavailable;
- но они продолжают существовать как часть канонического контракта, чтобы не потерять заранее согласованное место в модели.

## Архитектурные решения

## 1. Breaking change делаем явным

Старый persisted contract удаляется одновременно в:

- Rust models;
- importer mapping;
- SQL schema/migration;
- repo docs and runbook.

Так мы избегаем silent drift, когда код уже думает новыми категориями, а БД и документация ещё говорят старыми битами.

## 2. Миграция append-only, не переписывая старые migration files

Для уже существующей цепочки миграций добавляется новая migration, которая:

- дропает legacy columns;
- добавляет канонические columns;
- пересобирает uniqueness без `descriptor_version`.

Исторические migration файлы не переписываются.

## 3. Исторический Phase 1 spec-artifact не является runtime-контрактом

Существующий `docs/STREET_STRENGTH_V2_SPEC.md` нужен как исторический proof artifact предыдущего цикла.
Новый рабочий runtime-контракт должен быть описан отдельным unversioned документом, чтобы текущая модель больше не несла versioned naming.

## Тестовая стратегия

- RED tests на `made_hand_category` для paired-board / pair-family distinctions.
- RED tests на `draw_category` и приоритет конфликтных draw cases.
- RED tests на `overcards_count` и `has_air`.
- После core GREEN — importer/persistence tests на новый schema contract.
- Затем targeted SQL/query docs update checks.

## Риски

- Это осознанный breaking schema change; старые локальные SQL-запросы и ad-hoc tooling надо обновить в том же цикле.
- Так как worktree уже грязный по другим backend-веткам, builder должен трогать только street-strength surface и не вмешиваться в другие незавершенные фазы exact-core.

## Критерии готовности

- Legacy persisted columns удалены из канонического слоя.
- Новый unversioned contract документирован и покрыт тестами.
- `tracker_parser_core` и importer используют только канонические поля.
- `RUNBOOK` и `CLAUDE.md` больше не описывают legacy street-strength surface.
