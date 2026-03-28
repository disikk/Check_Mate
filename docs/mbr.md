# Аудит Check_Mate + MBR_Stats

Дата: 2026-03-27

## 1. Короткий вывод

### Что по факту с `Check_Mate`

Проект не на стадии «идеи» и не на стадии «почти готово». По текущему состоянию это **сильный backend/data foundation + несколько живых вертикальных срезов**, но **без закрытого контура корректности для экономики/статов и без продуктовой доводки**.

Моя оценка текущей стадии:

- **Проект в целом:** **30–35%** до состояния «можно доверять как продукту», и **45–50%** до состояния «сильная инженерная база для ускоренной разработки».
- **Схема БД / миграции / каноническая data-модель:** **55–60%**.
- **Ингест / очередь / upload-вертикаль:** **40–50%**.
- **Парсер + нормализация + exact settlement:** **25–30%** до replay-grade.
- **MBR-экономика / KO / boundary-логика:** **20–25%**.
- **Перенос legacy-статов из `MBR_Stats`:** **10–15%**.
- **API / frontend как продукт:** **20–25%**.

### Что критично важно понять

Главный риск сейчас не в том, что «мало кода». Главный риск в том, что **семантика статов и экономических примитивов ещё не зафиксирована как единый контракт**, а часть legacy-алгоритмов из `MBR_Stats` **просто нельзя переносить 1-в-1**, потому что они эвристические и местами математически некорректны.

### Главный приоритет

Сейчас нельзя ставить в приоритет HUD/попапы/красивый UI. Сначала нужно закрыть:

1. канонический контракт данных,
2. exact settlement / eliminations / prize decomposition,
3. спецификацию каждого legacy-стата,
4. parity/delta harness,
5. только потом — перенос статов и UI.

---

## 2. На какой стадии реально находится `Check_Mate`

### 2.1. Что уже выглядит сильным

1. Репозиторий разбит на осмысленные backend-crates:
   - `tracker_parser_core`
   - `tracker_ingest_runtime`
   - `tracker_ingest_runner`
   - `tracker_query_runtime`
   - `tracker_web_api`
   - `mbr_stats_runtime`

   Это хороший признак: архитектурно проект уже строится как набор отдельных доменных модулей, а не как один монолитный комбайн.

2. Есть длинная цепочка миграций. По именам миграций видно, что уже трогались реальные доменные слои:
   - exact pot / KO core,
   - stage economics,
   - positions,
   - all-in metadata,
   - street hand strength,
   - uncertain codes,
   - eliminations v2,
   - boundary resolution,
   - FT helper,
   - stage predicates,
   - KO money contracts,
   - ingest bundle queue order.

   Это уже не «черновой скелет», а настоящая эволюция схемы.

3. Есть признаки живого stat/runtime слоя:
   - `materializer.rs`
   - `registry.rs`
   - `big_ko.rs`
   - `ft_dashboard.rs`
   - `queries.rs`
   - тесты `canonical_snapshot_golden.rs`, `ft_dashboard_snapshot.rs`, `spec_parity.rs`

4. Есть живой upload/API вертикальный срез и FT dashboard slice.

### 2.2. Что не доведено до уровня «можно доверять цифрам»

1. **Сам репозиторий признаёт**, что normalizer ещё не replay-grade.
2. Pot-winner mapping и eliminations ещё не доведены до полностью закрытого exact-контракта.
3. Boundary KO EV пока point-estimate only.
4. Big KO decoder ещё не подключён к финальному stat/materialization-слою.
5. Summary seat-result lines ещё не до конца структурированы.
6. По root README roadmap отстаёт от реального состояния репозитория, то есть уже есть **documentation drift**.

### 2.3. Моя оценка корректности уже сделанного в `Check_Mate`

**Вердикт:** база собрана правильно, но **корректность итоговых чисел пока нельзя считать закрытой**.

То есть:

- **архитектурно** направление хорошее;
- **инженерно** проект уже вышел из стадии «наброска»;
- **математически и доменно** ещё рано считать слой статов законченным.

### 2.4. Что я считаю уже качественным решением

- Разделение ingestion / parser / query / stats / api на отдельные модули.
- Переход к exact-core мышлению вместо старого plugin-only стиля.
- Наличие golden/snapshot/parity тестовых следов в stat-runtime.
- Наличие специальных docs/plans по exact-core и normalized-hand design.

### 2.5. Что я считаю недоделанным или опасным

- Статус проекта описан неединообразно между root README и backend README.
- Нет оснований считать, что exact economics уже полностью закрыт.
- Нет оснований считать, что перенос legacy-статов уже специфицирован поштучно.
- Нельзя безопасно расширять UI, пока не решено, какие legacy-формулы надо **сохранить**, а какие надо **сломать сознательно**, потому что они неверны.

---

## 3. Аудит `MBR_Stats`: общие системные проблемы

Ниже — не косметика, а именно системные проблемы legacy-слоя.

### MBR-SYS-01 — нет отдельного набора unit-тестов на формулы статов
**Серьёзность:** P0

В репозитории есть тесты на инфраструктуру, сервисы, кэш, UI и т.п., но нет нормального набора изолированных тестов, который фиксирует правильность самих алгоритмов статов. Это означает, что многие ошибки могли жить долго просто потому, что никто не проверял именно математику.

### MBR-SYS-02 — смешаны разные источники истины
**Серьёзность:** P0

Разные плагины считают показатели по разным основаниям:

- где-то только `has_ts`,
- где-то только `has_hh`,
- где-то только `final_table_hands`,
- где-то без нормального source-gating вообще.

Из-за этого у разных статов отличаются:

- выборка,
- знаменатель,
- готовность данных,
- смысл нуля.

Это одна из главных причин, почему legacy нельзя просто «переписать на Rust» и считать работу законченной.

### MBR-SYS-03 — почти везде отсутствует разделение между `0` и `unknown/not-ready`
**Серьёзность:** P0

Во многих плагинах при нехватке данных возвращается `0.0`. Это очень опасно, потому что:

- реальный ноль и отсутствие данных становятся неразличимы;
- UI не может показать «частично готово / заблокировано / нет источника»;
- parity становится ложным: одинаковый ноль может означать разные причины.

### MBR-SYS-04 — контракт `precomputed_stats` сделан нестрого
**Серьёзность:** P0

Часть плагинов обращается к `precomputed_stats` как к словарю, часть — как к объекту с атрибутами. Это уже привело к реальным дефектам.

### MBR-SYS-05 — хардкод payout/KO-экономики живёт прямо в плагинах
**Серьёзность:** P0

В нескольких местах зашиты:

- регулярные выплаты `1 место = 4x buy-in`, `2 место = 3x`, `3 место = 2x`;
- статическая карта средних KO-значений по buy-in;
- `total_chips_at_ft = 18000`;
- `start_stack = 1000`;
- эвристика `avg_attempts_per_ko = 2.5`.

Такие вещи должны жить не в формулах статистики, а в канонической модели формата и/или в exact-derived примитивах.

### MBR-SYS-06 — legacy-слой слишком часто использует агрегированные поля вместо exact primitives
**Серьёзность:** P0

Примеры опасных зависимостей:

- `t.ko_count`
- `t.payout - regular_payout`
- `final_table_initial_stack_*`
- `reached_final_table`
- `pre_ft_ko`, привязанный к FT-hands

Пока нет закрытого exact-контракта, эти поля легко становятся «почти правильными», но не строго корректными в крайних случаях:

- split pot,
- side pot,
- multiple busts in one hand,
- ambiguous collect mapping,
- incomplete FT,
- частичные HH/TS-данные,
- нестандартные/редкие buy-in значения,
- Big KO / mystery KO.

### MBR-SYS-07 — отсутствует единый readiness/provenance contract
**Серьёзность:** P0

Для каждого стата надо явно определить:

- какие источники обязательны,
- какие допустимы частично,
- когда стат считается `ready`,
- когда `partial`,
- когда `blocked`,
- когда `empty`.

Сейчас этого контракта нет.

---

## 4. Аудит `MBR_Stats`: конкретные дефекты по алгоритмам

Ниже — по семействам и модулям.

## 4.1. Простые placement / volume статы

### `avg_finish_place.py`
**Риск:** низкий

Формула простая и в целом корректная: среднее `finish_place` по турнирам с `has_ts`.

Проблемы:
- не различает `0` и `not-ready`;
- нет явного контракта: считать только турниры с надёжным result-source;
- parity надо строить не к старому коду как таковому, а к каноническому `finish_place`.

Действие:
- переносить рано, но только поверх exact result contract.

### `avg_finish_place_ft.py`
**Риск:** низкий

Формула в целом корректна.

Проблемы:
- те же source/readiness вопросы;
- зависимость от корректности `reached_final_table`.

Действие:
- переносить в первой волне.

### `avg_finish_place_no_ft.py`
**Риск:** низкий/средний

Проблемы:
- реальный дефект: плагин обращается к `precomputed_stats` как к объекту с атрибутами, хотя сервис передаёт словарь;
- комментарий внутри файла сам признаёт, что этот стат не доведён до нормального precomputed path;
- опять же нет `unknown` vs `0`.

Действие:
- исправить сразу;
- портировать только после унификации контракта `precomputed_stats`.

### `final_table_reach.py`
**Риск:** низкий/средний

Формула простая.

Проблемы:
- нужен жёсткий ответ на вопрос: знаменатель — все турниры или только турниры с надёжной boundary-семантикой?
- в legacy это просто `len(tournaments)`, то есть смысл сильно зависит от того, что попало в выборку.

Действие:
- зафиксировать в спецификации один canonical denominator.

### `itm.py`
**Риск:** низкий

Формула сама по себе нормальная.

Проблемы:
- hardcoded `top-3` должен опираться на canonical regular prize model, а не на допущение внутри плагина;
- ноль и отсутствие TS не различаются.

Действие:
- переносить в первой волне.

### `avg_ft_initial_stack.py`
**Риск:** средний

Проблемы:
- плагин никак не проверяет, что `final_table_initial_stack_*` получены из exact first FT hand;
- нет source-gating;
- использует турнирные поля как истину, а не first FT canonical event.

Действие:
- переносить только после фиксации exact FT-entry contract.

### `total_ko.py`
**Риск:** средний

Проблемы:
- полностью зависит от агрегированного `t.ko_count`;
- неясно, что именно такое `ko_count`: целое число elimination events, fractional share, credit count, или что-то ещё;
- split/side/multi-bust кейсы legacy не делает прозрачными.

Действие:
- не переносить «как есть»;
- сперва ввести canonical `hero_exact_ko_event_count` и отдельно `hero_ko_share_total`.

### `avg_ko_per_tournament.py`
**Риск:** средний

Проблемы:
- все проблемы `total_ko.py` наследуются;
- значение зависит от того, какие турниры считаются HH-ready.

Действие:
- считать из canonical KO primitive.

### `pre_ft_chipev.py`
**Риск:** средний/высокий

Формула legacy:
`(sum(ft_entry_stack_chips for FT-reaching HH tournaments) / count(HH tournaments)) - 1000`

Это не «EV» в строгом смысле, а агрегат «средний результат в фишках к моменту FT, где non-FT турниры вносят ноль через знаменатель».

Проблемы:
- название вводит в заблуждение;
- зависимость от `final_table_initial_stack_chips`;
- скрытый смысл non-FT турниров зашит в знаменателе;
- нет явного контракта на HH coverage.

Действие:
- либо переименовать,
- либо строго задокументировать именно эту legacy-семантику,
- либо заменить на новый exact-метрик и сознательно сломать parity.

---

## 4.2. FT / stage / boundary-семейство

### `early_ft_bust.py`
**Риск:** средний

Проблемы:
- знаменатель — все FT турниры, даже если по части из них нет надёжного результата;
- `finish_place` может отсутствовать, но турнир останется в знаменателе;
- нужны readiness states.

Действие:
- переносить после result-source contract.

### `pre_ft_ko.py`
**Риск:** средний

Проблемы:
- корректность зависит от инварианта: `pre_ft_ko` должен сидеть только на одном специальном boundary-событии на турнир;
- если поле размножается или дублируется на нескольких FT-hands, будет overcount.

Действие:
- в `Check_Mate` сделать это не как «особое поле на руке», а как точный canonical boundary primitive.

### `early_ft_ko.py`
**Риск:** высокий

Ключевой дефект:
- плагин суммирует `hero_ko_this_hand` по `is_early_final`, **но не вычитает `pre_ft_ko`**, тогда как другие FT-конверсионные статы это делают.

Это создаёт внутреннюю несовместимость legacy-слоя:
- `early_ft_ko_count` и FT conversion family могут говорить о разном количестве ранних KO.

Дополнительные проблемы:
- знаменатель — все FT турниры, а не только турниры с HH/FT-hands;
- ноль и not-ready не различаются.

Действие:
- переписать на exact stage contract.

### `incomplete_ft_percent.py`
**Риск:** средний

Проблемы:
- логика зависит от смысла `hand.table_size`;
- если incomplete FT в данных записан как `table_size < 9`, фильтр `table_size == final_table_size` может выкинуть как раз нужные кейсы;
- возвращает целый процент, а не нормальный decimal-rate;
- нет явной связи с canonical first FT hand.

Действие:
- проверить модель поля;
- в `Check_Mate` считать только через exact FT-entry event.

### `deep_ft_stat.py`
**Риск:** средний

Проблемы:
- реальный дефект: опять дикт/объект-путаница в `precomputed_stats`;
- `reach_percent` считает знаменатель по всем FT-турнирам, а числитель — по тем, где есть `final_table_hands`; если coverage неполный, будет bias вниз;
- `roi` считается по stage tournaments без жёсткого `has_ts` gating.

Действие:
- сохранить идею стата, но переписать поверх canonical FT-stage facts.

### `ko_stage_2_3.py`
### `ko_stage_3_4.py`
### `ko_stage_4_5.py`
### `ko_stage_5_6.py`
### `ko_stage_6_9.py`
### `ko_stage_7_9.py`
**Риск:** высокий

Это семейство надо считать проблемным целиком.

Критичные проблемы:

1. **`*_amount` == `ko_total`**
   - То есть поле с названием `amount` фактически содержит не деньги, а count.
   - Это семантически опасно.

2. **Неправильный знаменатель для attempts-per-tournament**
   - В модулях, где считается `*_attempts_per_tournament`, знаменатель — число всех уникальных FT турниров в `final_table_hands`, а не число турниров, которые реально дошли до этой стадии.
   - Это систематически занижает показатель.

3. **Неясно, это sliding windows или disjoint stages**
   - `2-3`, `3-4`, `4-5`, `5-6` пересекаются по границам.
   - Если это осознанные sliding windows — это надо явно назвать так в спецификации.
   - Если нет — это просто плохая сегментация.

4. **Нет exact KO-money / KO-share semantics**
   - Это только счётчик по `hero_ko_this_hand`.

5. **Нет readiness contract**
   - Статы silently возвращают нули.

Действие:
- это семейство нельзя переносить без полного перепроектирования семантики.

---

## 4.3. FT stack conversion family

### `ft_stack_conversion.py`
**Риск:** критичный

Это один из самых проблемных legacy-алгоритмов.

Ключевые дефекты:

1. **Sample mismatch**
   - `early_ko_count` собирается по всем `final_table_hands`,
   - знаменатель `count` = все FT турниры,
   - а `ft_data` для expected value строится только по подмножеству турниров, где есть `final_table_initial_stack_chips` и `final_table_start_players`.

   То есть actual и expected считаются по разным подвыборкам.

2. **Используется median stack вместо tournament-level expectation**
   - Берётся медианный стек по всем FT, затем одна общая доля стека.
   - Это математически грубая эвристика. Правильнее считать expected KO по каждому турниру отдельно и потом агрегировать.

3. **Жёстко зашито `total_chips_at_ft = 18000`**
   - допустимо только если это formal invariant формата;
   - сейчас это просто магическое число внутри стата.

4. **Семантика early KO не синхронизирована с `early_ft_ko.py`**
   - здесь `pre_ft_ko` вычитается, а там — нет.

Действие:
- не переносить как есть;
- переписать на tournament-level exact model.

### `ft_stack_conversion_attempts.py`
**Риск:** критичный

Наследует все проблемы предыдущего стата и добавляет новые.

Критичные дефекты:

1. **тот же sample mismatch**;
2. **произвольная эвристика `avg_attempts_per_ko = 2.5`**;
3. attempts-adjustment не опирается на доменную модель, а просто штрафует/бустит коэффициент эвристически;
4. `success_rate` и adjusted efficiency не имеют строгой canonical интерпретации.

Действие:
- не сохранять legacy-формулу;
- либо убить стат,
- либо заново определить его математический смысл.

### `ft_stack_conversion_stage.py`
**Риск:** высокий

Лучше, чем два предыдущих модуля, потому что expected считается per tournament/stage, но проблемы всё равно есть:

- нужен formal contract stage entry;
- используется fixed total chips;
- `pre_ft_ko` вычитается через `stage_hands[0].pre_ft_ko`, что требует очень жёсткого инварианта модели;
- нет readiness/partial semantics.

Действие:
- оставить идею, но переписать на canonical stage facts.

---

## 4.4. KO-money / ROI / economics family

### `winnings_from_itm_stat.py`
**Риск:** низкий/средний

Идея нормальная: регулярные призовые считать отдельно.

Проблема в том, что сам payout schedule зашит прямо в плагин. В `Check_Mate` это должно браться из canonical `regular_prize_money`.

Действие:
- переносить в первой волне, но не через magic constants.

### `winnings_from_ko_stat.py`
**Риск:** средний

Идея тоже в целом нормальная: `payout - itm_payout`.

Проблемы:
- опирается на hardcoded payout schedule;
- это агрегат по турниру, а не exact KO-event decomposition;
- при переносе в `Check_Mate` надо считать через canonical decomposition (`regular_prize_money`, `mystery_money_total`, при необходимости `split bounty credits`).

Действие:
- переносить после materialization prize components.

### `roi.py`
**Риск:** низкий/средний

Сам по себе gross ROI корректен.

Проблемы:
- source-gating только TS;
- `0` и `not-ready` не различаются.

Действие:
- переносить после exact prize decomposition.

### `roi_on_ft.py`
**Риск:** низкий/средний

То же самое, только по FT tournaments.

Проблемы:
- нужно явно определить, FT определяется по canonical boundary, а не по legacy-флагу неизвестного происхождения.

Действие:
- переносить после FT-boundary contract.

### `ko_luck.py`
**Риск:** критичный

Это один из самых слабых legacy-алгоритмов.

Ключевые проблемы:

1. **Ожидаемая стоимость KO берётся из статической buy-in карты**
   - не exact,
   - не учитывает Big KO / mystery distribution,
   - не покрывает все buy-in значения.

2. **lookup по float-ключу**
   - потенциальные промахи по точному сравнению чисел с плавающей точкой.

3. **actual и expected считаются по разным логическим основаниям**
   - actual KO earnings берутся по всем TS+HH турнирам,
   - expected считается только когда buy-in найден в map и `ko_count > 0`.
   - Это создаёт систематический bias.

4. **Полностью зависит от `ko_count` как агрегата неизвестной точности**.

Действие:
- не переносить;
- заново определить через exact bounty-money model.

### `roi_adj.py`
**Риск:** критичный

Проблемы:
- наследует всю слабость `ko_luck.py`;
- реальный дефект: `ROIAdjustedStat` вызывает `KOLuckStat().compute(...)`, **не прокидывая `buyin_avg_ko_map` из `kwargs`**, то есть кастомная карта ожиданий просто теряется;
- adjusted ROI математически зависит от очень слабой модели luck.

Действие:
- не переносить legacy-формулу;
- сначала определить, нужен ли вообще такой стат в exact-версии.

### `ko_contribution.py`
**Риск:** высокий

Проблемы:
- actual contribution считается как доля KO money в total payout;
- adjusted contribution считается уже через heuristic expected KO map;
- получается смесь exact-ish actual и heuristic adjusted;
- опять float-key lookup по buy-in;
- опять `ko_count` как слабая база.

Действие:
- переносить только после exact money model; heuristic adjusted-ветку пересобрать заново или удалить.

### `big_ko.py`
**Риск:** критичный / нельзя переносить как есть

Это самый проблемный legacy-алгоритм.

Ключевые дефекты:

1. **Big KO определяется не по отдельным bounty events, а по общей KO-сумме турнира.**
2. Далее сумма разлагается **жадно** на множители `x10000, x1000, x100, x10, x2, x1.5`.
3. Такое разложение может породить **фиктивные big KO**, которых в реальности не было.

Пример логической ошибки:
- если игрок получил несколько обычных KO, их суммарная стоимость может быть жадно интерпретирована как один `x2` или `x1.5`, хотя ни одного такого отдельного bounty event не было.

Дополнительные проблемы:
- используется float floor/division;
- теряется identity конкретных KO events;
- алгоритм непригоден для exact-аудита.

Действие:
- переписывать с нуля только на per-elimination/per-award basis.

---

## 5. Самые важные найденные дефекты (сразу в backlog)

### P0 — исправлять в первую очередь

- **BUG-001**: `avg_finish_place_no_ft.py` — `precomputed_stats` используется как объект, хотя фактически это словарь.
- **BUG-002**: `deep_ft_stat.py` — та же dict/object путаница.
- **BUG-003**: `early_ft_ko.py` включает `pre_ft_ko`, из-за чего расходится с FT conversion family.
- **BUG-004**: `ko_stage_*_attempts_per_tournament` делит на все FT турниры, а не на турниры, достигшие конкретной стадии.
- **BUG-005**: `ko_stage_*_amount` хранит count, а не amount.
- **BUG-006**: `ft_stack_conversion.py` — actual/expected считаются по разным подвыборкам.
- **BUG-007**: `ft_stack_conversion.py` — медианный стек вместо per-tournament expected model.
- **BUG-008**: `ft_stack_conversion_attempts.py` — эвристика `2.5 attempts per KO` не имеет строгого доменного основания.
- **BUG-009**: `big_ko.py` — greedy decomposition общей KO-суммы математически некорректна.
- **BUG-010**: `ko_luck.py` — heuristic expected KO value map + float-key lookup + неполное покрытие buy-in.
- **BUG-011**: почти все статы смешивают `0` и `not-ready`.
- **BUG-012**: нет стат-тестов, которые фиксируют сами формулы.

### P1 — очень важно, но после exact-core

- **BUG-013**: `roi_adj.py` не прокидывает custom KO-map в `KOLuckStat`.
- **BUG-014**: `incomplete_ft_percent.py` зависит от семантики `table_size` и может пропускать short-start FT.
- **BUG-015**: `deep_ft_stat.py` считает ROI без жёсткого TS-gating.
- **BUG-016**: `pre_ft_chipev.py` имеет слабое имя/семантику и требует formal contract.
- **BUG-017**: documentation drift внутри `Check_Mate` создаёт ложную картину готовности.

### P2 — после стабилизации ядра

- UI/UX-статусы, красивые отчёты, HUD, popup, расширенный query/filter engine.

---

## 6. Какая canonical модель должна появиться в `Check_Mate`

Перед переносом статов нужно явно завести и зафиксировать следующие exact primitives.

### 6.1. Турнирные примитивы

- `tournament_has_ts`
- `tournament_has_hh`
- `result_is_exact`
- `finish_place_exact`
- `reached_final_table_exact`
- `ft_entry_hand_id`
- `ft_entry_players_count`
- `ft_entry_stack_chips`
- `ft_entry_stack_bb`
- `ft_started_incomplete`
- `deep_ft_reached_exact` (first point where players <= 5)

### 6.2. KO / elimination примитивы

- `hero_exact_ko_event_count`
- `hero_ko_share_total`
- `hero_ko_attempt_count`
- `hero_ko_money_total`
- `hero_big_ko_awards[]` — список фактических bounty-awards с multiplier/value
- `hero_pre_ft_ko_count_exact`
- `hero_stage_ko_count_exact(stage)`
- `hero_stage_ko_attempt_count(stage)`

### 6.3. Prize decomposition примитивы

- `buy_in_total`
- `regular_prize_money`
- `mystery_money_total` / `bounty_money_total`
- `total_prize_money`
- `roi_gross`
- `roi_regular_only`
- `roi_ko_only`

### 6.4. Readiness / provenance contract

Для каждого стата должен вычисляться не только `value`, но и:

- `status`: `ready | partial | blocked | empty`
- `required_sources`
- `used_sources`
- `coverage_numerator`
- `coverage_denominator`
- `uncertainty_flags[]`

Без этого перенос legacy-статов будет источником вечной путаницы.

---

## 7. Стратегия переноса legacy-статов: что сохранять, а что ломать сознательно

Все legacy-статы надо разбить на 3 класса.

### Класс A — переносить почти 1-в-1

- `avg_finish_place`
- `avg_finish_place_ft`
- `avg_finish_place_no_ft`
- `final_table_reach`
- `itm`
- `roi`
- `roi_on_ft`
- `winnings_from_itm`

### Класс B — переносить по смыслу, но не по текущей реализации

- `avg_ft_initial_stack`
- `total_ko`
- `avg_ko_per_tournament`
- `winnings_from_ko`
- `early_ft_bust`
- `deep_ft_stat`
- `pre_ft_ko`
- `incomplete_ft_percent`
- `pre_ft_chipev`
- `ko_stage_*`
- `ft_stack_conversion_stage`

### Класс C — parity ломать сознательно, потому что legacy-формула слабая или некорректна

- `big_ko`
- `ko_luck`
- `roi_adj`
- `ko_contribution`
- `ft_stack_conversion`
- `ft_stack_conversion_attempts`
- `early_ft_ko` (если legacy действительно включает `pre_ft_ko`)

Для класса C надо не «добиваться старых цифр», а:

1. задокументировать, почему старый алгоритм неверен,
2. показать новый exact-контракт,
3. держать отчёт о parity-break с объяснением.

---

## 8. Фазовый план работ для code-agent

Ниже — **конкретный backlog**, который можно отдавать код-агенту.

---

# ФАЗА 0 — Зафиксировать контракт и не допустить дальнейшего semantic drift

## TASK CM-P0-01 — Синхронизировать статус проекта и документацию
**Приоритет:** P0

### Что сделать
- Обновить root README.
- Привести root README, backend README и roadmap/status docs к одному состоянию.
- Отдельно описать: что уже работает, что experimental, что knowingly incorrect, что not yet wired.

### Почему
Сейчас документация даёт противоречивую картину. Это ведёт к неправильной приоритизации разработки.

### Чек-лист приёмки
- [ ] root README и backend README не противоречат друг другу по ключевым статусам.
- [ ] Есть таблица стадий по подсистемам.
- [ ] Отдельно перечислены known limitations exact-core.
- [ ] Отдельно перечислены уже работающие vertical slices.

---

## TASK CM-P0-02 — Зафиксировать canonical stat specification
**Приоритет:** P0

### Что сделать
Создать единый документ `docs/stat_catalog/mbr_stats_spec_v2.yml` (или аналог), где для **каждого** legacy-стата указать:

- canonical name,
- legacy source module,
- numerator,
- denominator,
- required sources,
- readiness rules,
- whether parity must be preserved,
- whether parity break is intentional,
- exact SQL/Rust primitive dependencies,
- examples of edge cases.

### Почему
Без этого перенос статов превратится в хаотичный «переписать плагины на новом стеке».

### Чек-лист приёмки
- [ ] Все 31 legacy-модуля описаны поштучно.
- [ ] Для каждого указано: preserve / reinterpret / replace.
- [ ] Для каждого есть explicit source contract.
- [ ] Для каждого есть explicit zero-vs-unknown contract.
- [ ] Для high-risk статов есть отдельное rationale, почему legacy-паритет не обязателен.

---

## TASK CM-P0-03 — Ввести readiness/provenance contract для статов
**Приоритет:** P0

### Что сделать
- Ввести единый тип результата стата: `value + status + provenance + uncertainty_flags`.
- Не возвращать голый `0.0` там, где данные неполные.
- Протащить это через runtime / API / frontend.

### Почему
Сейчас legacy-подход системно смешивает реальный ноль и отсутствие данных.

### Чек-лист приёмки
- [ ] В runtime есть единый result envelope.
- [ ] `ready/partial/blocked/empty` используются консистентно.
- [ ] API возвращает status и provenance рядом со значением.
- [ ] Frontend отображает blocked/partial отдельно от нуля.

---

## TASK CM-P0-04 — Собрать golden corpus и edge-case corpus
**Приоритет:** P0

### Что сделать
Подготовить набор fixture-турниров, покрывающий:

- обычные KO,
- split pot,
- side pot,
- multiple busts in one hand,
- ambiguous collect amounts,
- pre-FT KO,
- incomplete FT,
- 7-max/8-max FT start,
- отсутствует TS,
- отсутствует HH,
- mystery/big KO,
- buy-in вне стандартной карты.

### Почему
Именно тут ломаются почти все спорные legacy-статы.

### Чек-лист приёмки
- [ ] Есть минимальный reproducible набор fixture-файлов.
- [ ] Каждый edge-case имеет ожидаемый canonical outcome.
- [ ] Набор запускается в CI.
- [ ] Есть golden snapshots по materialized primitives, а не только по UI.

---

# ФАЗА 1 — Закрыть exact-core: settlement, eliminations, prize decomposition

## TASK CM-P0-05 — Довести normalizer до replay-grade для целевого покрытия
**Приоритет:** P0

### Что сделать
- Закрыть критичные пробелы normalizer.
- Убрать места, где ambiguity тихо превращается в guessed truth.
- Ввести uncertainty flags на уровне hand/tournament facts.

### Почему
Пока normalizer не replay-grade, всё остальное остаётся условным.

### Чек-лист приёмки
- [ ] Для golden corpus normalizer воспроизводит canonical facts без ручных допущений.
- [ ] Ambiguous cases не masquerade как exact.
- [ ] Есть репорт покрытия по parse/normalize outcomes.
- [ ] Regression tests добавлены в CI.

---

## TASK CM-P0-06 — Зафиксировать exact elimination / KO contract
**Приоритет:** P0

### Что сделать
Явно определить и материализовать:

- elimination event,
- KO credit,
- KO share,
- KO money,
- KO attempt,
- multi-bust semantics,
- side-pot semantics,
- split-bounty semantics.

### Почему
Почти все спорные legacy-статы упираются именно в это.

### Чек-лист приёмки
- [ ] У elimination есть formal data contract.
- [ ] Для split/side/multi-bust есть тесты.
- [ ] `hero_exact_ko_event_count` и `hero_ko_share_total` существуют отдельно.
- [ ] `hero_ko_money_total` считается независимо от count/share.

---

## TASK CM-P0-07 — Зафиксировать exact prize decomposition
**Приоритет:** P0

### Что сделать
Сделать canonical decomposition:

- buy-in,
- regular prize,
- KO/bounty prize,
- total payout,
- FT-only decomposition where needed.

### Почему
Нельзя оставлять payout-математику в каждом стате отдельно.

### Чек-лист приёмки
- [ ] `regular_prize_money` материализуется явно.
- [ ] `bounty/mystery_money_total` материализуется явно.
- [ ] `total_prize_money = regular + bounty_components` проверяется тестом.
- [ ] Нет magic constants 4/3/2 внутри runtime-статов.

---

## TASK CM-P0-08 — Зафиксировать FT boundary contract
**Приоритет:** P0

### Что сделать
Определить и материализовать:

- first FT hand,
- FT start players count,
- incomplete FT flag,
- pre-FT KO boundary fact,
- deep FT entry fact (players <= 5),
- stage windows.

### Почему
Без этого FT/stage-статы будут вечным источником расхождений.

### Чек-лист приёмки
- [ ] `ft_entry_hand_id` и `ft_entry_players_count` существуют как exact facts.
- [ ] `pre_ft_ko` не живёт как неясная приклейка к legacy hand list.
- [ ] `deep_ft_reached_exact` выводится детерминированно.
- [ ] Для incomplete FT есть тесты 7/8/9-start.

---

# ФАЗА 2 — Материализовать exact primitives и построить stat-runtime поверх них

## TASK CM-P0-09 — Построить materialized primitive layer для статов
**Приоритет:** P0

### Что сделать
- В `mbr_stats_runtime` и/или ref-schema материализовать canonical primitive facts.
- Сделать registry не вокруг legacy plugin names, а вокруг exact primitive dependencies.

### Почему
Новый stat-runtime должен зависеть от exact facts, а не от старых эвристик.

### Чек-лист приёмки
- [ ] Есть явный список primitive tables/views.
- [ ] Любой stat можно выразить через primitive layer.
- [ ] Snapshot tests фиксируют primitive outputs.
- [ ] Нет дублирования экономической логики по нескольким stat modules.

---

## TASK CM-P0-10 — Собрать parity/delta harness против `MBR_Stats`
**Приоритет:** P0

### Что сделать
- На одном и том же fixture corpus запускать legacy-расчёты и новый runtime.
- Для каждого стата получать одно из состояний:
  - `PARITY_OK`
  - `PARITY_KNOWN_DELTA`
  - `BLOCKED_BY_SPEC`

### Почему
Нельзя двигаться вслепую. Нужно видеть, где цифры обязаны совпасть, а где legacy надо ломать сознательно.

### Чек-лист приёмки
- [ ] Отчёт строится автоматически.
- [ ] Для каждого стата есть classification.
- [ ] Known deltas снабжены пояснением.
- [ ] Harness крутится в CI.

---

# ФАЗА 3 — Перенос low-risk статов

## TASK CM-P0-11 — Перенести low-risk placement/economics статы
**Приоритет:** P0

### Список
- `avg_finish_place`
- `avg_finish_place_ft`
- `avg_finish_place_no_ft`
- `final_table_reach`
- `itm`
- `roi`
- `roi_on_ft`
- `winnings_from_itm`

### Что сделать
- Реализовать их через exact primitives.
- Сразу убрать dict/object баги legacy.

### Чек-лист приёмки
- [ ] Для каждого стата есть spec entry.
- [ ] Есть unit-тесты на positive/negative/partial cases.
- [ ] Parity harness показывает либо `PARITY_OK`, либо documented delta.
- [ ] API отдаёт value+status.

---

## TASK CM-P1-12 — Перенести mid-risk FT/result статы
**Приоритет:** P1

### Список
- `avg_ft_initial_stack`
- `early_ft_bust`
- `pre_ft_ko`
- `incomplete_ft_percent`
- `deep_ft_stat`
- `pre_ft_chipev`

### Что сделать
- Переписать поверх FT-boundary primitives.
- Убрать implicit assumptions из legacy-кода.

### Чек-лист приёмки
- [ ] Все расчёты опираются на canonical FT-entry/deep-FT facts.
- [ ] Для missing HH/TS есть `partial/blocked` режим, а не silent zero.
- [ ] Есть edge-case tests для incomplete FT и boundary-hands.

---

# ФАЗА 4 — Переписать high-risk KO/stage family

## TASK CM-P0-13 — Переписать `early_ft_ko` и всё stage KO family
**Приоритет:** P0

### Список
- `early_ft_ko`
- `ko_stage_2_3`
- `ko_stage_3_4`
- `ko_stage_4_5`
- `ko_stage_5_6`
- `ko_stage_6_9`
- `ko_stage_7_9`

### Что сделать
- Явно решить: stages disjoint или sliding windows.
- Если sliding — назвать так в API/spec.
- Развести count/share/money.
- Исправить знаменатели attempts-per-stage.
- Убрать путаницу `amount = count`.

### Чек-лист приёмки
- [ ] Stage model описана явно.
- [ ] `amount` означает деньги только там, где это правда.
- [ ] Attempts denominator = количество турниров, достигших стадии.
- [ ] `pre_ft_ko` не течёт в ранний FT stat случайно.
- [ ] Для stage family есть dedicated tests.

---

## TASK CM-P0-14 — Переписать `big_ko` на exact per-award basis
**Приоритет:** P0

### Что сделать
- Считать big KO только по фактическим bounty awards.
- Не разлагать суммарную KO-сумму жадно.
- Материализовать individual award multipliers.

### Чек-лист приёмки
- [ ] Нет greedy decomposition по total KO sum.
- [ ] Каждый big KO traceable до конкретного award/event.
- [ ] Edge-cases с несколькими малыми bounty не образуют фиктивный x2/x10.
- [ ] Есть тесты на multiplier bins.

---

## TASK CM-P0-15 — Переписать `ko_luck`, `roi_adj`, `ko_contribution`
**Приоритет:** P0

### Что сделать
- Решить, нужны ли эти статы вообще в exact-v2.
- Если нужны — задать новую строгую модель expected KO value.
- Если строгая модель невозможна — пометить как heuristic и вынести из exact core.

### Почему
Сейчас это смесь фактических денег и слабой эвристики по средним KO.

### Чек-лист приёмки
- [ ] Для каждого стата есть formal definition.
- [ ] Ясно указано, exact это или heuristic.
- [ ] Если heuristic — он не выдаётся как exact truth.
- [ ] `roi_adj` не зависит от неявно потерянных kwargs.

---

# ФАЗА 5 — Пересобрать FT conversion family

## TASK CM-P1-16 — Переписать `ft_stack_conversion` family
**Приоритет:** P1

### Список
- `ft_stack_conversion`
- `ft_stack_conversion_attempts`
- `ft_stack_conversion_stage`

### Что сделать
- Expected KO считать **per tournament/per stage**, а не через общий median stack.
- Убрать sample mismatch.
- Либо удалить эвристику `2.5 attempts per KO`, либо вынести в явно heuristic-слой.

### Чек-лист приёмки
- [ ] Actual и expected считаются по одной и той же выборке.
- [ ] Нет median-based aggregation там, где нужна tournament-level модель.
- [ ] Attempts-adjustment имеет формальную интерпретацию или убран.
- [ ] Есть тесты на incomplete FT, missing stack data, missing HH.

---

# ФАЗА 6 — Интеграция в API и frontend

## TASK CM-P1-17 — Вывести новый stat-runtime в API/FT dashboard
**Приоритет:** P1

### Что сделать
- Подключить новые статы к API.
- Показывать status/provenance.
- Для parity-broken статов явно пометить upgraded semantics.

### Чек-лист приёмки
- [ ] API отдаёт новый envelope.
- [ ] FT dashboard не теряет ready/partial/blocked.
- [ ] Есть snapshot tests на API responses.
- [ ] Frontend не маскирует blocked как `0`.

---

## TASK CM-P1-18 — Усилить ingest/upload observability
**Приоритет:** P1

### Что сделать
- Добавить structured ingest diagnostics.
- Видеть, какой стат заблокирован из-за какого source gap.
- Явно хранить parse issues / uncertainty flags.

### Чек-лист приёмки
- [ ] На турнир можно открыть provenance trail.
- [ ] Видно, почему stat partial/blocked.
- [ ] Ошибки ingestion не превращаются в тихую недосчитанную статистику.

---

# ФАЗА 7 — Product hardening и только потом расширение продукта

## TASK CM-P1-19 — Auth / session / RLS / multitenancy hardening
**Приоритет:** P1

### Чек-лист приёмки
- [ ] Есть auth/session story.
- [ ] Есть row-level isolation или эквивалентная защита.
- [ ] Uploads/users разделены жёстко.
- [ ] Тесты на tenant isolation есть.

---

## TASK CM-P1-20 — Object storage / retention / quarantine
**Приоритет:** P1

### Чек-лист приёмки
- [ ] Сырые файлы и derived artifacts хранятся отдельно.
- [ ] Есть quarantine path для проблемного ingest.
- [ ] Retention policy описана.
- [ ] Повторный reprocess не ломает идемпотентность.

---

## TASK CM-P2-21 — Query/filter engine и advanced analytics
**Приоритет:** P2

### Чек-лист приёмки
- [ ] Query engine работает уже поверх exact primitives.
- [ ] Фильтры не дублируют доменную логику stat-runtime.
- [ ] Есть regression tests на сложные фильтры.

---

## TASK CM-P2-22 — Popup/HUD и прочий продуктовый UI
**Приоритет:** P2

### Почему так поздно
Пока exact semantics не зафиксированы, HUD будет просто красиво показывать потенциально неправильные числа.

### Чек-лист приёмки
- [ ] HUD питается только из stable exact API.
- [ ] У каждого показываемого стата есть status/provenance.
- [ ] Нет legacy-эвристик, спрятанных в UI.

---

## 9. Очерёдность миграции статов

### Волна 1
- avg_finish_place
- avg_finish_place_ft
- avg_finish_place_no_ft
- final_table_reach
- itm
- roi
- roi_on_ft
- winnings_from_itm

### Волна 2
- winnings_from_ko
- avg_ft_initial_stack
- early_ft_bust
- pre_ft_ko
- incomplete_ft_percent
- deep_ft_stat
- pre_ft_chipev
- total_ko
- avg_ko_per_tournament

### Волна 3
- early_ft_ko
- ko_stage_*
- ft_stack_conversion_stage

### Волна 4
- big_ko
- ko_luck
- roi_adj
- ko_contribution
- ft_stack_conversion
- ft_stack_conversion_attempts

---

## 10. Что code-agent должен делать, а что не должен

### Должен
- Работать от спецификации, а не от «как сейчас считает legacy».
- Для каждого стата сначала писать spec, потом tests, потом implementation.
- Разделять parity-preserving и intentional-parity-break cases.
- Выводить status/provenance вместе со значением.

### Не должен
- Переносить magic constants в новый runtime.
- Маскировать нехватку данных нулём.
- Пытаться «сохранить parity любой ценой» для явно плохих алгоритмов.
- Дублировать payout/KO-экономику в нескольких слоях.

---

## 11. Definition of Done для stat layer v2

Слой можно считать реально готовым, только если выполнены все пункты:

- [ ] Для всех 31 legacy-модулей есть spec entry.
- [ ] Для всех статов есть classification: preserve / reinterpret / replace.
- [ ] Для exact-статов нет скрытых эвристик внутри runtime.
- [ ] Для heuristic-статов это явно помечено.
- [ ] Есть golden corpus + edge-case corpus.
- [ ] Есть parity/delta harness.
- [ ] Для Big KO нет greedy decomposition по total sum.
- [ ] Для KO/stage family нет путаницы count/share/money.
- [ ] Для FT conversion family actual и expected считаются по одной выборке.
- [ ] API/Frontend различают `0` и `blocked/partial`.
- [ ] Документация проекта синхронизирована с реальным состоянием.

---

## 12. Мой итоговый вердикт

### По `Check_Mate`
Проект уже прошёл стадию пустого каркаса. Основа хорошая. Но **слой корректности чисел ещё не закрыт**, и именно там сейчас основной технический долг.

### По `MBR_Stats`
Legacy-набор полезен как:
- каталог доменных идей,
- источник ожидаемых метрик,
- база для parity harness.

Но он **не годится как прямой источник истины для exact-runtime**. Часть статов там нормальные и легко переносимы, а часть — откровенно эвристические или математически слабые.

### Самое важное решение
Новый runtime должен переносить **смыслы**, а не переносить **ошибки legacy-кода**.
