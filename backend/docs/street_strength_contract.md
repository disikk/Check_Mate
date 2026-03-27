# Street Strength Contract

## Статус

Этот документ фиксирует текущий exact postflop contract `tracker_parser_core::street_strength` по состоянию на 2026-03-26.

Он описывает уже реализованное поведение descriptor-слоя и служит каноническим reference для:

- `backend/crates/tracker_parser_core/src/street_strength.rs`
- `backend/crates/tracker_parser_core/tests/street_hand_strength.rs`
- `backend/crates/tracker_parser_core/tests/street_strength_reference.rs`
- `backend/crates/tracker_parser_core/tests/street_strength_corpus_golden.rs`

## Proof Surface

Текущий exact contract защищён тремя уровнями tests:

- synthetic acceptance coverage в `street_hand_strength.rs`;
- independent reference/differential harness в `street_strength_reference.rs`;
- corpus-backed golden suite в `street_strength_corpus_golden.rs`.

Corpus-backed layer разделён на два snapshot-формата:

- curated raw real-hand golden с полным active row contract;
- aggregated full-pack golden sweep по committed HH fixtures.

Оба golden-файла обновляются только через explicit `UPDATE_GOLDENS=1`.

## Проблема

До этого момента семантика `street_strength` жила в коде, тестах и коротких заметках в `CLAUDE.md`. После перехода ordinary draw на improvement-aware live-outs semantics это стало недостаточно надежно: любой следующий рефакторинг рискует незаметно изменить exact contract.

## Цель

Сделать current `street_strength` contract:

- явным;
- отделенным от hand-grain/runtime описаний;
- привязанным к текущему exact behavior;
- пригодным как baseline для дальнейшего test hardening.

## Scope

Этот контракт покрывает:

- postflop materialization (`flop` / `turn` / `river`);
- `best_hand_class` и `best_hand_rank_value`;
- `made_hand_category`;
- `draw_category`;
- `overcards_count`;
- `has_air`;
- `missed_flush_draw` / `missed_straight_draw`;
- `is_nut_hand`;
- `is_nut_draw`;
- `certainty_state`;
- текущую nut-policy semantics.

Не входят в текущий contract:

- preflop descriptors;
- redraw categories;
- улучшения внутри того же hand class;
- heuristic bucket projection (`best | good | weak | trash`), потому что это runtime/UI layer, а не exact layer.

## Materialization scope

### Seat scope

`street_strength` materialize-ится только для:

- Hero;
- opponents, чьи hole cards exact-known по showdown surface.

Guessed / partial / unknown opponents в этот layer не попадают.

### Street scope

Rows materialize-ятся только для реально достигнутых postflop streets:

- flop;
- turn;
- river.

Префлоп в active contract отсутствует.

## Best hand surface

### `best_hand_class`

Текущий лучший class на конкретной улице:

- `high_card`
- `pair`
- `two_pair`
- `trips`
- `straight`
- `flush`
- `full_house`
- `quads`
- `straight_flush`

### `best_hand_rank_value`

Exact rank-order value внутри class. Поле используется как exact ordering/tiebreak surface, но ordinary draw semantics на нем не базируются.

## Made hand category

`made_hand_category` — rule-based postflop descriptor поверх current best hand.

Текущие значения:

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

Это exact descriptor layer текущего проекта, а не универсальная EV-модель силы руки.

## Draw contract

### Общий принцип

`draw_category` — exact postflop descriptor текущего improving potential.

Есть два разных смысловых класса:

1. ordinary draw
2. `backdoor_flush_only`

Они намеренно не эквивалентны.

### Ordinary draw

Ordinary draw categories:

- `gutshot`
- `open_ended`
- `double_gutshot`
- `flush_draw`
- `combo_draw`

Ordinary draw materialize-ится только из legal unseen next-card live outs, которые:

- доступны из текущего exact board state;
- повышают именно `best_hand_class`;
- используют хотя бы одну hole card в resulting best hand.

Следствия:

- board-only pseudo-draw не materialize-ятся;
- redraw к более сильной руке внутри того же class не materialize-ится;
- pure redraw к `straight_flush` не materialize-ится как ordinary straight/flush draw, если next card не дает exact `Straight` или exact `Flush`.

### Straight draw semantics

Straight draw categories определяются только из real next-card outs, чьим final best class становится exact `Straight`.

Классификация:

- `gutshot`: ровно один completion rank;
- `open_ended`: минимум два completion ranks и edge-completion straight pattern;
- `double_gutshot`: минимум два completion ranks без open-ended shape.

### Flush draw semantics

`flush_draw` materialize-ится только если существует legal unseen next card, который:

- повышает `best_hand_class`;
- делает resulting best class exact `Flush`;
- still uses at least one hole card in that exact best hand.

Если одновременно существуют ordinary flush out и ordinary straight out, результат — `combo_draw`.

### `backdoor_flush_only`

`backdoor_flush_only` — flop-only descriptor.

Он materialize-ится только если:

- ordinary `flush_draw` отсутствует;
- существует runner-runner путь в flush-family;
- итоговый best hand на таком пути повышает `best_hand_class`;
- final best class на этом пути — `Flush` или `StraightFlush`;
- resulting best hand использует хотя бы одну hole card.

Это значит:

- две suited hole cards + одна suited flop card дают валидный `backdoor_flush_only`;
- одна suited hole card + две suited flop cards тоже дают валидный `backdoor_flush_only`;
- path в `StraightFlush` включается в contract `backdoor_flush_only`;
- descriptor не materialize-ится на turn/river.

## Auxiliary exact descriptors

### `overcards_count`

Материализуется только если current best hand — `high_card`.

Считает количество hole cards, которые старше максимального board rank.

### `has_air`

`has_air = true`, только если:

- current best hand — `high_card`;
- ordinary flush draw отсутствует;
- ordinary straight draw отсутствует;
- `overcards_count == 0`.

`backdoor_flush_only` сам по себе не отключает `has_air` в текущем contract.

## Missed draw contract

### Источник истории

`missed_flush_draw` и `missed_straight_draw` на river строятся только из historical ordinary draw facts, а не из board pattern heuristics.

`backdoor_flush_only` сам по себе historical frontdoor draw не образует.

Redraw history в `missed_*` не входит.

### `missed_flush_draw`

`true`, только если:

- на flop или turn существовал ordinary `flush_draw` или `combo_draw`;
- к river flush-family не собрался;
- final river hand не suppress-ит исторический missed-факт.

### `missed_straight_draw`

`true`, только если:

- на flop или turn существовал ordinary straight draw (`gutshot` / `open_ended` / `double_gutshot` / `combo_draw`);
- к river straight-family не собрался;
- final river hand не suppress-ит исторический missed-факт.

### Backdoor promotion

`backdoor_flush_only` на flop сам по себе не materialize-ит `missed_flush_draw`.

Но если на одной из следующих улиц backdoor-path превращается в ordinary `flush_draw`, а к river flush-family не собирается, тогда `missed_flush_draw = true`.

Аналогично для straight-family: pure backdoor history не считается missed до тех пор, пока не появится ordinary straight draw той же family.

## Nut fields

### `is_nut_hand`

`is_nut_hand` в текущем contract active и governed by `STREET_HAND_STRENGTH_NUT_POLICY = hand_and_draw`.

Поле materialize-ится как exact `Some(true | false)` для каждого postflop row.

Семантика:

- берется текущий board state этой улицы;
- берутся hole cards самого игрока;
- перечисляются все legal opponent two-card combinations из оставшейся колоды;
- для каждой opponent combo считается exact best hand на том же board;
- `true` возвращается только если ни одна legal combo не дает строго больший `best_hand_rank_value`.

Следствия:

- shared nuts / chop считаются `true`;
- known showdown hole cards оппонентов не считаются dead cards;
- значение зависит только от board + hole cards этого игрока, а не от конкретного reveal surface раздачи.

### `is_nut_draw`

`is_nut_draw` в текущем contract active и тоже governed by `STREET_HAND_STRENGTH_NUT_POLICY = hand_and_draw`.

Поле materialize-ится как exact `Some(true | false)` для каждого postflop row.

Семантика:

- участвуют только ordinary draw families;
- ordinary family определяется теми же legal improving next-card outs, что и `draw_category`;
- `backdoor_flush_only` сам по себе никогда не дает `is_nut_draw = true`;
- river rows и rows без ordinary draw materialize-ятся как `Some(false)`.

#### Flush family

Flush family считается nut-family только если все ordinary flush outs приводят к resulting hand, для которой `is_nut_hand = true` на resulting board.

Следствие:

- nut flush draw не определяется по одному lucky out;
- dominated flush family дает `false`, даже если один частный out случайно приводит к nut flush.

#### Straight family

Straight family считается nut-family только если все ordinary straight outs приводят к resulting hand, для которой `is_nut_hand = true` на resulting board.

Следствие:

- dominated straight family на monotone / paired / connected boards дает `false`, если хотя бы часть ordinary completions приводит к non-nut result.

#### Combo draw

`combo_draw` дает `is_nut_draw = true`, если хотя бы одна active ordinary family является nut-family:

- nut straight family + non-nut flush family -> `true`;
- nut flush family + non-nut straight family -> `true`;
- две non-nut family -> `false`.

## Certainty

Текущий `street_strength` contract materialize-ится как `certainty_state = exact` для rows, которые вообще были созданы.

Отдельного estimated/guessed branch в active contract нет.

## Non-goals current version

Этот contract сознательно не обещает:

- preflop materialization;
- redraw taxonomy;
- draw semantics by same-class rank improvement;
- opponent-range or EV-aware strength model.
