# Committed Pack Syntax Catalog

Дата: 2026-03-27
Статус: active family-level catalog for GG MBR committed surfaces

## Зачем это нужно

Этот каталог фиксирует не каждую сырую строку, а нормализованные семейства синтаксических поверхностей, которые уже встречаются в committed GG MBR pack и в committed quarantine sample.

Каталог нужен для трех вещей:

- отличать known explicit surfaces от реально новых parser gaps;
- review-friendly обновлять triage без свалки из почти одинаковых строк;
- синхронизировать allowlist для `wide_corpus_triage`.

## Статусы

- `allowed_explicit_surface` — известная поверхность, которую мы пока осознанно принимаем как typed `parse_issue`, а не как parser failure.
- `diagnostic_tail_conflict` — диагностическая поверхность: файл парсится, но хвост подтверждения конфликтует с result line.
- `needs_grammar_support` — новый family, который пока не признан допустимым и требует grammar work. В текущем committed catalog таких записей нет.

## Канонические семейства

| family_key | surface_kind | issue_code / failure | status | notes |
|---|---|---|---|---|
| `hh_show_line::partial_reveal_show_line` | `hh_show_line` | `partial_reveal_show_line` | `allowed_explicit_surface` | showdown line показывает неполный reveal вроде `shows [5d]`; hand продолжает парситься, но reveal intentionally остается неполным |
| `hh_summary_show_line::partial_reveal_summary_show_surface` | `hh_summary_show_line` | `partial_reveal_summary_show_surface` | `allowed_explicit_surface` | summary line повторяет частичный reveal surface |
| `hh_show_line::unsupported_no_show_line` | `hh_show_line` | `unsupported_no_show_line` | `allowed_explicit_surface` | явный `doesn't show hand`; surface сохраняем как typed warning, не превращая в fake cards |
| `ts_tail::ts_tail_finish_place_mismatch` | `ts_tail` | `ts_tail_finish_place_mismatch` | `diagnostic_tail_conflict` | result line и tail confirmation расходятся по месту |
| `ts_tail::ts_tail_total_received_mismatch` | `ts_tail` | `ts_tail_total_received_mismatch` | `diagnostic_tail_conflict` | result line и tail confirmation расходятся по payout |

## Representative examples

### `hh_show_line::partial_reveal_show_line`

- `PartialA: shows [5d]`
- `PartialB: shows [6d]`
- committed real-pack example: `43b06066: shows [5d] (a pair of Fives)`

### `hh_summary_show_line::partial_reveal_summary_show_surface`

- `Seat 2: PartialA (small blind) showed [5d] and lost`
- `Seat 2: PartialB (small blind) showed [6d] and lost`

### `hh_show_line::unsupported_no_show_line`

- `NoShowA: doesn't show hand`
- `NoShowB: doesn't show hand`

### `ts_tail::*`

- `You finished the tournament in 2nd place.`
- `You received a total of $204.`

## Правило обновления

Когда `wide_corpus_triage` находит новый family:

1. Сначала решаем, это expected explicit surface или нежелательный parser gap.
2. Если surface признан допустимым, добавляем family сюда и только после этого при необходимости расширяем allowlist.
3. Если surface требует grammar support, фиксируем family здесь со статусом `needs_grammar_support` и не добавляем в allowlist автоматически.

Важно: allowlist и catalog должны оставаться согласованными, но allowlist не должен расширяться без review.
