# Аудит репозитория Check_Mate: parser / normalizer / позиции / сайд-поты
Дата: 2026-03-25

## 1. Объем проверки

Проверены:
- `backend/crates/tracker_parser_core/src/parsers/hand_history.rs`
- `backend/crates/tracker_parser_core/src/parsers/tournament_summary.rs`
- `backend/crates/tracker_parser_core/src/models.rs`
- `backend/crates/tracker_parser_core/src/positions.rs`
- `backend/crates/tracker_parser_core/src/betting_rules.rs`
- `backend/crates/tracker_parser_core/src/normalizer.rs`
- `backend/crates/tracker_parser_core/src/pot_resolution.rs`
- `backend/crates/tracker_parser_core/tests/fixture_parsing.rs`
- `backend/crates/tracker_parser_core/tests/hand_normalization.rs`
- `backend/crates/tracker_parser_core/tests/phase0_exact_core_corpus.rs`
- `backend/crates/tracker_parser_core/tests/positions.rs`
- `backend/crates/tracker_parser_core/tests/street_hand_strength.rs`
- committed fixture pack из `backend/fixtures/mbr/hh` и `backend/fixtures/mbr/ts` по именам и по покрытию в тестах.

Важно: это именно глубокий аудит исходников и тестового контура по текущему репозиторию. В этой среде я не поднимал проект и не прогонял `cargo test` локально, поэтому вывод по корректности основан на чтении кода, покрытии тестами и заявленном committed corpus, а не на самостоятельном исполнении кода.

## 2. Краткий вердикт

### Моя оценка стадии
- Проект целиком: **примерно 25%**.
- Ядро `tracker_parser_core` как узкий exact-core под текущий committed GG MBR pack: **примерно 60%**.
- То же ядро как общий production-grade parser/normalizer для широкого реального корпуса: **примерно 30–35%**.

### Что уже выглядит реально сильным
1. **Позиции и порядок действий** сделаны не на «наивных ярлыках», а через отдельный модуль с вычислением порядка активных мест, c отдельной логикой для HU и со специальными тестами на 2–9 активных игроков, пропуски мест и sit-out.
2. **Принудительные all-in от анте/блайндов** не просто отмечаются по строке лога, а дополнительно вычисляются повторным проходом по стеку/вкладам: есть `AnteExhausted`, `BlindExhausted`, `forced_all_in_preflop`.
3. **Вклады, возвраты неуравненной ставки и нарезка main/side pot** в ядре выглядят математически корректно: нормализатор ведет `committed_total`, отдельно учитывает `betting_round_contrib`, а `pot_resolution` режет банки по стандартной лестнице уровней вложений.
4. **Проверка легальности последовательности действий** заметно сильнее среднего: short all-in, reopening action, HU pre/postflop actor order, forced blind actors — это не косметика, а отдельный механизм в `betting_rules.rs`.
5. **Неоднозначные банки не угадываются**. Это правильно. Если exact mapping по собранным данным не выводится однозначно, код предпочитает uncertainty вместо «красивой, но ложной точности».

### Что мешает сказать «ошибок нет»
Главная проблема сейчас — **не базовая арифметика**, а **границы покрытия и не до конца зафиксированная семантика**:
- parser coverage вне committed pack еще не доказан;
- summary seat-result lines покрыты не полностью;
- exact winner mapping все еще строится через поиск по агрегированным collect amounts;
- есть как минимум несколько мест, где нужен явный контракт, а не молчаливое поведение кода.

Итог: **явной арифметической ошибки в core-логике side pot / forced all-in / actor order я не нашел**, но **до уровня “там не должно быть ни единой ошибки” проект пока не доведен**.

## 3. Что я считаю уже корректным или близким к корректному

## 3.1. Позиции и очередность действий
Вердикт: **хорошо**, уверенность высокая.

Что выглядит правильным:
- активный порядок строится по часовой стрелке только по реально активным местам;
- в HU preflop первым действует BTN/SB, postflop первым действует BB;
- в multiway preflop старт идет после BTN/SB/BB, postflop — с первого активного после баттона;
- тесты покрывают 2–9 активных игроков, seat gaps и sit-out.

Замечание:
- сама **логика порядка** выглядит корректной;
- риск вижу не в очередности, а в **канонических именах позиций**: их надо отделить от машинного индекса, иначе статистикам и фильтрам можно «подсунуть» формально корректный порядок, но спорные ярлыки позиции.

## 3.2. Принудительные all-in от анте и блайндов
Вердикт: **сильно**, уверенность высокая.

Что уже хорошо:
- есть отдельные причины all-in (`Voluntary`, `CallExhausted`, `RaiseExhausted`, `BlindExhausted`, `AnteExhausted`);
- forced all-in не ограничен только буквальным `and is all-in` в HH-строке;
- тесты явно проверяют ante-exhausted / blind-exhausted сценарии.

Это именно тот участок, который обычно ломают. Здесь архитектурно подход выбран правильный.

## 3.3. Учет вкладов, возвратов и нарезка side pot
Вердикт: **хорошо**, уверенность высокая на покрытых кейсах.

Что выглядит корректным:
- forced posts и добровольные действия попадают в разный учет, но все сходится в `committed_total`;
- `ReturnUncalled` не только возвращает стек, но и вычитает сумму из `committed_total` и `betting_round_contrib`;
- side pot режется по distinct positive commitment levels;
- eligibility строится из вкладчиков уровня, исключая folded.

Здесь я не вижу очевидной математической ошибки.

## 3.4. Определение победителей банков
Вердикт: **безопасно, но пока не replay-grade**, уверенность средняя.

Сильная сторона:
- код не дорисовывает победителей там, где exact mapping неоднозначен;
- odd-chip/сплиты рассматриваются через варианты распределения и сверяются с фактом collect amounts.

Слабая сторона:
- exactness все еще опирается на поиск соответствия агрегированным collected totals, а не на полноценную pot-level evidence модель;
- поэтому механизм безопасен, но неполон: часть рук останется uncertain даже там, где для production-grade ядра хотелось бы exact.

## 3.5. Легальность действий
Вердикт: **сильно**, уверенность высокая.

Что понравилось:
- engine понимает difference между full raise и short all-in raise;
- reopening action учтен;
- forced actors на blind streets валидируются;
- есть специальные контракты на illegal actor order и action_not_reopened_after_short_all_in.

Это одна из лучших частей текущего ядра.

## 4. Найденные проблемы, пробелы и спорные места

Ниже не смешиваю все в одну кучу. Явно разделяю:
- **подтвержденный пробел** — покрытие/функциональность реально неполные;
- **семантический риск** — код может быть внутренне консистентен, но контракт не зафиксирован или рискован для аналитики;
- **вероятная логическая дыра** — по коду видно ограничение, которое очень похоже на источник ошибки на краю.

| ID | Приоритет | Тип | Область | Проблема |
|---|---|---|---|---|
| F-01 | P0 | подтвержденный пробел | parser | `summary seat-result lines` еще не сведены к полной структурной модели результата |
| F-02 | P0 | подтвержденный пробел | parser/coverage | coverage доказан только на committed pack; широкий реальный корпус не закрыт |
| F-03 | P0 | подтвержденный пробел | normalizer/pot winners | exact winner mapping все еще зависит от поиска по агрегированным collect amounts |
| F-04 | P1 | семантический риск | positions | логика порядка хорошая, но naming позиций надо отделить от машинного индекса и зафиксировать контракт |
| F-05 | P1 | вероятная логическая дыра | normalizer/snapshots | terminal all-in snapshot, судя по логике, ловится слишком узко и может пропускать часть закрытий улицы |
| F-06 | P1 | семантический риск | eliminations/KO | KO сейчас выводится через все pot'ы, в которых busted player участвовал; это не обязательно совпадает с «кто именно выбил» |
| F-07 | P1 | подтвержденный пробел | parser/summary tails | `parse_summary_collected_tail` выглядит слишком узким и может терять дополнительные суффиксы итоговой строки |
| F-08 | P1 | семантический риск | showdown/odd chip | explicit room-rule для odd chip не зафиксирован как доменное правило |
| F-09 | P2 | модельный запах | models/parser | `Call` заполняет `to_amount`, хотя это поле по смыслу ближе к raise-to; сейчас безвредно, но семантически мутно |
| F-10 | P2 | архитектурный долг | uncertainty model | часть неопределенностей живет как warning-строки, а не как строгие typed причины |
| F-11 | P2 | вне core, но важно для продолжения | ingest/time | dev-only ingest и ненормализованное время мешают следующему слою продукта |

### F-01. Summary seat-result lines не доведены до полной структуры — P0
Почему это важно:
- финальная точность normalization и derived-логики упирается в то, насколько полно разобран summary;
- если часть итоговых строк сидит как warnings/partial parse, downstream начинает жить на неполном факте.

Что именно видно:
- в backend README это прямо признано;
- в `hand_history.rs` есть `parse_summary_seat_outcome_line`, но helper'ы покрывают не весь возможный хвост summary-линий;
- есть явная дорожка в `unparsed_summary_seat_line`.

Что делать:
- расширять grammar summary tails до полного набора вариантов, а не латать единичные строки вручную.

### F-02. Coverage широкого реального корпуса не доказан — P0
Почему это важно:
- committed pack — это полезный, но узкий контур;
- «ошибок нет» нельзя заявлять без широкого real-world regression corpus.

Что именно видно:
- сам репозиторий явно пишет, что committed pack покрывается чисто, но широкий корпус еще не доказан;
- тесты хорошие, но они все еще опираются на ограниченный пакет и curated edge matrix.

Что делать:
- отдельный ingestion-поток для расширения корпуса;
- triage-пайплайн с фиксацией новых синтаксических поверхностей;
- property/mutation testing поверх ручных golden fixtures.

### F-03. Pot winner mapping по агрегированным collect amounts — P0
Почему это важно:
- для replay-grade exact core важно уметь объяснить exact pot-level winner mapping, а не только совпасть суммой collect;
- иначе некоторые руки останутся uncertain, а часть derived-аналитики будет ограничена.

Что именно видно:
- механизм сознательно ищет allocation по наборам pot options и collected amounts;
- ambiguous mappings не угадываются — это правильно;
- но значит exactness still limited by evidence model.

Что делать:
- ввести pot-level evidence graph: collect lines, summary outcomes, show lines, eligibilities, showdown ranks, причины uncertainty.

### F-04. Канонический контракт позиций не зафиксирован до конца — P1
Почему это важно:
- порядок действий и label позиции — это разные вещи;
- для фильтров/статов нужно хранить и машинный индекс, и канонический label по выбранной таблице соответствия.

Что именно видно:
- `positions.rs` решает actor order качественно;
- но naming позиций в некоторых конфигурациях выглядит спорно для downstream-консьюмеров.

Что делать:
- оставить core order logic;
- вынести позицию в два поля: `position_index` и `position_label`;
- отдельно задокументировать mapping для 2–9 active players.

### F-05. Вероятный пробел в terminal all-in snapshots — P1
Почему это важно:
- если snapshot финального all-in узла захватывается не во всех путях закрытия раздачи, replay/derived-слой теряет важный узел состояния;
- это особенно важно для коротких стеков, forced all-in и закрытия улицы не через стандартный call/check.

Что именно видно:
- по логике `should_capture_snapshot` триггер выглядит слишком привязанным к конкретному типу завершающего действия;
- очень вероятно, что закрытие через fold или forced-preflop all-in может пройти мимо snapshot.

Что делать:
- определить event-agnostic критерий terminal contest state;
- привязывать snapshot к состоянию стола, а не только к типу последнего действия.

### F-06. Семантика KO / elimination требует v2 — P1
Почему это важно:
- вопрос «кто выбил» и вопрос «кто получил долю банка, где busted player участвовал» — это не одно и то же;
- для bounty, KO EV и отчетности нужна строгая спецификация.

Что именно видно:
- elimination слой собирается по всем pot'ам, где у busted seat были contributions;
- это может быть приемлемо для текущего derived-слоя, но не надо молча считать, что это final semantics.

Что делать:
- разделить:
  1. `contributing_pots`
  2. `busting_pots`
  3. `ko_winners`
  4. `ko_share_fraction`

### F-07. Узкий parser для `collected (...)` tail в summary — P1
Почему это важно:
- это конкретный участок grammar, который легко ломается на вариациях румного текста;
- при этом downstream будет думать, что summary разобран, хотя часть структуры потеряна.

Что делать:
- сделать grammar summary tails декларативным и полным;
- добавить unit tests на все встречающиеся варианты collected/won/showed/mucked/lost.

### F-08. Odd chip правило не оформлено как доменный контракт — P1
Почему это важно:
- пока odd chip фактически выводится через matching факта collect;
- для production-grade exact core нужно либо явно знать правило рума, либо явно сохранять uncertainty reason.

Что делать:
- либо кодировать verified GG rule;
- либо хранить `odd_chip_rule_unknown` как формальную причину uncertainty.

### F-09. Семантика `to_amount` у Call мутная — P2
Почему это важно:
- сейчас это, похоже, не ломает арифметику;
- но создает риск неверного использования поля позже.

Что делать:
- либо сделать `to_amount` опциональным только для raise-to;
- либо явно переименовать/документировать поля action model.

### F-10. Uncertainty model надо типизировать — P2
Почему это важно:
- warning-строки плохи для downstream анализа и quality gates;
- код-агенту потом будет труднее строить строгие проверки.

Что делать:
- ввести typed enums/structs для причин:
  - parser syntax gap
  - partial reveal
  - no-show
  - ambiguous winner mapping
  - summary insufficiency
  - odd chip unresolved
  - collect conflict

### F-11. Dev-only ingest и время — P2
Почему это важно:
- это не core parser bug, но это следующий стоп-фактор для продукта;
- без нормального ingest/time contract дальше build'ить продукт опасно.

## 5. Что я бы НЕ переписывал сейчас

1. **Не переписывать side-pot slicing с нуля.** В текущем виде это сильная часть ядра.
2. **Не ослаблять uncertainty до guessed exactness.** Сейчас осторожность лучше ложной точности.
3. **Не смешивать parser и normalizer.** Разделение сейчас сделано правильно.
4. **Не переделывать actor-order engine, пока не зафиксирован exact-core contract.** Ломать там сейчас нечего; нужно сначала зафиксировать интерфейс и тесты.

## 6. План для код-агента по фазам

Ниже задачи расположены по порядку выполнения. Приоритет P0/P1/P2 — это не «важность вообще», а порядок, в котором я бы реально давал их код-агенту.

---

## Фаза 0. Зафиксировать exact-core контракт и закрыть обязательные пробелы

### Задача P0-01. Зафиксировать канонический контракт нормализованной руки
**Цель:** убрать неявные договоренности из кода и превратить их в тестируемый контракт.

**Что сделать:**
- добавить документ `backend/docs/exact_core_contract.md`;
- перечислить все invariants normalizer'а:
  - conservation of chips;
  - conservation of pots;
  - pot slicing formula;
  - eligibility rules;
  - actor-order rules для 2–9 active players;
  - forced all-in semantics;
  - return-uncalled semantics;
  - KO semantics v1 (текущая) и target semantics v2;
  - uncertainty contract.

**Файлы:**
- `backend/docs/exact_core_contract.md`
- ссылки на `normalizer.rs`, `pot_resolution.rs`, `positions.rs`, `betting_rules.rs`.

**Чек-лист приемки:**
- [ ] есть отдельный документ контракта;
- [ ] для каждого invariant есть минимум один тест, который его защищает;
- [ ] нет неописанных полей/флагов с «подразумеваемой» семантикой;
- [ ] код-агент не меняет поведение, пока не описал контракт.

---

### Задача P0-02. Довести grammar summary seat-result lines до полного exact-core покрытия
**Цель:** убрать `unparsed_summary_seat_line` на committed corpus и на ближайшем расширенном корпусе.

**Что сделать:**
- переписать парсинг хвостов summary-строк как декларативный набор форм;
- покрыть варианты:
  - `folded before Flop`
  - `folded on Flop/Turn/River`
  - `showed [...] and won (...)`
  - `showed [...] and lost`
  - `won (...)`
  - `collected (...)`
  - `mucked`
  - `lost`
  - комбинации с hand class / extra suffixes, если они реально встречаются;
- все еще неизвестные формы переводить не в silent drop, а в строго типизированную проблему.

**Файлы:**
- `src/parsers/hand_history.rs`
- `tests/fixture_parsing.rs`

**Чек-лист приемки:**
- [ ] на committed HH pack нет `unparsed_summary_seat_line`;
- [ ] на edge fixtures нет silent fallback;
- [ ] для каждого вида summary tail есть отдельный unit test;
- [ ] parser warnings остаются только там, где поддержка реально сознательно отсутствует (`no-show`, partial reveal или новая неизвестная форма).

---

### Задача P0-03. Ужесточить exact-core edge matrix для forced all-in / blinds / ante / dead blind
**Цель:** формально закрыть именно те сценарии, о которых ты спрашивал: кто сколько внес, кто авто-all-in, в каком порядке кто действует.

**Что сделать:**
- добавить/расширить fixtures на случаи:
  - short ante => all-in до любого добровольного действия;
  - short SB => forced blind all-in;
  - short BB => forced blind all-in;
  - dead blind + ante;
  - HU preflop и postflop actor order;
  - gaps в местах + sit-out;
  - uncalled bet return;
  - multiway side-pot ladder из 3+ уровней;
  - main pot split + side pot single winner;
  - разные победители main/side pot;
  - busted player, который проигрывает main, но side pot выигрывает другой игрок;
  - odd chip;
  - hidden showdown ambiguity.

**Файлы:**
- `backend/fixtures/mbr/hh/*edge*`
- `tests/fixture_parsing.rs`
- `tests/hand_normalization.rs`
- `tests/phase0_exact_core_corpus.rs`

**Чек-лист приемки:**
- [ ] для каждого edge-hand явно проверяются `seq`, `street`, `player_name`, `action_type`;
- [ ] явно проверяются `is_forced`, `is_all_in`, `all_in_reason`, `forced_all_in_preflop`;
- [ ] явно проверяются `committed_total`, `returns`, `final_pots`, `pot_contributions`, `pot_eligibilities`;
- [ ] в edge matrix нет unexpected warnings;
- [ ] нет invariant errors.

---

### Задача P0-04. Разделить машинный индекс позиции и человекочитаемый label
**Цель:** сохранить правильную очередность действий и одновременно исключить будущие ошибки статистики по названиям позиций.

**Что сделать:**
- ввести канонический `position_index` как машинный факт;
- отдельно хранить `position_label` как отображение по согласованной таблице;
- зафиксировать mapping для 2–9 active players;
- явно описать политику при seat gaps/sit-out.

**Файлы:**
- `src/positions.rs`
- `src/models.rs`
- `tests/positions.rs`

**Чек-лист приемки:**
- [ ] actor order тесты не сломаны;
- [ ] есть явный snapshot expected labels для 2–9 active players;
- [ ] нет мест, где downstream строит аналитику только по строковому label без machine index;
- [ ] спорные обозначения позиций больше не влияют на статистику.

---

### Задача P0-05. Ввести golden snapshot regression на normalized hand
**Цель:** после каждого изменения сравнивать не только «тест прошел/не прошел», а полную форму нормализованной руки.

**Что сделать:**
- на curated edge matrix и нескольких representative committed hands сохранить golden JSON snapshots;
- при изменении структуры normalized hand diff должен быть явным и осознанно подтвержденным.

**Файлы:**
- новый каталог `tests/golden/`
- `tests/hand_normalization.rs`
- возможно helper в `tests/common.rs`

**Чек-лист приемки:**
- [ ] для edge matrix есть golden snapshots;
- [ ] snapshot diff показывает pots / winners / returns / eliminations / positions / flags;
- [ ] любое изменение snapshot требует явного review.

---

## Фаза 1. Довести normalizer до replay-grade exact core

### Задача P1-01. Исправить критерий terminal all-in snapshot
**Цель:** не терять финальное состояние all-in ветки.

**Что сделать:**
- отвязать capture от узкого набора event types;
- вычислять snapshot по state-based критерию: contest closed, remaining actors no longer able/required to act, board not complete yet;
- отдельно покрыть завершение через fold, forced all-in preflop, short all-in without reopen.

**Файлы:**
- `src/normalizer.rs`
- `tests/hand_normalization.rs`
- `tests/phase0_exact_core_corpus.rs`

**Чек-лист приемки:**
- [ ] есть тест на closure через fold;
- [ ] есть тест на closure только forced all-in'ами;
- [ ] есть тест на short all-in без reopen;
- [ ] snapshots появляются ровно там, где должны, и не дублируются.

---

### Задача P1-02. Перевести winner resolution на pot-level evidence model
**Цель:** сделать exactness объяснимой и расширяемой.

**Что сделать:**
- хранить не только финальный результат поиска, но и доказательства:
  - какие collect lines увидели;
  - какие summary outcomes разобраны;
  - какие show lines разобраны;
  - какие contenders допустимы по eligibility/status;
  - какие ambiguity branches остались;
- exact mapping materialize'ить только при полном доказательстве.

**Файлы:**
- `src/pot_resolution.rs`
- `src/models.rs`
- `src/normalizer.rs`

**Чек-лист приемки:**
- [ ] ambiguous hand не выдает guessed winners;
- [ ] exact hand хранит объяснимый evidence trail;
- [ ] collect conflict выдает строгий conflict reason;
- [ ] downstream может понять, почему hand exact или uncertain.

---

### Задача P1-03. Разделить semantics `elimination` и `KO share`
**Цель:** убрать смешение понятий «участвовал в поте» и «выбил игрока».

**Что сделать:**
- сохранить отдельно:
  - `pots_participated_by_busted`
  - `pots_causing_bust`
  - `ko_winner_set`
  - `ko_share_fraction`
  - `is_joint_ko`
  - `is_sidepot_based_ko`
- явно решить, что считается KO в split/main/sidepot сценариях.

**Файлы:**
- `src/normalizer.rs`
- `src/models.rs`
- `tests/hand_normalization.rs`

**Чек-лист приемки:**
- [ ] есть тест, где main pot и side pot выигрывают разные игроки;
- [ ] есть тест на split main pot с bust;
- [ ] есть тест, где busted seat участвовал в нескольких pot'ах, но не все они являются busting;
- [ ] `hero_share_fraction` совпадает с зафиксированной спецификацией, а не с неявной эвристикой.

---

### Задача P1-04. Зафиксировать odd chip contract
**Цель:** убрать неявное поведение на остаточном фишечном банке.

**Что сделать:**
- проверить и задокументировать правило GG для odd chip, если его можно надежно подтвердить;
- если правило не подтверждено, оставить strict uncertainty reason вместо молчаливого выбора.

**Файлы:**
- `src/pot_resolution.rs`
- `backend/docs/exact_core_contract.md`
- тесты на odd-chip fixtures

**Чек-лист приемки:**
- [ ] odd-chip кейсы либо exact по verified rule, либо explicit uncertain;
- [ ] нет silent arbitrary distribution;
- [ ] сумма банка всегда сохраняется.

---

### Задача P1-05. Перевести warning-based неопределенности в typed model
**Цель:** сделать quality gates машинно проверяемыми.

**Что сделать:**
- создать строгий тип причин uncertainty / parser issue;
- warning string оставлять только как human-readable rendering;
- quality gates строить по кодам причин, а не по текстовому `starts_with`.

**Файлы:**
- `src/models.rs`
- `src/parsers/*`
- `src/normalizer.rs`
- тесты

**Чек-лист приемки:**
- [ ] parser issues имеют machine code + severity + payload;
- [ ] uncertainty reasons имеют enum/struct, а не только строки;
- [ ] тесты проверяют коды, а не текст.

---

## Фаза 2. Расширить покрытие и продолжить разработку продукта без потери точности

### Задача P2-01. Подключить широкий реальный корпус и triage pipeline
**Цель:** перестать жить только committed pack.

**Что сделать:**
- создать процедуру загрузки новых HH/TS в quarantine corpus;
- автоматически считать:
  - parse pass rate
  - unexpected warning rate
  - uncertain hand rate
  - conflict rate
- новые синтаксические поверхности складывать в syntax catalog.

**Файлы:**
- `docs/COMMITTED_PACK_SYNTAX_CATALOG.md`
- новый `docs/WIDE_CORPUS_TRIAGE.md`
- test helpers / scripts

**Чек-лист приемки:**
- [ ] есть отдельный quarantined corpus;
- [ ] на каждый новый синтаксический паттерн появляется запись в catalog;
- [ ] parse coverage измеряется числом, а не ощущением.

---

### Задача P2-02. Добавить property tests / mutation tests / fuzzing для математики банков
**Цель:** ловить не только известные сценарии, но и классы ошибок.

**Что сделать:**
- генерировать случайные лестницы вложений и статусы folded/live/all-in;
- проверять invariants:
  - pot conservation
  - no negative stack
  - no negative contribution
  - eligibilities subset of contributors
  - returns do not create chips
- сделать mutation tests на перестановку collect lines и summary order, где это допустимо.

**Файлы:**
- новый тестовый модуль property-based / fuzz
- `src/pot_resolution.rs`
- `src/normalizer.rs`

**Чек-лист приемки:**
- [ ] 10k+ случайных сценариев без panic;
- [ ] invariants соблюдаются;
- [ ] перестановка order-insensitive данных не меняет результат;
- [ ] order-sensitive данные ломают тест ожидаемо и объяснимо.

---

### Задача P2-03. Убрать dev-only ingest assumptions и нормализовать время
**Цель:** сделать следующий слой разработки реальным, а не стендовым.

**Что сделать:**
- убрать жесткую привязку к `Hero` и dev organization;
- ввести корректный time contract:
  - raw local time
  - source timezone / room timezone
  - canonical UTC when derivable
  - explicit nullability contract when not derivable

**Файлы:**
- `parser_worker`
- import pipeline
- schema / migrations
- docs

**Чек-лист приемки:**
- [ ] импорт не зависит от dev-only hero name;
- [ ] время либо нормализовано, либо явно помечено как ненадежное;
- [ ] downstream queries не опираются на случайный NULL/локальное время.

---

### Задача P2-04. Только после стабилизации exact-core подключать MBR-specific derived слой
**Цель:** не строить статистику на неполном факте.

**Что сделать:**
- после Phase 0/1 стабилизировать:
  - `big_ko` decoder integration
  - stat-layer materialization
  - stage/KO/economics derived logic
- не тянуть derived-слой вперед core-правды.

**Чек-лист приемки:**
- [ ] derived layer использует только зафиксированные exact/uncertain contracts;
- [ ] нет статистик, которые молча трактуют uncertainty как exact;
- [ ] KO-related metrics опираются на KO semantics v2, а не на legacy approximation.

## 7. Минимальный порядок выполнения без распыления

Если давать работу код-агенту последовательно, я бы дал именно так:

1. **P0-01** — contract doc  
2. **P0-02** — summary seat-result grammar  
3. **P0-03** — edge matrix на all-in / side-pot / positions  
4. **P0-04** — position index + labels  
5. **P0-05** — golden snapshots  
6. **P1-01** — terminal all-in snapshot  
7. **P1-02** — pot-level evidence model  
8. **P1-03** — KO semantics v2  
9. **P1-04** — odd chip contract  
10. **P1-05** — typed uncertainty  
11. **P2-01** — wide corpus triage  
12. **P2-02** — property/fuzz tests  
13. **P2-03** — ingest/time hardening  
14. **P2-04** — derived/stat integration

## 8. Финальный практический вывод

### Что уже можно считать опорой
- actor order engine;
- forced all-in detection from blinds/antes;
- contribution accounting;
- return-uncalled accounting;
- side-pot slicing;
- conservative handling of ambiguity.

### Что пока нельзя считать закрытым
- полный parser coverage вне committed pack;
- полный summary normalization;
- replay-grade exact winner mapping на широком корпусе;
- финальная KO semantics;
- production-ready downstream contracts.

### Главная рекомендация
**Не делать большой рефакторинг.** Ядро уже не слабое. Правильная стратегия сейчас:
1. зафиксировать exact-core контракт;
2. закрыть parser gaps именно в summary/result surface;
3. ужесточить regression matrix на all-in / side-pot / positions / KO;
4. только потом расширять corpus и поднимать derived/product слой.
