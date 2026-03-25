# Street Strength V2 Phase 1 Design

**Дата:** 2026-03-25

## Проблема

Текущий `street_strength` v1 в `tracker_parser_core` опирается на корректный exact made-hand evaluator, но descriptor-слой поверх него дает ложные сигналы в двух критичных местах:

- straight-draw флаги срабатывают на board-only completions, где hole cards игрока не участвуют в потенциальном стритe;
- агрегированный `has_missed_draw_by_river` смешивает разные типы дро, учитывает backdoor-only случаи и продолжает шуметь даже после улучшения руки в сильную made-hand категорию.

Одновременно `is_nut_hand` и `is_nut_draw` присутствуют в persisted surface, но фактически всегда `NULL`, что создает ложное ощущение поддержанной функциональности.

## Цель

Закрыть только `TASK-SH-001..005` из `docs/сила руки.md`, не заходя в полную категориальную модель v2 и не делая runtime/filter integration. Результат этого этапа должен дать формальный контракт поведения, regression harness и корректный exact descriptor substrate для straight/flush missed logic.

## Выбранный подход

Выбран узкий `spec-first + narrow v2 slice`.

Что делаем:

- формализуем правила в `docs/STREET_STRENGTH_V2_SPEC.md`;
- расширяем тестовый harness так, чтобы RED/GREEN явно доказывал найденные ошибки;
- меняем только ту часть `street_strength.rs`, которая отвечает за player-specific straight draws и river missed-draw semantics;
- фиксируем честный `deferred` contract для nut-полей вместо псевдо-реализации.

Что не делаем:

- не вводим `made_hand_category_v2`;
- не вводим `draw_category_v2` как полный canonical слой;
- не делаем versioned persistence migration;
- не интегрируем изменения в feature registry, AST filters и UI/public bucket layer.

## Ключевые решения

## 1. Straight draw должен быть player-specific

Completion rank считается valid draw completion только если существует хотя бы один завершенный стрит, в котором участвует минимум одна hole card игрока. Если completion создает стрит только на борде, пользовательские draw-флаги не поднимаются.

Следствие:

- board-only OESD/gutshot false positives исчезают;
- при желании дальше можно добавить диагностический board-only flag, но в этом этапе это не обязательно.

## 2. `has_missed_draw_by_river` заменяется на раздельные missed-флаги

Вместо одного агрегированного бита вводятся:

- `missed_flush_draw`
- `missed_straight_draw`

Они должны отражать только meaningful missed draws и не должны:

- активироваться от backdoor-only flush draw;
- оставаться true после улучшения в сильную made hand, для которой старое draw-состояние больше не релевантно.

Для этого этапа safe rule такая:

- missed flush/straight учитывается только для frontdoor/player-specific active draws;
- missed-флаг не ставится, если на river получена hand category уровня `two_pair+`.

## 3. Nut fields получают explicit deferred contract

На этом этапе nut-логика не реализуется эвристически. Вместо этого мы документируем и закрепляем в коде, что:

- `is_nut_hand` и `is_nut_draw` недоступны в текущем descriptor version;
- они не должны трактоваться как вычисленные exact-поля;
- их отсутствие является осознанным deferred decision до отдельной спецификации.

## Компоненты и затронутые файлы

- `backend/crates/tracker_parser_core/src/street_strength.rs`
- `backend/crates/tracker_parser_core/tests/street_hand_strength.rs`
- `backend/crates/parser_worker/src/local_import.rs` при необходимости адаптации persisted mapping
- `backend/migrations/0001_init_source_of_truth.sql` только если без схемы невозможно честно выразить новый surface
- `docs/STREET_STRENGTH_V2_SPEC.md`
- `CLAUDE.md` только если изменится архитектурный contract hand-strength слоя

## Тестовая стратегия

- Сначала добавить failing tests на board-only straight completions.
- Затем добавить failing tests на false-positive missed draw scenarios.
- Отдельно зафиксировать behavior для ordinary busted flush draw, который должен остаться `true`.
- Проверить, что existing exact made-hand coverage не регрессирует.
- Запустить focused `street_hand_strength` tests и затем backend-targeted tests, если будет затронут importer.

## Риски

- Existing importer/persistence layer завязан на текущие поля v1, поэтому новый surface надо вносить минимально и без неявного semantic drift.
- Если окажется, что schema already assumes old field shape, Phase 1 должна выбрать минимальный безопасный adaptation path, а не пытаться тайком сделать весь `TASK-SH-009`.

## Критерии готовности этого этапа

- Есть формальная спецификация Phase 1 contract.
- Есть regression cases на подтвержденные P0 ошибки.
- Board-only straight false positives устранены.
- False-positive missed draw by river устранены.
- Nut-fields не остаются в состоянии "как будто поддерживаются".
