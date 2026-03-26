# Position Index / Label Split Design

Дата: 2026-03-26
Задача: `P0-04` из `docs/par_nor.md`

## Проблема

Сейчас exact-core persisted surface использует только строковый `position_code` вместе с `preflop_act_order_index` и `postflop_act_order_index`. Это смешивает два разных смысла:

- machine-level факт места игрока относительно баттона и active-seat topology;
- человекочитаемый покерный label, который может быть спорным для short-handed конфигураций.

Из-за этого downstream легко начинает строить аналитику по строковому label как по канонической машинной истине.

## Цель

Сделать breaking cleanup текущего контракта:

- ввести отдельный `position_index` как machine-safe факт;
- оставить `position_label` только как display/analytics label;
- сохранить отдельные `preflop_act_order_index` и `postflop_act_order_index`;
- расширить позиционный движок до `2..=10` active players;
- обновить persistence/runtime surface так, чтобы downstream больше не зависел только от строкового label.

## Принятые решения

### 1. Machine fact

`position_index` — это `u8`, 1-based, считается по active seats строго по часовой стрелке от баттона:

- `1` всегда получает BTN seat;
- далее идут следующие active seats по кругу;
- `position_index` не дублирует action-order индексы и не зависит от label table.

### 2. Human-readable label

`position_label` — отдельный label, который может использоваться в UI/filters/reporting, но не считается машинной истиной для вычислений.

Для heads-up выбран compact policy:

- `BTN` остается `BTN`, даже если этот seat одновременно posts SB;
- факт blind-role остается в blind/action surface, а не кодируется в `position_label`.

### 3. Label mapping for 2..=10 active players

- `2`: `BTN`, `BB`
- `3`: `BTN`, `SB`, `BB`
- `4`: `BTN`, `SB`, `BB`, `CO`
- `5`: `BTN`, `SB`, `BB`, `HJ`, `CO`
- `6`: `BTN`, `SB`, `BB`, `LJ`, `HJ`, `CO`
- `7`: `BTN`, `SB`, `BB`, `MP`, `LJ`, `HJ`, `CO`
- `8`: `BTN`, `SB`, `BB`, `UTG+1`, `MP`, `LJ`, `HJ`, `CO`
- `9`: `BTN`, `SB`, `BB`, `UTG`, `UTG+1`, `MP`, `LJ`, `HJ`, `CO`
- `10`: `BTN`, `SB`, `BB`, `UTG`, `UTG+1`, `UTG+2`, `MP`, `MP+1`, `HJ`, `CO`

Эта таблица фиксируется snapshot-тестами.

### 4. Breaking cleanup scope

Изменение делается сразу по текущему persisted/runtime surface:

- `tracker_parser_core::models::HandPosition` меняется с `position_code` на `position_index + position_label`;
- `core.hand_positions` schema перестает хранить старый `position_code`;
- `parser_worker` пишет и читает новый surface;
- `mbr_stats_runtime` seat-surface filters переходят на `position_label` и получают numeric `position_index`;
- `backend/docs/exact_core_contract.md` и `CLAUDE.md` обновляются под новый контракт.

## Не делаем в этом срезе

- не вводим отдельный `blind_role` column;
- не пересматриваем preflop/postflop actor-order engine;
- не меняем semantic meaning `preflop_act_order_index` / `postflop_act_order_index`.

## Acceptance

- actor-order tests остаются зелеными;
- есть явный expected-label snapshot для `2..=10` active players;
- в persisted/runtime surface больше нет зависимости только от `position_label`;
- `position_index` доступен downstream как отдельный machine-safe факт.
