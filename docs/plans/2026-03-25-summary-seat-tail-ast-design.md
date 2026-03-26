# Summary Seat Tail AST Design

## Проблема

`parse_summary_seat_outcome_line` уже умеет разбирать основные `*** SUMMARY ***` seat-result строки, но делает это через хрупкий набор tail-prefixes и точечные regex-хелперы. Из-за этого:
- логика `head` и `tail` смешана в одной функции;
- неизвестный `tail` сейчас маскируется под общий `unparsed_summary_seat_line`, хотя `Seat N / player / marker` могли быть распознаны корректно;
- `parse_summary_collected_tail` и соседние хелперы плохо масштабируются на новые хвосты и подтвержденные суффиксы;
- parser hardening по summary tails тяжело расширять без новых ad-hoc веток.

## Цель

Ввести внутренний AST для `summary seat-result tail`, не меняя внешний `SummarySeatOutcome` surface, чтобы:
- отделить parse `head` от parse `tail`;
- сделать grammar хвостов декларативным и расширяемым;
- переводить unknown tail в reason-coded warning уровня tail;
- оставить downstream-контракт parser/normalizer неизменным.

## Подходы

### 1. Доработать текущие regex-хелперы точечно

Плюсы:
- быстро;
- минимальный diff.

Минусы:
- хрупкость останется;
- следующий новый tail снова потребует точечной латки;
- не решает проблему смешения `head`/`tail`.

### 2. Ввести внутренний AST tail + mapping в текущий `SummarySeatOutcome`

Плюсы:
- закрывает `P0-02` без schema/model refactor;
- дает расширяемый parser-internal слой;
- позволяет reason-coded warning именно по tail.

Минусы:
- больше изменений во внутреннем parser-коде, чем у точечного patch.

### 3. Сразу вводить persisted AST и typed parser issues

Плюсы:
- самый фундаментальный вариант.

Минусы:
- уже выходит за границы `P0-02`;
- смешивает parser hardening с отдельной задачей typed issues / schema evolution.

## Выбранный вариант

Вариант 2: внутренний AST для summary tail, внешний `SummarySeatOutcome` сохраняется без изменений.

## Дизайн

### 1. Разделение на `head` и `tail`

Новый parser pipeline:
1. `parse_summary_seat_head(line)` разбирает:
   - `seat_no`
   - `player_name`
   - `position_marker`
   - сырой `tail`
2. `parse_summary_seat_tail_ast(tail)` возвращает внутренний enum AST.
3. `map_summary_tail_ast_to_outcome(...)` строит текущий `SummarySeatOutcome`.

Если `head` не распознан:
- warning остается `unparsed_summary_seat_line: ...`

Если `head` распознан, но `tail` нет:
- warning становится отдельным reason-coded кодом tail-уровня

### 2. Scope grammar `v1`

Поддерживаемые формы:
- `folded before Flop`
- `folded on the Flop|Turn|River`
- `showed [..] and won (...)`
- `showed [..] and lost`
- `won (...)`
- `collected (...)`
- `mucked`
- `lost`

Поддерживаемые суффиксы:
- `with <hand_class>` после `showed ... and won`
- `with <hand_class>` после `showed ... and lost`
- `with <hand_class>` после `won (...)`

`collected (...)` parser должен стать расширяемым, но в `P0-02` не обязан поддерживать неподтвержденные свободные suffix-формы без fixture evidence.

### 3. Warning policy

- `unparsed_summary_seat_line: ...` только для head-level failures
- новый tail-level warning для случая "head ok, tail unknown"
- warning surface пока остается строковым, чтобы не смешивать задачу с typed parser issues

### 4. Downstream compatibility

Не меняем:
- `SummarySeatOutcome`
- `CanonicalParsedHand`
- `NormalizedHand`
- DB schema

Минимально затрагиваем:
- parser tests
- parser worker warning-to-issue mapping, чтобы новый tail-code не уходил в generic `parser_warning`
- regression helpers, если они whitelist-ят explicit warning codes

## Тестовая стратегия

- synthetic parser test с полным набором известных tail-форм
- отдельный test на unknown tail при корректном head:
  - no `unparsed_summary_seat_line`
  - yes new tail-specific warning
- committed HH regression:
  - не должно появиться новых неожиданных warning-ов
- phase0 edge regression:
  - explicit warning whitelist при необходимости дополняется новым tail-code только там, где это действительно нужно

## Не входит в задачу

- typed parser issues
- новый persisted AST
- schema changes
- normalizer/pot-resolution refactor
- глобальная warning-model migration
