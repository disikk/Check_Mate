
# Аудит репозитория `disikk/Check_Mate`
## Фокус: стадия проекта, корректность parser/normalizer, pot-resolution, порядок действий, all-in/forced all-in, позиции

Дата аудита: 2026-03-27  
Репозиторий: `https://github.com/disikk/Check_Mate`

---

## 1. Что именно я проверял

Я вручную и очень детально прошёл:

- `backend/crates/tracker_parser_core/src/`
  - `parsers/hand_history.rs`
  - `parsers/tournament_summary.rs`
  - `normalizer.rs`
  - `pot_resolution.rs`
  - `betting_rules.rs`
  - `positions.rs`
  - `models.rs`
- тесты `tracker_parser_core/tests/`
  - `hand_normalization.rs`
  - `fixture_parsing.rs`
  - `phase0_exact_core_corpus.rs`
  - `pot_math_properties.rs`
  - `positions.rs`
  - golden/fixture-слой
- структуру проекта целиком: backend/runtime/web/frontend-контур.

### Ограничение аудита
Это **статический аудит кода и тестов**. В текущем контейнере нет `cargo/rustc`, поэтому я **не запускал** тесты сам. Оценка ниже опирается на:
- исходники;
- тесты;
- golden snapshot-ы;
- committed fixtures;
- README/документацию репозитория.

---

## 2. Короткий вердикт

### Моя оценка стадии
- **Проект целиком как продукт**: примерно **25–30%**.
- **Ядро `tracker_parser_core` на committed GG MBR surface**: примерно **55–65%**.
- **Как production-grade parser/normalizer на широком реальном корпусе**: **ещё не готов**.

### Почему так
Сильнее всего выглядит именно exact-core для узкого surface:
- есть отдельные модули на parser / legality / normalizer / pot-resolution / positions;
- есть осознанный `certainty_state`;
- есть тесты на side pots, odd chip, hidden showdown ambiguity, short all-in, actor order, forced all-in by blind/ante exhaustion, heads-up order, golden snapshots;
- есть edge-matrix для phase0 exact core.

Но утверждать «там нет ни единой ошибки» **сейчас нельзя**. Причина не в общей идее — идея хорошая. Причина в том, что:
1. есть несколько **реальных дефектов/семантических ловушек**;
2. часть логики пока безопасна только внутри узкого committed surface;
3. документация о текущем состоянии местами уже отстаёт от дерева репозитория;
4. под malformed / drifted surface есть места, где состояние может уйти в некорректные значения.

### Практический вывод
Сейчас это:
- **хороший exact-core foundation** для узкого GG MBR pack;
- **не тот уровень**, на котором я бы разрешил параллельно наращивать новые фичи до закрытия P0/P1-блока.

---

## 3. Что уже выглядит сильным и в целом корректным

Ниже — то, что по результатам разбора выглядит действительно хорошей основой и **не должно быть сломано** при исправлениях.

### 3.1. Математика построения банков
`pot_resolution.rs` строит банки по уникальным уровням cumulative contribution. Это правильный базовый способ для main/side-pot decomposition.

Что хорошо:
- folded contributors остаются в `contributions`, но исключаются из `eligibilities`;
- main/side-pot slicing не завязан на порядок collect lines;
- есть property tests на pot-contract.

### 3.2. Подход «лучше uncertain, чем guessed exact»
Это одна из лучших частей текущего ядра.

Что хорошо:
- ambiguous hidden showdown не форсится в фиктивных winner-rows;
- conflicting collect/summary evidence переводится в `Inconsistent`, а не «угадывается»;
- odd chip ambiguity обрабатывается как множество candidate allocations и не материализуется в guessed exact.

Это правильная инженерная философия для poker exact-core.

### 3.3. Forced all-in от анте/блайндов
`annotate_action_all_in_metadata()` в `hand_history.rs` в целом правильно помечает:
- `AnteExhausted`;
- `BlindExhausted`;
- `CallExhausted`;
- `RaiseExhausted`;
- `forced_all_in_preflop`.

Под покрытые кейсы это выглядит хорошо.

### 3.4. Очерёдность действий и reopen после short all-in
`betting_rules.rs` выглядит продуманно:
- HU preflop/postflop order покрыт;
- full raise reopen-ит action;
- short all-in raise не reopen-ит action;
- premature street close детектится;
- illegal check/call/bet surfaces не игнорируются молча.

### 3.5. Позиции как seat-order / action-order
Сама **геометрия порядка** (button order, preflop order, postflop order) выглядит корректной.
Важно: ниже у меня есть замечание по **taxonomy labels**, но не по базовому ordering.

---

## 4. Найденные проблемы и риски

Ниже разделено на:
- **подтверждённые дефекты**;
- **высокорисковые контрактные/архитектурные проблемы**;
- **ограничения покрытия**, которые обязательно нужно закрыть до claim-а exactness.

---

## 4.1. P0 — подтверждённые дефекты / опасные некорректные состояния

### P0-01. Возможны отрицательные стеки и отрицательные committed-состояния
**Уверенность:** высокая  
**Статус:** подтверждённый дефект  
**Критичность:** максимальная

**Файлы:**
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:1061-1164`
- `backend/crates/tracker_parser_core/src/betting_rules.rs:470-547`
- `backend/crates/tracker_parser_core/src/normalizer.rs:221-345`

**Суть проблемы**

Во всех трёх replay-проходах:
- all-in annotation pass;
- legality engine state update;
- normalizer replay state;

деньги списываются/возвращаются **без жёсткой защиты от underflow**.

Примеры:
- `calls 200` при доступном остатке 150;
- `bets 500` при остатке 300;
- `raises to 1000` при остатке меньше нужного delta;
- `Uncalled bet returned` больше реально доступного overage/contribution.

Сейчас логика:
- просто вычитает `delta`;
- во многих местах считает all-in только если остаток стал **ровно 0**;
- не запрещает уход в **минус**.

**Почему это опасно**
- игрок может остаться со `stack_current < 0`;
- событие может **не** быть помечено all-in, хотя состояние уже испорчено;
- `committed_total`, `committed_by_street`, `betting_round_contrib` тоже могут уйти в минус;
- дальше начинают врать pot construction, invariants, elimination, final stacks.

**Это не теоретическая косметика.**
Даже если committed pack этого сейчас не содержит, exact-core не должен допускать такие состояния вообще.

**Что исправлять**
1. Вынести единый безопасный слой изменения state:
   - `debit_stack(player, delta, context)`
   - `refund_commitment(player, refund, context)`
2. Никогда не допускать отрицательных:
   - `stack_current`
   - `committed_total`
   - `committed_by_street[*]`
   - `betting_round_contrib`
3. При overflow/underflow:
   - либо завершать нормализацию explicit error;
   - либо materialize-ить invariant issue и **не мутировать** состояние дальше.
4. all-in определять как:
   - `after == 0`;
   - но только после безопасного расчёта;
   - состояние `< 0` не должно существовать вообще.

**Что нельзя делать**
- молча `max(0, ...)` без issue/error;
- чинить только в одном месте из трёх проходов.

---

### P0-02. Возврат uncalled может увести committed-слой в минус даже после того, как mismatch уже замечен
**Уверенность:** высокая  
**Статус:** подтверждённый дефект  
**Критичность:** максимальная

**Файлы:**
- `backend/crates/tracker_parser_core/src/betting_rules.rs:282-305`
- `backend/crates/tracker_parser_core/src/betting_rules.rs:506-516`
- `backend/crates/tracker_parser_core/src/normalizer.rs:253-267`

**Суть проблемы**

`validate_actor_surface()` умеет заметить, что refund слишком большой или actor mismatch.  
Но после этого код всё равно идёт дальше и реально применяет refund к state.

То есть сейчас возможно:
- issue уже поднят;
- но state всё равно испорчен.

**Почему это плохо**
Это худший тип ошибки: система одновременно:
- «понимает», что surface некорректный;
- и всё равно продолжает на его основе строить exact-like state.

**Что исправлять**
- если refund больше допустимого overage / больше committed contribution:
  - state не должен мутировать;
  - hand должна переходить в `Inconsistent`/error path;
  - selected exact settlement не должен materialize-иться как будто ничего не произошло.

---

## 4.2. P1 — подтверждённые семантические проблемы и опасные ловушки контракта

### P1-01. `Call.to_amount` сейчас моделируется семантически неверно
**Уверенность:** высокая  
**Статус:** подтверждённая семантическая ошибка  
**Критичность:** высокая

**Файлы:**
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:935-955`
- см. также контрактные ожидания в `backend/crates/tracker_parser_core/tests/phase0_exact_core_corpus.rs:926-936`

**Суть проблемы**

Для `ActionType::Call` парсер делает:
- `amount = called delta`
- `to_amount = Some(amount)`

Это семантически неверно.

Для call:
- `amount` — это сколько игрок **добавил сейчас**;
- `to_amount` — если это поле вообще существует для call — должен означать **итоговый вклад на улице**, а не delta.

Сейчас `to_amount` у call выглядит как терминальное «to», но содержит просто тот же delta.

**Почему это опасно**
Пока текущий код почти не использует `to_amount` у call, поэтому явного runtime-bug в покрытом surface я не увидел.  
Но это очень опасная мина под будущие потребители:
- derived-layer;
- export/json-contract;
- code-agent refactor;
- UI/drilldown;
- сторонние аналитические процедуры.

Кто-то увидит `to_amount` и вполне логично прочитает его как «вклад игрока после call».

**Что исправлять**
Есть два нормальных варианта:

**Вариант A (предпочтительный):**
- для `Call` ставить `to_amount = None`;
- поле `to_amount` оставить только для `RaiseTo`.

**Вариант B:**
- разделить модель:
  - `delta_amount`
  - `target_contribution`
- и перестать перегружать `amount/to_amount`.

**Важно**
Исправлять надо вместе с:
- тестами edge-matrix;
- golden snapshots;
- любым сериализуемым контрактом.

---

### P1-02. Fallback `parse_hidden_dealt_to_line()` может молча съесть malformed dealt-line
**Уверенность:** высокая  
**Статус:** подтверждённый parser-bug  
**Критичность:** высокая

**Файлы:**
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:169-177`
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:357-377`

**Суть проблемы**

Сейчас последовательность такая:
1. сначала `parse_dealt_to_line(line)` пытается распарсить `Dealt to X [Ah Ad]`;
2. если не получилось, вызывается `parse_hidden_dealt_to_line(line)`;
3. `parse_hidden_dealt_to_line()` матчится на любой `Dealt to <что угодно до конца строки>`.

Из-за этого строка вида:
- `Dealt to Hero [Ah Ad`
- `Dealt to Hero [Ah Ad extra`
- другой полумалформат

может быть не помечена как malformed dealt surface, а тихо пройти как «hidden dealt line».

**Дополнительная проблема**
Если когда-нибудь hero surface придёт как hidden dealt line без карт:
- `hero_name` не сохраняется;
- `normalize_hand()` потом может упасть на `MissingLine("hero_name")`.

**Что исправлять**
1. `parse_hidden_dealt_to_line()` должен матчиться только на строгий формат:
   - `^Dealt to <player_name>$`
   - без `[` и `]`
2. Если строка начинается с `Dealt to`, но не соответствует ни явному, ни hidden-формату:
   - надо materialize-ить explicit parse issue/error.
3. Для hidden hero dealt line:
   - сохранять `hero_name`;
   - `hero_hole_cards = None`.

---

### P1-03. `stacks_after_actual` сейчас не являются строго exact-выводом
**Уверенность:** высокая  
**Статус:** подтверждённая архитектурная проблема контракта  
**Критичность:** высокая

**Файлы:**
- `backend/crates/tracker_parser_core/src/normalizer.rs:77-86`

**Суть проблемы**

Сейчас:
- final stacks считаются через `observed_payouts.best_effort_totals()`;
- а `best_effort_totals()` при конфликте evidence берёт collect payouts и не требует exact selected settlement.

То есть поле `stacks_after_actual` звучит как canonical actual result, но на конфликтных surfaces это фактически:
- partly observed;
- partly best-effort;
- не обязательно exact.

**Почему это плохо**
Это может очень легко отравить downstream-логику:
- derived stats;
- economics;
- bust accounting;
- debugging UI.

Потребитель будет считать, что это финальная точная бухгалтерия руки, хотя это не всегда так.

**Что исправлять**
Развести как минимум на два слоя:
1. `observed_payout_totals`
2. `exact_selected_payout_totals` (nullable / optional)
3. `stacks_after_observed`
4. `stacks_after_exact` (nullable / optional)

И перестать называть best-effort результат exact-подобным именем.

---

### P1-04. Header/parser surface пока слишком жёстко зашит под узкий committed format
**Уверенность:** высокая  
**Статус:** не «баг на committed pack», но серьёзное ограничение production readiness  
**Критичность:** высокая

**Файлы:**
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:104-116`
- `backend/crates/tracker_parser_core/src/parsers/tournament_summary.rs:37-45`
- `backend/crates/tracker_parser_core/src/parsers/tournament_summary.rs:132-145`

**Суть проблемы**
Парсеры очень жёстко завязаны на конкретные строки:
- `Level\d+`
- точный формат header line
- точный формат table line
- title line summary-а с конкретным split по запятым
- точный `Buy-in: $x + $y + $z`

Это нормально для committed narrow alpha, но это **не** уровень общего production parser.

**Что здесь важно**
Я не считаю это «ошибкой алгоритма pot math».  
Я считаю это ключевым ограничением стадии проекта.  
Пока parser должен позиционироваться как:
- **exact on committed surface**
- **narrow outside it**

---

## 4.3. P2 — контрактные / документационные / coverage issues

### P2-01. Position label taxonomy требует явного публичного контракта
**Уверенность:** высокая  
**Статус:** не баг order-логики, а контрактный риск  
**Критичность:** средняя

**Файлы:**
- `backend/crates/tracker_parser_core/src/positions.rs:153-229`
- `backend/crates/tracker_parser_core/tests/positions.rs:32-104`

**Суть**
Порядок мест корректен, но labels выбраны по логике «distance from button / retained late-position names».

Примеры:
- 6-handed first-to-act маркируется как `LJ`, а не `UTG`;
- 5-handed first-to-act — как `HJ`;
- 2-handed button маркируется как `BTN`, а не `BTN/SB`.

Это может быть осознанным решением. Но тогда:
- это должно быть **явно задокументировано**;
- downstream-статы не должны молча интерпретировать это как общепринятый naming.

**Вывод**
Это не причина менять order-алгоритм.  
Это причина жёстко зафиксировать taxonomy contract.

---

### P2-02. Rule surface для `PostDead` ещё недостаточно расширен тестами
**Уверенность:** средняя  
**Статус:** coverage gap  
**Критичность:** средняя

**Файлы:**
- `backend/crates/tracker_parser_core/src/betting_rules.rs:171-177`
- `backend/crates/tracker_parser_core/src/betting_rules.rs:483-486`

**Суть**
Сейчас `PostDead` учитывается как forced betting post и участвует в `current_to_call` как blind-contribution.

На покрытых кейсах из committed/edge pack это выглядит нормально.  
Но rule surface здесь всё ещё узкий:
- dead blind == BB;
- dead blind + ante;
- fold sequences around dead blind.

Нужен отдельный adversarial matrix на разные dead-blind/missed-blind surfaces, иначе тут остаётся зона риска.

---

### P2-03. Статус- и roadmap-документация уже частично устарела
**Уверенность:** высокая  
**Статус:** подтверждённая документационная проблема  
**Критичность:** средняя

**Проявления**
- верхний README оценивает проект как систему с нереализованным web/API слоем и mock UI;
- backend README уже описывает поднятые upload/status и FT analytics slices;
- верхний README говорит о 3 Rust-crates, но по текущему `backend/crates/` их больше.

**Почему это важно**
Стадия проекта и backlog сейчас читаются не из одного источника правды.  
Это мешает и людям, и код-агентам.

---

## 5. Итог по ключевым алгоритмам из вашего запроса

### 5.1. Кто какой side pot забирает
**Вердикт:** базовый подход хороший.

Почему:
- банки строятся правильно по уровням вклада;
- folded players не остаются eligible;
- ambiguous hidden showdown не форсится в guessed winners;
- odd chip обрабатывается аккуратно через candidate allocations.

**Но**
Я бы не называл это «безошибочно готовым», пока не закрыты:
- P0 underflow/overflow hardening;
- P1 final-stack exactness contract;
- P2 dead-blind coverage matrix.

---

### 5.2. Кто сколько внёс
**Вердикт:** на покрытом committed surface логика в целом хорошая.

Почему:
- `committed_total` и `committed_by_street` replay-ятся последовательно;
- returns вычитаются обратно;
- по тестам покрыты uncalled-return и несколько tricky lines.

**Но**
Сейчас есть критический hardening gap:
- malformed action/refund может испортить committed-состояние и увести его в минус.

---

### 5.3. Кто в авто-all-in на префлопе из-за анте/блайндов
**Вердикт:** реализовано хорошо для покрытых surface-ов.

Хорошо:
- есть distinction между `AnteExhausted`, `BlindExhausted`, `CallExhausted`, `RaiseExhausted`, `Voluntary`;
- есть флаг `forced_all_in_preflop`;
- есть тесты на blind/ante exhaustion.

---

### 5.4. Последовательность действий всех игроков
**Вердикт:** legality engine выглядит сильной частью текущего exact-core.

Хорошо:
- HU preflop/postflop order;
- reopen после full raise;
- no reopen после short all-in;
- premature street close;
- illegal actor order.

Главный риск здесь не в самой идее, а в том, что corrupted surface сейчас может повредить state вместо безопасного stop/fail.

---

### 5.5. Позиции
**Вердикт:** seat/action ordering корректен, taxonomy labels нужно явно зафиксировать.

То есть:
- как порядок действий — выглядит хорошо;
- как словарь названий позиций — нужен чёткий публичный контракт, иначе downstream легко ошибётся в трактовке.

---

## 6. Что я бы запретил менять код-агенту

Это важно. Ниже — вещи, которые сейчас выглядят правильными и которые легко случайно испортить «упрощением»:

- **не убирать** `certainty_state`;
- **не заменять** uncertain/inconsistent на guessed exact;
- **не выкидывать** folded contributors из `pot.contributions`;
- **не смешивать** parser и normalizer обратно в один слой;
- **не ломать** separate legality pass;
- **не переписывать** pot construction «по collect lines» вместо contribution-level decomposition.

---

## 7. План исправлений и продолжения разработки для код-агента

Ниже — фазовый backlog.  
Формат: одна задача = один логический PR/итерация.  
Порядок менять нельзя: сначала P0, потом P1, потом P2.

---

# ФАЗА 0 — стоп на новые фичи, фиксация контракта exact-core

## Задача P0-1. Запретить отрицательные состояния replay/state
**Цель:** сделать невозможными отрицательные значения в любом replay-pass.

**Что делать**
1. Вынести единые функции мутации денег:
   - `apply_debit(...)`
   - `apply_refund(...)`
2. Использовать их в:
   - all-in annotation pass;
   - legality engine;
   - normalizer replay.
3. Добавить новые issue/error codes:
   - `action_amount_exceeds_stack`
   - `refund_exceeds_committed`
   - `refund_exceeds_betting_round_contrib`
4. Если найден overflow/underflow:
   - не продолжать мутацию state как будто всё нормально;
   - hand должна переходить в fail-safe режим.

**Чек-лист приёмки**
- [ ] Ни один путь не допускает `stack_current < 0`.
- [ ] Ни один путь не допускает `committed_total < 0`.
- [ ] Ни один путь не допускает `committed_by_street[*] < 0`.
- [ ] Ни один путь не допускает `betting_round_contrib < 0`.
- [ ] На malformed overbet/overcall/overraise/refund hand получает явный issue/error.
- [ ] Все существующие committed/golden тесты сохраняют прежний зелёный статус.

---

## Задача P0-2. Добавить adversarial regression fixtures на state-safety
**Цель:** не дать P0-1 регрессировать.

**Что делать**
Добавить отдельный набор synthetic HH fixtures минимум на такие кейсы:
1. call больше остатка стека;
2. bet больше остатка стека;
3. raise-to, требующий delta больше остатка;
4. uncalled return больше overage;
5. uncalled return больше `committed_total`;
6. uncalled return больше `betting_round_contrib`.

**Чек-лист приёмки**
- [ ] Каждый кейс имеет fixture.
- [ ] Каждый кейс имеет explicit expected issue/error contract.
- [ ] Во всех кейсах после normalizer нет отрицательных state-values.
- [ ] Golden/manifest-тесты расширены под новые surface-ы.

---

# ФАЗА 1 — исправление контрактов parser/normalizer

## Задача P1-1. Починить контракт `Call.amount` / `Call.to_amount`
**Цель:** убрать семантическую ловушку из canonical action model.

**Что делать**
Выбрать один из двух вариантов и довести до конца:

### Вариант A
- оставить `amount` как delta;
- для `Call` ставить `to_amount = None`;
- `to_amount` использовать только для `RaiseTo`.

### Вариант B
- заменить текущую схему на явную:
  - `delta_amount`
  - `target_contribution`
- мигрировать все тесты и сериализацию.

**Рекомендация**
Я бы выбрал **вариант A** как минимально взрывоопасный.

**Чек-лист приёмки**
- [ ] В кодовой базе больше нет места, где `Call.to_amount` трактуется как pseudo-target.
- [ ] Edge-matrix обновлён.
- [ ] Документация canonical action model обновлена.
- [ ] Сериализуемый контракт не двусмысленен.

---

## Задача P1-2. Починить `Dealt to ...` parsing fallback
**Цель:** перестать молча съедать malformed dealt-lines и сохранить hero identity на hidden surface.

**Что делать**
1. Сделать hidden dealt regex строгим:
   - только `Dealt to <player>$`;
   - без card-surface.
2. Если строка начинается с `Dealt to`, но не распарсилась ни как explicit, ни как hidden:
   - это явный parse issue/error.
3. На hidden hero line:
   - сохранять `hero_name`;
   - `hero_hole_cards = None`.

**Чек-лист приёмки**
- [ ] malformed dealt line не проходит молча.
- [ ] hidden hero dealt surface не ломает `normalize_hand`.
- [ ] committed fixtures остаются без новых неожиданных warnings.
- [ ] Есть новые тесты на malformed dealt-lines.

---

## Задача P1-3. Развести observed и exact final-stack contracts
**Цель:** перестать выдавать best-effort output за exact final accounting.

**Что делать**
Разделить выход нормализатора на:
- `observed_winner_collections`;
- `exact_selected_payout_totals` (optional);
- `stacks_after_observed`;
- `stacks_after_exact` (optional).

Дополнительно:
- если settlement `Uncertain/Inconsistent`, exact-stack слой не должен маскироваться под финальный truth.

**Чек-лист приёмки**
- [ ] Конфликтные руки не отдают псевдо-exact `stacks_after_actual`.
- [ ] Потребители могут явно выбрать observed vs exact.
- [ ] Existing tests на conflict/uncertain surfaces обновлены.
- [ ] Названия полей больше не вводят в заблуждение.

---

## Задача P1-4. Расширить committed syntax contract вокруг header/summary parser
**Цель:** уменьшить хрупкость regex-bound surface.

**Что делать**
1. Вынести syntax-cases в явный каталог:
   - header variants;
   - summary title variants;
   - buy-in variants;
   - table line variants.
2. Добавить triage fixtures на вариативность surface-а.
3. Если surface не exact-supported:
   - это должен быть reason-coded parse issue, а не немая потеря данных.

**Чек-лист приёмки**
- [ ] У каждого supported format есть fixture.
- [ ] Unsupported variants reason-coded.
- [ ] Wide corpus triage показывает численное покрытие по reason-codes.
- [ ] Нет silent-swallow сценариев.

---

## Задача P1-5. Довести rule-matrix по `PostDead` и missed blind surfaces
**Цель:** снять риск ложной exactness на dead-blind ветке.

**Что делать**
Добавить synthetic/real fixtures минимум на:
- dead blind == BB;
- dead blind < BB;
- dead blind + ante;
- short dead blind all-in;
- dead blind + fold before action closes;
- multiway surface around dead blind.

**Чек-лист приёмки**
- [ ] Есть отдельный test-module под dead-blind matrix.
- [ ] Для каждого кейса зафиксированы:
  - action order
  - current_to_call
  - committed totals
  - pot totals
  - invariants
- [ ] Не осталось неописанного поведения вокруг `PostDead`.

---

# ФАЗА 2 — качество контракта и CI-gates

## Задача P2-1. Жёстко зафиксировать публичный контракт по позициям
**Цель:** исключить downstream-путаницу в трактовке labels.

**Что делать**
1. Решить и задокументировать:
   - labels отражают «distance from button» или «first-to-act naming».
2. Если выбран текущий вариант:
   - написать это явно в docs;
   - downstream stats не должны маппить `LJ` в `UTG` молча.
3. Если нужен standard naming:
   - добавить отдельное поле standard label и не ломать existing order indices.

**Чек-лист приёмки**
- [ ] Есть один источник правды по taxonomy.
- [ ] Тесты на 2–10 handed прямо подтверждают выбранный контракт.
- [ ] UI/derived-layer не делают скрытых reinterpretation-ов.

---

## Задача P2-2. Синхронизировать README/STATUS/ROADMAP с реальным деревом репозитория
**Цель:** чтобы код-агент и люди читали одну и ту же стадию проекта.

**Что делать**
1. Свести в один актуальный источник:
   - текущие crates;
   - текущие vertical slices;
   - текущий stage estimate;
   - что exact only on committed pack;
   - что ещё не production-grade.
2. Удалить противоречия между:
   - root README;
   - backend README;
   - roadmap/status docs.

**Чек-лист приёмки**
- [ ] Количество crates и vertical slices в docs совпадает с деревом репозитория.
- [ ] Текущая стадия проекта описана одинаково во всех entry docs.
- [ ] Narrow alpha vs production coverage формулируются одинаково.

---

## Задача P2-3. Подключить wide-corpus triage как quality gate
**Цель:** перестать измерять parser coverage «на глаз».

**Что делать**
1. Подключить `wide_corpus_triage` в CI/quality gates.
2. Определить пороги:
   - unexpected unparsed lines
   - parse issue severity distribution
   - new syntax drift detection
3. Ломать PR, если exact-surface регрессировал.

**Чек-лист приёмки**
- [ ] Есть воспроизводимый отчёт по wide corpus.
- [ ] Есть baseline и пороги регрессии.
- [ ] PR не может тихо ухудшить parser coverage.

---

# ФАЗА 3 — только после P0/P1/P2

## Задача P2-4. Продолжение продуктовой разработки
Это уже можно делать только после стабилизации exact-core.

**Разрешённые направления после стабилизации**
- hand/drilldown API;
- hand/street explorer;
- derived-layer/stat-layer расширение;
- web upload/auth hardening;
- перенос следующего слоя MBR stats.

**Чек-лист приёмки**
- [ ] Exact-core больше не имеет открытых P0.
- [ ] Контракты parser/normalizer/positions задокументированы.
- [ ] Wide-corpus gate подключён.
- [ ] Docs синхронизированы.

---

## 8. Порядок работы код-агента

Чтобы не устроить хаос, агенту нужно работать так:

1. **Сначала** создать safety-regression fixtures.
2. **Потом** чинить state underflow/overflow.
3. **Потом** чинить `Call.to_amount`.
4. **Потом** чинить `Dealt to` fallback.
5. **Потом** разводить observed vs exact final stacks.
6. **Потом** расширять syntax/dead-blind coverage.
7. **Потом** синхронизировать docs и quality gates.
8. **Только после этого** возвращаться к новым фичам.

### Что нельзя делать агенту
- смешивать несколько фаз в один гигантский PR;
- переписывать pot math «с нуля»;
- убирать `certainty_state`;
- заменять `uncertain/inconsistent` на guess;
- менять position order indices без явной миграции контракта.

---

## 9. Мой итоговый приоритетный backlog

### P0
- P0-1 Запрет отрицательных состояний replay/state
- P0-2 Adversarial regression fixtures на underflow/overflow/refund

### P1
- P1-1 Починка контракта `Call.to_amount`
- P1-2 Починка `Dealt to` fallback и hidden hero support
- P1-3 Развод observed/exact final stacks
- P1-4 Расширение syntax contract вокруг header/summary parser
- P1-5 Dead-blind rule matrix

### P2
- P2-1 Публичный контракт по taxonomy positions
- P2-2 Синхронизация docs/status/roadmap
- P2-3 Wide-corpus triage как CI gate
- P2-4 Только после этого — новые продуктовые фичи

---

## 10. Финальный вывод

### Что уже можно сказать уверенно
- exact-core здесь **реально есть**;
- pot-resolution в основе **не игрушечный**;
- side-pot / split / odd-chip / hidden-showdown philosophy сделаны в правильную сторону;
- short all-in, forced all-in by blind/ante, actor order и positions-order продуманы заметно лучше среднего раннего прототипа.

### Что нельзя пока говорить
Нельзя пока говорить:
- что parser/normalizer production-grade;
- что широкому GG corpus можно доверять без CI-gated triage;
- что в core «нет ни одной ошибки».

### Мой инженерный вердикт
Сейчас проект стоит воспринимать так:

> **Не как готовый трекер и не как production parser, а как хороший узкий foundation exact-core, который уже достоин дальнейшего усиления — но только через жёсткий P0/P1 hardening, а не через наращивание новых фич поверх текущих контрактных дыр.**



---

## 11. Минимальный набор прогонов после каждой фазы

Код-агенту нужно гонять не «что-нибудь», а один и тот же фиксированный набор:

```bash
cd backend
cargo test -p tracker_parser_core
cargo test -p tracker_parser_core --test fixture_parsing
cargo test -p tracker_parser_core --test hand_normalization
cargo test -p tracker_parser_core --test phase0_exact_core_corpus
cargo test -p tracker_parser_core --test pot_math_properties
cargo test -p tracker_parser_core --test positions
bash scripts/run_wide_corpus_triage.sh
```

### Отдельно обязательно прогонять после P0/P1
- synthetic fixtures на overcall / overbet / overraise;
- synthetic fixtures на refund overflow;
- malformed `Dealt to ...` fixtures;
- hidden hero dealt fixture;
- dead-blind matrix fixtures;
- обновлённые golden snapshots.

### Условие merge
PR не должен мержиться, если:
- появился новый unexpected parse issue на committed pack;
- появился хоть один отрицательный state-value;
- changed golden не объяснён в PR description;
- exact/uncertain/inconsistent semantics поменялись без обновления docs и acceptance-manifest.
