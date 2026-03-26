# Phase0 Edge Matrix Design

## Проблема

У `tracker_parser_core` уже есть сильный набор точечных parser/normalizer тестов и один канонический phase0 proof-pack:

- `backend/fixtures/mbr/hh/GG20260325-phase0-exact-core-edge-matrix.txt`
- `backend/crates/tracker_parser_core/tests/phase0_exact_core_corpus.rs`

Но сейчас этот слой не является полным `P0-03` acceptance gate из `docs/par_nor.md`:
- сценарии покрыты неравномерно;
- часть кейсов существует только как точечные unit tests, а не как proof-pack;
- в matrix сейчас 10 рук, и этого недостаточно для полного phase0-контракта по forced all-in / blinds / ante / dead blind / side-pot ladder.

Из-за этого exact-core edge behavior еще не зафиксирован как один жесткий regression gate.

## Цель

Довести один канонический phase0 edge-matrix до полного acceptance `P0-03`, не распыляя сценарии по множеству fixture-файлов и не переписывая весь proof-pack с нуля.

## Подходы

### 1. Один канонический proof-pack

Плюсы:
- одна точка входа для phase0 regression;
- проще видеть coverage целиком;
- меньше риска расхождения между разными fixture-пакетами.

Минусы:
- нужно аккуратно держать manifest-style проверки, иначе тест станет громоздким.

### 2. Много отдельных edge-fixture файлов по группам кейсов

Плюсы:
- локально проще читать отдельные кейсы.

Минусы:
- phase0 proof расползается;
- выше риск дублирования и пропуска сценариев;
- сложнее держать “один канонический exact-core pack”.

## Выбранный вариант

Вариант 1: один канонический proof-pack `GG20260325-phase0-exact-core-edge-matrix.txt` плюс manifest-style proof-gate в `phase0_exact_core_corpus.rs`.

## Наблюдения по текущему coverage

Текущий matrix уже содержит базу для:
- sit-out + dead blind (`BRCM0401`);
- partial reveal / no-show (`BRCM0402`);
- short ante all-in (`BRCM0403`);
- HU preflop illegal order (`BRLEGAL2`);
- HU postflop illegal order + uncalled return (`BRLEGAL3`);
- short all-in non-reopen (`BRLEGAL4`);
- side-pot with folded contributor (`BRSIDE1`);
- hidden-showdown ambiguity (`BRCM0502`);
- odd chip (`BRCM0503`);
- joint multi-pot KO (`BRCM0601`).

Минимально недостает:
- отдельного `short BB => forced blind all-in`;
- `dead blind + ante` в одной руке;
- явного `3+ level side-pot ladder` с полным proof по `final_pots`, `pot_contributions`, `pot_eligibilities`.

## Дизайн

### 1. Fixture strategy

Не переписываем текущий matrix полностью.

Делаем минимальное расширение:
- сохраняем существующие руки;
- добавляем только недостающие 2-3 руки;
- не дублируем сценарий, если он уже есть и его можно усилить тестом.

### 2. Test strategy

Оставляем трехслойный контур:

- `fixture_parsing.rs`
  - parser surface: ключевые action nodes, forced metadata, dead blind / sit-out parsing;
- `hand_normalization.rs`
  - глубокие математические и semantic asserts;
- `phase0_exact_core_corpus.rs`
  - главный proof-gate по одному fixture-pack.

### 3. Acceptance model

Для каждого hand в edge-pack задается краткий manifest:
- какие key action nodes проверить;
- какие forced/all-in поля проверить;
- какие normalized outputs проверить;
- ожидается ли exact / uncertain / inconsistent.

Общие обязательные свойства:
- parse без panic;
- normalize без panic;
- нет unexpected warnings;
- нет invariant errors, кроме явно ожидаемых uncertainty/inconsistency кейсов.

### 4. Границы задачи

В `P0-03` не делаем:
- новый persisted contract;
- refactor normalizer semantics;
- property/fuzz testing;
- golden JSON snapshots;
- KO semantics redesign.

Это именно hardening phase0 regression matrix.

## Критерии готовности

- phase0 edge-matrix покрывает весь `P0-03` acceptance checklist;
- в matrix добавлены только реально недостающие руки;
- `phase0_exact_core_corpus.rs` стал строгим manifest-style proof-gate;
- parser и normalizer regressions по edge-pack проходят стабильно;
- проверка фиксирует `seq`, `street`, `player_name`, `action_type`, forced/all-in metadata, `committed_total`, `returns`, `final_pots`, `pot_contributions`, `pot_eligibilities`.
