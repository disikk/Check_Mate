# Аудит репозитория Check_Mate — стадия проекта, корректность парсинга/нормализации и план продолжения
Дата аудита: 2026-03-25

## 1. Краткий вердикт

Проект сейчас находится на стадии **foundation / narrow alpha**.

Это не пустой каркас: в репозитории уже есть рабочий узкий контур
`парсер -> нормализатор -> persistence -> derived facts`, есть committed-пакет
фикстур, есть тесты на инварианты, есть synthetic edge-cases для split / side-pot /
ambiguous mapping. Но до уровня, при котором можно говорить
«в парсинге и нормализации не должно быть ни единой ошибки», проект **не дошёл**.

Главная причина: самые критичные доменные вещи пока либо не реализованы вовсе,
либо реализованы только как узкий компромисс для committed GG-пака:

1. **нет полноценного движка позиций** игроков;
2. **нет строгого движка легальности очередности действий**;
3. **summary seat-result lines намеренно игнорируются**, хотя именно там есть важные
   семантические данные;
4. **распределение winners по pot-ам сейчас выводится обратным подбором по aggregated collected amounts**,
   а не из детерминированной exact-модели банка;
5. **KO attribution упрощён** и не покрывает совместный bust через несколько pot-ов;
6. **forced auto-all-in от blind/ante не оформлен как отдельная доказанная доменная логика**;
7. **часть синтаксических классов вообще не поддержана** (`PostDead`, `Muck`, no-show / partial reveal и др.).

### Моя оценка по слоям

- Система в целом: **25–30%**
- Парсер HH/TS на committed GG-паке: **55–60%**
- Парсер как общий production parser по реальному корпусу: **30–35%**
- Normalizer exact-core: **35–40%**
- Вычисление позиций: **0–10%** (по сути отсутствует)
- Exact action-order / betting legality: **0–15%** (по сути отсутствует)
- Exact pot / side-pot / KO attribution engine: **40–45%** на покрытых кейсах, но **не replay-grade**
- MBR stage / economics: **15–20%**
- Product layer (ingest/API/UI/stats): **далеко от готовности**

## 2. Что уже сделано хорошо

### 2.1. Архитектурно

Сильная сторона проекта — правильный вектор архитектуры:

- есть явный source-of-truth слой;
- parser и normalizer выделены отдельно;
- derived facts вынесены отдельно;
- ambiguous winner mappings не «угадываются», а оставляются uncertain;
- exact-core пытаются проверять инвариантами, а не только UI-результатом.

### 2.2. Что выглядит корректным уже сейчас

1. **Committed GG-пакет реально закрыт узким parser-покрытием.**
   По текущему паку parser/tests выглядят собранно.

2. **Normalizer уже умеет базовые exact-вещи:**
   - total committed by player;
   - final pots;
   - pot contributions;
   - uncalled returns;
   - chip conservation;
   - pot conservation;
   - split / side-pot synthetic cases.

3. **Есть правильная политика неопределённости.**
   Если mapping winner collections -> per-pot winners неоднозначен, код сейчас
   не materialize-ит guessed winners, а оставляет hand uncertain.

4. **Схема БД уже усилена migration v2.**
   Composite FK-контракты для hand-seat / hand-pot дочерних таблиц уже добавлены,
   это хороший знак для exact-core.

## 3. Подтверждённые проблемы

Ниже — именно подтверждённые проблемы / пробелы, а не «возможные пожелания».

---

## P0-01. Нет полноценного движка позиций игроков

### Что подтверждено
В текущем коде и схеме есть:
- `seat_no`;
- `button_seat`;
- `is_button`.

Но **нет** доменной модели позиций вида:
- BTN
- SB
- BB
- UTG
- UTG+1
- LJ
- HJ
- CO
- heads-up special case

Также нет отдельной таблицы/поля с вычисленным `position_code`.

### Почему это критично
Пользовательский запрос прямо требует:
- позиции каждого игрока;
- корректную очередность действий;
- корректную трактовку short-handed и heads-up.

Без position engine нельзя честно утверждать, что:
- позиция игрока определена;
- preflop order корректен;
- postflop order корректен;
- heads-up обрабатывается правильно;
- short-handed 9-max final table без дырок размечается правильно.

### Где видно в коде
- `backend/crates/tracker_parser_core/src/models.rs:22-34`
- `backend/crates/tracker_parser_core/src/models.rs:72-90`
- `backend/migrations/0001_init_source_of_truth.sql:157-217`
- `backend/migrations/0004_exact_core_schema_v2.sql` — position-поля не добавлены

### Риск
Очень высокий. Любая статистика по позициям, open/fold/call/3bet, steal, defend,
c-bet-by-position и т.п. сейчас либо невозможна, либо будет построена на догадках.

---

## P0-02. Нет строгого движка легальности очередности действий

### Что подтверждено
Нормализатор воспроизводит денежный ledger, но **не валидирует**:
- кто должен ходить следующим;
- кто может открыть торги;
- когда action reopened;
- когда short all-in **не** reopen-ит action;
- когда call/check закрывает betting round;
- кто first-to-act postflop;
- heads-up special rule (BTN posts SB and acts first preflop, last postflop).

### Почему это критично
Пользователь прямо требует:
- чёткую последовательность действий всех игроков;
- учёт auto-all-in;
- корректность кто сколько внёс;
- side-pot correctness.

Без legality engine можно иметь chip/pot invariants = green, но при этом:
- action order семантически неверный;
- reopen semantics неверна;
- позиционная трактовка неверна;
- derived stats будут искажены.

### Где видно в коде
- `backend/crates/tracker_parser_core/src/normalizer.rs:225-385`
- `backend/crates/tracker_parser_core/src/normalizer.rs:415-470`

Код ведёт учёт денег, но не держит полноценное состояние betting rules:
`to_call`, `last_full_raise_size`, `last_aggressor`, `reopened`, `eligible_to_act`,
`street starter`, `street closer`.

### Риск
Очень высокий.

---

## P0-03. Summary seat-result lines сейчас намеренно игнорируются

### Что подтверждено
Строки вида:

- `Seat 7: Hero (big blind) showed [Qh Kh] and lost with a pair of Kings`
- `Seat 2: Villain (small blind) folded before Flop`
- `Seat 1: Player (button) showed [...] and won (...)`

на committed-паке **намеренно игнорируются**.

Parser не поднимает их в structured model и даже не считает warning-ами,
если строка начинается с `Seat `.

### Почему это критично
Именно эти строки часто несут важную семантику:
- button / small blind / big blind;
- folded before flop / on turn / on river;
- showed / mucked / won / lost;
- итоговое textual подтверждение результата раздачи.

То есть текущие «0 unexpected warnings на committed pack» **не означают**
полную семантическую закрытость hand history.
Часть важных данных сейчас просто проходит мимо.

### Где видно в коде
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:111-196`
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:194-196`
- `docs/COMMITTED_PACK_SYNTAX_CATALOG.md` — summary seat outcome prose explicitly marked as intentionally ignored

### Риск
Очень высокий. Это прямой разрыв между «syntactic pass» и «semantic completeness».

---

## P0-04. Exact pot / side-pot winners сейчас выводятся обратным подбором по collected sums

### Что подтверждено
Текущий normalizer:
1. строит pot layers по committed totals;
2. затем пытается разложить aggregated `collected_amounts` по pot-ам через combinatorial search;
3. если решение одно — считает mapping exact;
4. если решений несколько — hand остаётся uncertain.

### Почему это критично
Это разумный временный компромисс, но это **не** replay-grade exact engine.

Проблема не в том, что решение всегда неверно. Проблема в том, что:
- exact winner attribution не выводится из полной модели action/pot/showdown semantics;
- exact mapping иногда недоопределён;
- часть semantic constraints из summary/showdown lines вообще не используется;
- код не знает реального порядка и причин появления pot-ов, а реконструирует их
  из финальных totals.

Для узкого пакета этого достаточно часто хватает.
Для широкого реального корпуса — этого мало.

### Где видно в коде
- `backend/crates/tracker_parser_core/src/normalizer.rs:473-551`
- `backend/crates/tracker_parser_core/src/normalizer.rs:554-755`

### Риск
Очень высокий для задач вида:
- кто именно забрал какой side pot;
- exact KO attribution;
- сложные split-sidepot-showdown случаи;
- сложные ambiguous mappings.

---

## P0-05. KO attribution упрощён до `max(pot_no)` и не покрывает multi-pot joint bust

### Что подтверждено
Elimination сейчас строится так:
- берётся `max(pot_no)`, в который busted player вносил деньги;
- winners этого pot-а считаются KO-involved;
- дальше на этом строятся `hero_involved`, `hero_share_fraction`, `split_n`, `is_sidepot_based`.

### Почему это критично
Это **неполная** модель bust semantics.

Контрпример:
- игрок проигрывает main pot одному оппоненту;
- side pot — другому;
- суммарно это и выбивает его из турнира.

Текущая модель может отдать KO только winners самого высокого pot-а,
хотя фактически bust произошёл из-за потери **нескольких** pot-ов
разным игрокам.

Следствие:
- `hero_involved` может быть занижен;
- `hero_share_fraction` может быть концептуально неверным;
- `split_n` и `ko_involved_winner_count` не описывают совместный bust по нескольким pot-ам.

### Где видно в коде
- `backend/crates/tracker_parser_core/src/normalizer.rs:757-824`

### Риск
Очень высокий для KO-статов и MBR-аналитики.

---

## P0-06. Forced auto-all-in от blind/ante пока не доведён до отдельной доменной модели

### Что подтверждено
Сейчас status становится `AllIn`, если:
- line содержит `and is all-in`, или
- после forced post стек стал 0.

Это лучше, чем ничего.
Но нет:
- отдельного `all_in_reason`;
- признака `forced_all_in_preflop`;
- различения `voluntary all-in` vs `forced blind all-in` vs `forced ante all-in`;
- dedicated test-матрицы именно на forced all-in by blind/ante exhaustion.

### Почему это критично
Пользовательский запрос прямо требует:
- кто ушёл в auto-all-in на префлопе из-за короткого стека и уплаты blind/ante.

Текущая логика **может** вычислить конечный `AllIn`,
но не даёт полноценного доменного ответа:
- по какой причине;
- на каком forced post это случилось;
- была ли у игрока возможность сделать действие позже;
- является ли это отдельным типом префлоп-ситуации.

### Где видно в коде
- `backend/crates/tracker_parser_core/src/normalizer.rs:240-350`
- покрывающих dedicated-тестов на blind/ante auto-all-in не найдено

### Риск
Высокий.

---

## P0-07. Parser surface всё ещё не закрывает важные классы строк

### Что подтверждено
В models/action enum есть:
- `PostDead`
- `Muck`

Но parser сейчас реально поддерживает:
- ante
- SB
- BB
- fold
- check
- call
- bet
- raise-to
- return
- show
- collect

При этом:
- `posts dead` не парсится;
- `mucks` не парсится;
- roadmap сам отдельно перечисляет `no-show / muck / partial reveal`;
- sit-out классы тоже не подняты в exact model.

### Почему это критично
Как только corpus расширится за пределы committed-пака,
появятся реальные edge-синтаксисы, которые:
- либо выпадут в `unparsed_line`,
- либо вообще не будут подняты семантически.

Для exact-core это неприемлемо.

### Где видно в коде
- `backend/crates/tracker_parser_core/src/models.rs:53-69`
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:406-541`
- `docs/ROADMAP.md` — showdown variants / summary seat-result / no-show / muck / partial reveal перечислены как дальнейшая работа

### Риск
Высокий.

---

## P1-01. `is_sitting_out` есть в схеме, но parser не доказывает этот признак

### Что подтверждено
В схеме есть `is_sitting_out`.
Но текущий parser seat-line поднимает только:
- seat_no
- player_name
- starting_stack

Никакой structured sit-out semantics нет.

### Почему это важно
Sit-out влияет на:
- positions;
- forced posts;
- action order;
- eligibility.

### Где видно
- `backend/migrations/0001_init_source_of_truth.sql:177-189`
- `backend/crates/tracker_parser_core/src/models.rs:72-76`
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:248-260`

### Риск
Средне-высокий.

---

## P1-02. Hand header и TS parser слишком жёсткие для production corpus

### Что подтверждено
- HH header regex жёстко ждёт `Level\d+` и конкретный shape table line.
- TS parser жёстко опирается на первые 6 строк и на buy-in из ровно 3 частей.

### Почему это важно
Для committed-пака это ок.
Для production parser — хрупко.

### Где видно
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:53-90`
- `backend/crates/tracker_parser_core/src/parsers/tournament_summary.rs:5-61`

### Риск
Средний.

---

## P1-03. `Call.to_amount` сейчас семантически двусмыслен

### Что подтверждено
Для `Call` поле `to_amount` заполняется значением самого колла, а не итоговым target level.
Normalizer это сейчас не использует для Call, поэтому прямого падения нет.
Но контракт поля уже выглядит опасно.

### Почему это важно
Когда появится полноценный legality/action engine, это почти наверняка станет источником ошибок
или путаницы.

### Где видно
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs:485-501`

### Риск
Средний.

---

## P1-04. Тестовое покрытие хорошее для committed-пака, но не доказательное для exact-core в широком корпусе

### Что подтверждено
Есть хорошие тесты на:
- committed pack parse warnings;
- committed pack invariants;
- split KO;
- side-pot KO;
- repeated collect;
- ambiguous mapping;
- unsatisfied collect mapping.

Но не найдено полноценного покрытия на:
- dead blind;
- muck;
- sit-out;
- blind/ante auto-all-in;
- short all-in not reopening action;
- heads-up special acting order;
- multi-pot joint bust;
- explicit position assignment matrix;
- summary seat-result structured parse.

### Почему это важно
Для exact-core отсутствие тестов на эти доменные случаи само по себе уже блокер.

### Где видно
- `backend/crates/tracker_parser_core/tests/fixture_parsing.rs`
- `backend/crates/tracker_parser_core/tests/hand_normalization.rs`

### Риск
Высокий.

---

## P1-05. MBR stage / boundary слой пока placeholder

### Что подтверждено
Есть явный legacy-heuristic:
- boundary candidate = последняя 5-max hand перед первой 9-max hand;
- `boundary_ko_method = legacy_pre_ft_candidate_v1`;
- в `build_mbr_stage_resolutions()` `exact_hero_boundary_ko_share` сейчас изначально подставляется как `Some(0.0)`.

### Почему это важно
Этот слой downstream-зависим от exact-core.
Его можно оставлять только как временный foundation placeholder.

### Где видно
- `backend/crates/parser_worker/src/local_import.rs:1255-1321`

### Риск
Средний для ядра, высокий для MBR analytics.

---

## 4. Что это значит practically

### Сейчас можно считать надёжным
- узкий committed GG-пакет парсится;
- committed-пакет держит базовые инварианты;
- simple/synthetic split/sidepot cases уже не разваливают normalizer;
- система старается не угадывать winners в ambiguous кейсах.

### Сейчас нельзя считать доказанным
- позиции каждого игрока;
- exact последовательность действий по legal order;
- exact reopen semantics;
- forced all-in preflop как отдельный тип ситуации;
- универсальную корректность side-pot winners по широкому корпусу;
- универсальную корректность KO attribution;
- production-grade GG parser;
- production-grade MBR stage/economics.

## 5. Жёсткий порядок продолжения разработки

## Правило
**Не делать новые статы, popup/HUD, UI-фичи и расширение runtime-слоя, пока не закрыт exact-core.**

Ниже — рекомендованный порядок задач для код-агента.

---

# Фаза 0. Exact-core hardening — обязательно закрыть первой

## TASK CM-01 — Structured parse summary seat-result lines
**Priority:** P0

### Цель
Перестать молча игнорировать summary seat-result prose и поднять её в структурированную модель.

### Что сделать
1. Добавить новую модель `SummarySeatOutcome` / `SeatResultLine`.
2. Парсить как минимум:
   - seat_no
   - player_name
   - button/sb/bb marker если есть
   - showed / mucked / won / lost / folded
   - folded_before_flop / folded_on_flop / folded_on_turn / folded_on_river
   - shown_cards (если есть)
   - won_amount (если есть)
   - textual_hand_class (если есть)
3. Сохранять эти строки в canonical hand.
4. Persist-ить их в отдельную таблицу, не смешивая с raw actions.
5. Использовать их как дополнительные constraints для normalizer / KO layer.

### Файлы, которые почти наверняка затронутся
- `backend/crates/tracker_parser_core/src/models.rs`
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs`
- `backend/crates/parser_worker/src/local_import.rs`
- новая migration для `core.hand_summary_results` (или аналог)
- тесты parser и integration tests

### Чек-лист приёмки
- [ ] Ни одна summary seat-result line из committed и extended corpus не игнорируется молча
- [ ] Каждая line либо структурно распарсена, либо даёт parse issue с reason code
- [ ] Для committed pack нет regression по existing tests
- [ ] Есть synthetic tests минимум на:
  - [ ] `folded before Flop`
  - [ ] `folded on the Turn`
  - [ ] `showed ... and lost`
  - [ ] `showed ... and won (...)`
  - [ ] `(button)`, `(small blind)`, `(big blind)`

---

## TASK CM-02 — Ввести position engine
**Priority:** P0

### Цель
Для каждой раздачи вычислять **позицию каждого активного игрока на старте hand**.

### Что сделать
1. Определить доменную модель:
   - `table_position_code`
   - `preflop_act_order_index`
   - `postflop_act_order_index`
2. Поддержать:
   - 9-max full ring;
   - short-handed 2..9 active seats;
   - heads-up special case;
   - исключение игроков со stack=0 / sitting out / не участвующих в hand start.
3. Position engine должен быть чистой функцией:
   - вход: occupied seats, button seat, active flags;
   - выход: static positions + act-order indexes.
4. Persist-ить position facts отдельно.

### Файлы
- новый модуль `positions.rs`
- `models.rs`
- importer
- migration для `core.hand_positions` или columns в `core.hand_seats`
- tests

### Чек-лист приёмки
- [ ] Для каждой hand есть ровно одна позиция для каждого active seat
- [ ] Heads-up размечается корректно: BTN=SB, BB отдельно
- [ ] Для 3/4/5/6/7/8/9 active seats позиции детерминированы и покрыты тестами
- [ ] Есть отдельные golden tests на hand-start seat maps
- [ ] Position facts не строятся для `sitting_out` / dead seats
- [ ] Парсер/импортер не путает static position с current actor order

---

## TASK CM-03 — Ввести betting legality / action-order engine
**Priority:** P0

### Цель
Перестать считать только деньги и начать доказывать **правильность последовательности действий**.

### Что сделать
1. Добавить state machine с явными полями:
   - `current_to_call`
   - `last_full_raise_size`
   - `last_aggressor`
   - `eligible_to_act`
   - `street_opener`
   - `street_closer`
   - `action_reopened`
2. Валидации:
   - illegal actor;
   - illegal check;
   - illegal call amount;
   - incomplete raise;
   - short all-in that does not reopen;
   - overcall/undercall inconsistencies;
   - premature street close.
3. Отдельно обработать:
   - preflop order;
   - postflop order;
   - heads-up;
   - dead blind / missed blind, если встретится.

### Файлы
- новый модуль `betting_rules.rs` / `replay_engine.rs`
- `normalizer.rs`
- tests

### Чек-лист приёмки
- [ ] Каждый action проходит через legal-actor validation
- [ ] Для illegal sequence hand не silently passes
- [ ] Есть отдельные tests на:
  - [ ] heads-up preflop order
  - [ ] heads-up postflop order
  - [ ] limp / raise / call chains
  - [ ] short all-in that does not reopen action
  - [ ] full raise that reopens action
  - [ ] uncalled return after failed call chain
- [ ] Ошибки legality выдаются reason-coded, а не free-form строкой

---

## TASK CM-04 — Ввести explicit forced-all-in / dead-blind / muck / sit-out support
**Priority:** P0

### Цель
Закрыть критичные синтаксические и доменные edge-cases.

### Что сделать
1. Реально парсить:
   - `posts dead`
   - `mucks`
   - no-show / partial reveal variants
   - sit-out markers, если room их даёт
2. Ввести:
   - `all_in_reason = voluntary | call_exhausted | raise_exhausted | blind_exhausted | ante_exhausted`
   - `forced_all_in_preflop` flag
3. Поддержать forced posts, которые оставляют игроку 0 stack.

### Файлы
- `models.rs`
- `hand_history.rs`
- `normalizer.rs`
- importer
- tests

### Чек-лист приёмки
- [ ] `PostDead` и `Muck` реально приходят из parser, а не только существуют в enum
- [ ] Есть tests на blind-exhausted all-in и ante-exhausted all-in
- [ ] Sit-out не попадает в normal active order
- [ ] No-show / partial reveal не ломают exact-core и явно маркируются

---

## TASK CM-05 — Заменить reverse winner mapping на deterministic exact pot engine v2
**Priority:** P0

### Цель
Строить exact pot tree не как обратный подбор по финальным collected sums, а как доменную модель банка.

### Что сделать
1. Ввести явную сущность:
   - pot layers;
   - contributors;
   - eligibility set;
   - amount;
   - settlement evidence.
2. Разделить:
   - `pot construction`
   - `pot settlement`
3. Settlement должен опираться на максимум доступной информации:
   - action replay;
   - summary seat results;
   - showdown cards;
   - collected lines;
   - explicit uncertainty reasons, если exact недостаточно.
4. Не материализовать guessed winners никогда.

### Файлы
- `normalizer.rs`
- возможно новый модуль `pot_resolution.rs`
- tests
- importer / persistence layer

### Чек-лист приёмки
- [ ] Каждый final pot имеет exact contributors и eligible players
- [ ] Side pots детерминированно воспроизводятся из action ledger
- [ ] В ambiguous cases hand получает `uncertain_reason_codes`
- [ ] Нет ни одного guessed `hand_pot_winners`
- [ ] Есть tests на:
  - [ ] main + 1 side pot
  - [ ] main + 2 side pots
  - [ ] split main / single-winner side
  - [ ] repeated collect
  - [ ] reordered collect
  - [ ] odd-chip split
  - [ ] hidden showdown / partial reveal ambiguity

---

## TASK CM-06 — Переписать KO attribution v2
**Priority:** P0

### Цель
Сделать KO attribution корректным и способным описывать bust через несколько pot-ов.

### Что сделать
1. Уйти от текущего `resolved_by_pot_no = max(contributed pot_no)`.
2. Ввести модель:
   - `ko_resolved_by_pots[]`
   - `ko_involved_winners[]`
   - `hero_ko_share_total`
   - `joint_ko = true/false`
3. Поддержать сценарии:
   - single-pot KO;
   - split KO;
   - side-pot-only KO;
   - main+side joint KO разными winners.

### Файлы
- `normalizer.rs`
- derived persistence
- tests

### Чек-лист приёмки
- [ ] Есть тест на joint bust через main и side pot разным winners
- [ ] Hero involvement считается по всем bust-relevant pot-ам
- [ ] `hero_share_fraction` семантически определена и документирована
- [ ] Нет ложного attribution только по highest pot

---

## TASK CM-07 — Построить доказательный test corpus и quality gate exact-core
**Priority:** P0

### Цель
Закрыть не только committed pack, но и доменный edge-space.

### Что сделать
1. Собрать extended real corpus.
2. Отдельно собрать synthetic matrix:
   - positions;
   - heads-up;
   - dead blind;
   - ante all-in;
   - short all-in non-reopen;
   - muck/no-show;
   - joint KO;
   - multi-side-pot;
   - odd chip.
3. Зафиксировать golden outputs.
4. В CI ввести hard gate:
   - parser semantic coverage;
   - no silent ignores;
   - exact/uncertain contracts;
   - invariants.

### Чек-лист приёмки
- [ ] `warning-level parse issues = 0` на committed pack
- [ ] Все silent-ignore кейсы либо убраны, либо формально whitelisted как harmless prose
- [ ] Extended corpus даёт reason-coded unknowns
- [ ] Exact-core regression suite зелёная
- [ ] На каждом новом syntax pattern есть fixture + test

---

# Фаза 1. После exact-core — MBR/domain hardening

## TASK CM-08 — Tournament Summary parser v2 + economics normalization
**Priority:** P1

### Цель
Уйти от слишком узкого TS parser и довести economics до usable состояния.

### Что сделать
1. Ослабить жёсткую привязку к «ровно 6 строкам».
2. Ввести structured tail parser для полезной информации после result line.
3. Нормализовать:
   - buy-in
   - rake
   - bounty
   - regular prize
   - mystery money
   - total payout
4. Документировать все uncertainty states.

### Чек-лист приёмки
- [ ] TS parser не ломается от harmless trailing lines
- [ ] economics fields заполняются детерминированно или маркируются uncertain
- [ ] Есть regression tests на текущие 9 TS + новые edge fixtures

---

## TASK CM-09 — Заменить legacy boundary heuristic в MBR stage layer
**Priority:** P1

### Цель
Уйти от `legacy_pre_ft_candidate_v1` и placeholder-подхода.

### Что сделать
1. Формально описать, что такое boundary zone и что именно надо считать.
2. Не подставлять `exact_hero_boundary_ko_share = 0.0` как временную заглушку.
3. Ввести доказательную модель на exact-core данных.

### Чек-лист приёмки
- [ ] Нет placeholder-значений, маскирующих отсутствие exact evidence
- [ ] boundary method документирован
- [ ] certainty-state прозрачен
- [ ] есть synthetic + real tests

---

## TASK CM-10 — Persist-ить exact-core descriptors для downstream stat engine
**Priority:** P1

### Цель
Сделать exact-core реально пригодным для статистик, но не тащить в runtime догадки.

### Что сделать
Persist-ить:
- positions
- all-in reasons
- action legality facts
- uncertainty reason codes
- structured summary outcomes
- exact KO participants

### Чек-лист приёмки
- [ ] downstream stat-layer не зависит от raw parsing эвристик
- [ ] exact и uncertain facts различаются явно
- [ ] нет stat-ов, опирающихся на guessed facts

---

# Фаза 2. Только после exact-core и domain hardening

## TASK CM-11 — Перенос stat engine на exact-core
**Priority:** P2

### Цель
Строить статы уже поверх правильных positions/actions/pots/KO facts.

### Чек-лист приёмки
- [ ] ни один stat не читает raw action text напрямую
- [ ] все position-based stats идут от position engine
- [ ] все KO-based stats идут от KO v2
- [ ] все all-in stats идут от explicit all-in reason model

---

## TASK CM-12 — Production ingest / API / UI
**Priority:** P2

### Цель
Довести систему до продуктового контура только после того, как exact-core доказан.

### Чек-лист приёмки
- [ ] ingest не dev-only
- [ ] есть dedupe / retries / archive-members / queue contract
- [ ] API читает exact-core facts
- [ ] UI не показывает guessed data как exact

---

## 6. Итоговый приоритетный список для код-агента

### P0 — делать немедленно
1. CM-01 — structured summary seat-result parsing
2. CM-02 — position engine
3. CM-03 — betting legality / action-order engine
4. CM-04 — forced-all-in + dead blind + muck/no-show/sit-out support
5. CM-05 — deterministic pot engine v2
6. CM-06 — KO attribution v2
7. CM-07 — exact-core corpus + gates

### P1 — после закрытия exact-core
8. CM-08 — TS parser v2 + economics normalization
9. CM-09 — MBR boundary/stage v2
10. CM-10 — persist exact-core descriptors for stats

### P2 — только после P0/P1
11. CM-11 — stat engine migration
12. CM-12 — production ingest/API/UI

## 7. Самые важные выводы в одном месте

1. **На committed-паке проект уже не сырой.**
2. **Но exact-core всё ещё не доказан на уровне, который нужен для poker tracker / popup / stats.**
3. **Самые большие пробелы — позиции, legality/action-order, summary seat semantics, exact pot settlement, KO attribution.**
4. **Текущее “0 warnings / green invariants” нельзя интерпретировать как полную семантическую корректность.**
5. **Правильный следующий шаг — не новые статы и не UI, а жёсткое доведение parser+normalizer до exact-core.**

## 8. Уровень уверенности

- Высокая уверенность:
  - отсутствие position engine;
  - отсутствие полноценного legality engine;
  - intentional ignore summary seat-result lines;
  - reverse winner mapping по collected sums;
  - simplified KO attribution через highest pot;
  - отсутствие PostDead/Muck parser support.
- Средняя уверенность:
  - точная степень риска некоторых GG edge-syntaxes вне committed pack;
  - точная доля готовности в процентах.
