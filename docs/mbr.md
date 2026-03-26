# Аудит `Check_Mate` и текущего переноса `MBR Stats`
Дата: 2026-03-25

## 1. Что именно я проверял

Проверка была не по README «на слово», а по коду и формальному каталогу внутри репозитория:

- `README.md`, `docs/STATUS_ASSESSMENT.md`, `docs/QUALITY_GATES.md`;
- `docs/stat_catalog/mbr_stats_inventory.yml`;
- `docs/stat_catalog/mbr_stats_spec_v1.yml`;
- миграции и seed;
- crates `tracker_parser_core`, `parser_worker`, `mbr_stats_runtime`;
- unit/integration tests, лежащие в репозитории;
- официальные текущие правила GG по Mystery Battle Royale для сверки FT/KO-money semantics.

## 2. Ограничения аудита — честно

1. Внешний репозиторий `https://github.com/disikk/MBR_Stats` из этой среды не открылся.  
   Поэтому «супер тщательная» сверка stat-алгоритмов сделана по **замороженному внутреннему эталону** внутри `Check_Mate`:  
   `docs/stat_catalog/mbr_stats_inventory.yml` + `docs/stat_catalog/mbr_stats_spec_v1.yml`.

2. В этом sandbox нет установленного `cargo`, поэтому я **не мог сам заново прогнать Rust tests**.  
   Я проверял:
   - сам код;
   - сами тесты;
   - repo-документы, где зафиксированы уже выполненные runtime/test passes.

Это не мешает находить реальные логические ошибки и несоответствия контрактам, но важно не притворяться, будто я лично здесь rerun-ил весь test suite.

## 3. Короткий итог

Мой независимый вывод такой:

- **стадия проекта в целом**: всё ещё **foundation / narrow alpha**;
- **но README уже отстаёт от реального состояния backend-core**;
- **exact-core и первый канонический tranche MBR-статов ушли заметно дальше, чем это описано на главной странице**;
- **главный оставшийся blocker — не “отсутствие stat-layer”, а корректность KO-credit / KO-money semantics и несколько жёстких контрактов вокруг pre-FT и ordering**.

### Моя оценка стадии по слоям

Это именно **оценка аудита**, а не официальная цифра из репозитория:

- продукт целиком: **примерно 25–35%**;
- backend data-core в текущем GG MBR scope: **примерно 60–70%**;
- stat-runtime как **доверяемый аналитический слой**: **примерно 45–55%**, пока не закрыты P0 по KO semantics и `pre_ft_chipev`.

Иными словами: это уже **не пустой каркас**, но ещё **не beta** и точно **не production-grade tracker**.

## 4. Что уже сделано лучше, чем это описано в старых текстах репозитория

Ниже — вещи, которые в коде уже есть, хотя часть старых audit-документов их описывает как отсутствующие или очень ранние.

1. **Есть реальный движок позиций**, а не пустышка.  
   См. `backend/crates/tracker_parser_core/src/positions.rs:27-216`.

2. **Есть слой проверки action-legality**, и normalizer его действительно вызывает.  
   См. `backend/crates/tracker_parser_core/src/normalizer.rs:37-43` и `backend/crates/tracker_parser_core/src/betting_rules.rs`.

3. **Boundary/stage logic уже не та старая “last 5-max heuristic”**, о которой пишут старые аудиты.  
   Сейчас в коде есть `timeline_last_non_ft_candidate_v2`.  
   См. `backend/crates/parser_worker/src/local_import.rs:1698-1746`.

4. **Runtime stat layer уже не ограничен только 5–7 seed-метриками.**  
   По факту:
   - в inventory описано: **31** legacy-модулей;
   - frozen spec содержит: **31** модулей;
   - frozen spec содержит: **60** уникальных canonical stat keys;
   - runtime code покрывает ровно **60** keys;
   - missing keys относительно spec: **0**;
   - extra keys относительно spec: **0**.

5. **Big KO branch сделан честнее, чем можно было ожидать на этой стадии**:  
   текущая логика не притворяется, будто знает exact multiplier по суммарной mystery-сумме; она считает **expected value / bucket mass** через official envelope frequencies.  
   См. `backend/crates/mbr_stats_runtime/src/big_ko.rs:40-74` и `backend/crates/mbr_stats_runtime/src/split_bounty.rs:19-57`.

## 5. Где документация уже расходится с кодом

Это отдельная проблема, потому что она будет ломать планирование.

### 5.1. README уже занижает фактический объём stat-runtime

README всё ещё пишет, что runtime stat layer — «очень ранний seed-safe срез».  
По коду это уже не так: есть canonical snapshot на **60 ключей**, покрывающий **31** frozen module.

### 5.2. Inventory вводит в заблуждение

В `docs/stat_catalog/mbr_stats_inventory.yml` все **31** модуля помечены как `inventory_only`.  
Но frozen spec уже помечает все **31** как `mapped`, и runtime code реально эти ключи строит.

### 5.3. Старые audit-файлы уже частично исторические

Часть старых файлов в `docs/` всё ещё утверждает:
- что нет positions engine;
- что нет action-legality engine;
- что stage logic держится на старом heuristic;
- что runtime stat layer — только seed slice.

По текущему коду это уже не соответствует реальности.

## 6. Статус MBR Stats: что реально реализовано

### 6.1. Формальный статус по frozen spec

- frozen spec модулей: **31**
- canonical stat keys: **60**
- из них `coverage_limited_exact`: **45**
- из них `estimated`: **15**

### 6.2. Что можно считать уже безопасным к использованию

Эти ключи выглядят честными уже сейчас, потому что они не завязаны на спорный KO-credit substrate:

`avg_finish_place`, `itm_percent`, `ko_contribution_percent`, `roi_pct`, `winnings_from_itm`, `winnings_from_ko_total`

### 6.3. Что выглядит условно корректным, но только в рамках текущего committed GG scope

Эти ключи зависят от FT-helper / stage ordering / current-format assumptions.  
Для текущего формата они выглядят рабочими, но не должны объявляться как универсально доказанные:

`avg_finish_place_ft`, `avg_finish_place_no_ft`, `avg_ft_initial_stack_bb`, `avg_ft_initial_stack_chips`, `deep_ft_avg_stack_bb`, `deep_ft_avg_stack_chips`, `deep_ft_reach_percent`, `deep_ft_roi_pct`, `early_ft_bust_count`, `early_ft_bust_per_tournament`, `final_table_reach_percent`, `incomplete_ft_percent`, `roi_on_ft_pct`

### 6.4. Что пока нельзя считать доверенным аналитическим слоем

Эти ключи сейчас нужно считать **provisional**, пока не закрыты P0/P1 из этого отчёта:

`avg_ko_attempts_per_ft`, `avg_ko_event_per_tournament`, `big_ko_x10000_count`, `big_ko_x1000_count`, `big_ko_x100_count`, `big_ko_x10_count`, `big_ko_x1_5_count`, `big_ko_x2_count`, `early_ft_ko_event_count`, `early_ft_ko_event_per_tournament`, `ft_stack_conversion`, `ft_stack_conversion_3_4`, `ft_stack_conversion_3_4_attempts`, `ft_stack_conversion_5_6`, `ft_stack_conversion_5_6_attempts`, `ft_stack_conversion_7_9`, `ft_stack_conversion_7_9_attempts`, `ko_attempts_success_rate`, `ko_contribution_adjusted_percent`, `ko_luck_money_delta`, `ko_stage_2_3_attempts_per_tournament`, `ko_stage_2_3_event_count`, `ko_stage_2_3_money_total`, `ko_stage_3_4_attempts_per_tournament`, `ko_stage_3_4_event_count`, `ko_stage_3_4_money_total`, `ko_stage_4_5_attempts_per_tournament`, `ko_stage_4_5_event_count`, `ko_stage_4_5_money_total`, `ko_stage_5_6_attempts_per_tournament`, `ko_stage_5_6_event_count`, `ko_stage_5_6_money_total`, `ko_stage_6_9_event_count`, `ko_stage_6_9_money_total`, `ko_stage_7_9_attempts_per_tournament`, `ko_stage_7_9_event_count`, `ko_stage_7_9_money_total`, `pre_ft_chipev`, `pre_ft_ko_count`, `roi_adj_pct`, `total_ko_event_count`

## 7. Матрица по всем 31 legacy-модулям

Легенда:
- **OK** — выглядит честно уже сейчас;
- **УСЛОВНО** — формула выглядит правильной, но есть stage/helper/order assumptions;
- **РИСК** — формула может совпадать со spec, но её субстрат сейчас слишком хрупкий или вероятно неверный;
- **СМЕШАННО** — внутри одного legacy-модуля часть ключей уже нормальна, часть пока нет.

| Legacy module | Canonical keys | Вердикт | Комментарий |
|---|---|---|---|
| `avg_finish_place` | `avg_finish_place` | **OK** | Формула и субстрат выглядят корректно: summary-only, без stage/KO эвристик. |
| `avg_finish_place_ft` | `avg_finish_place_ft` | **УСЛОВНО** | Формула корректна по frozen spec, но зависит от FT-helper и порядка рук; для текущего формата выглядит честно. |
| `avg_finish_place_no_ft` | `avg_finish_place_no_ft` | **УСЛОВНО** | Формула корректна по frozen spec, но зависит от FT-helper и порядка рук. |
| `avg_ft_initial_stack` | `avg_ft_initial_stack_chips, avg_ft_initial_stack_bb` | **УСЛОВНО** | Формула корректна по frozen spec, но зависит от точности first_ft_hand и сортировки по local timestamp/external id. |
| `avg_ko_per_tournament` | `avg_ko_event_per_tournament` | **РИСК** | Формула совпадает со spec, но субстрат derived.hand_eliminations.hero_involved вероятно неверно кредитует KO при side-pot кейсах. |
| `big_ko` | `big_ko_x1_5_count, big_ko_x2_count, big_ko_x10_count, big_ko_x100_count, big_ko_x1000_count, big_ko_x10000_count` | **РИСК** | Ожидаемые bucket-метрики реализованы честно как expected value, но полностью зависят от корректности KO-share semantics; сейчас риск высокий. |
| `deep_ft_stat` | `deep_ft_reach_percent, deep_ft_avg_stack_chips, deep_ft_avg_stack_bb, deep_ft_roi_pct` | **УСЛОВНО** | Формулы корректны по spec, но deepest/entry метрики зависят от stage-helper и хронологии local timestamps. |
| `early_ft_bust` | `early_ft_bust_count, early_ft_bust_per_tournament` | **УСЛОВНО** | Формула выглядит корректно, но stage-классификация опирается на текущий FT-helper/форматный контракт. |
| `early_ft_ko` | `early_ft_ko_event_count, early_ft_ko_event_per_tournament` | **РИСК** | Формула совпадает со spec, но exact KO event substrate под риском из-за side-pot KO-credit semantics. |
| `final_table_reach` | `final_table_reach_percent` | **УСЛОВНО** | Формула корректна по spec; риск — жёсткое допущение FT = 9-max и локальная хронология. |
| `ft_stack_conversion` | `ft_stack_conversion` | **РИСК** | Зависит от ранних FT KO event counts; при неверном KO-credit side-pot логика загрязняется. |
| `ft_stack_conversion_attempts` | `avg_ko_attempts_per_ft, ko_attempts_success_rate` | **РИСК** | Current implementation of KO attempts — эвристика, а не доказанный exact event model; exactness claim сейчас завышен. |
| `ft_stack_conversion_stage` | `ft_stack_conversion_7_9, ft_stack_conversion_7_9_attempts, ft_stack_conversion_5_6, ft_stack_conversion_5_6_attempts, ft_stack_conversion_3_4, ft_stack_conversion_3_4_attempts` | **РИСК** | Содержит те же attempt-эвристики + stage/order dependencies; нужен formal event model. |
| `incomplete_ft_percent` | `incomplete_ft_percent` | **УСЛОВНО** | Формула корректна, но зависит от first_ft_hand и допущения текущего формата. |
| `itm` | `itm_percent` | **OK** | Summary-only, выглядит корректно. |
| `ko_contribution` | `ko_contribution_percent, ko_contribution_adjusted_percent` | **СМЕШАННО** | Внутри модуля смешанная ситуация: ko_contribution_percent summary-based и выглядит корректно; adjusted-percent зависит от KO-money posterior и side-pot semantics. |
| `ko_luck` | `ko_luck_money_delta` | **РИСК** | Полностью зависит от корректности KO-money semantics и official envelope mapping. |
| `ko_stage_2_3` | `ko_stage_2_3_event_count, ko_stage_2_3_money_total, ko_stage_2_3_attempts_per_tournament` | **РИСК** | Event part зависит от KO-credit semantics; money part дополнительно зависит от KO-share semantics и envelope weighting. |
| `ko_stage_3_4` | `ko_stage_3_4_event_count, ko_stage_3_4_money_total, ko_stage_3_4_attempts_per_tournament` | **РИСК** | То же: event part под риском, money part под ещё большим риском. |
| `ko_stage_4_5` | `ko_stage_4_5_event_count, ko_stage_4_5_money_total, ko_stage_4_5_attempts_per_tournament` | **РИСК** | То же: event part под риском, money part под ещё большим риском. |
| `ko_stage_5_6` | `ko_stage_5_6_event_count, ko_stage_5_6_money_total, ko_stage_5_6_attempts_per_tournament` | **РИСК** | То же: event part под риском, money part под ещё большим риском. |
| `ko_stage_6_9` | `ko_stage_6_9_event_count, ko_stage_6_9_money_total` | **РИСК** | То же: event part под риском, money part под ещё большим риском. |
| `ko_stage_7_9` | `ko_stage_7_9_event_count, ko_stage_7_9_money_total, ko_stage_7_9_attempts_per_tournament` | **РИСК** | То же: event part под риском, money part под ещё большим риском. |
| `pre_ft_chipev` | `pre_ft_chipev` | **РИСК** | Есть явный bias bug: отсутствие pre-FT snapshot превращается в synthetic zero delta через fallback 1000 -> 0. |
| `pre_ft_ko` | `pre_ft_ko_count` | **РИСК** | Зависит и от boundary/FT helper, и от KO-credit semantics. |
| `roi` | `roi_pct` | **OK** | Summary-only, выглядит корректно. |
| `roi_adj` | `roi_adj_pct` | **РИСК** | Полностью зависит от adjusted KO-money branch; до исправления side-pot semantics не считать доверенным. |
| `roi_on_ft` | `roi_on_ft_pct` | **УСЛОВНО** | Формула корректна по spec, но зависит от FT reach/helper. |
| `total_ko` | `total_ko_event_count` | **РИСК** | Exact KO event count зависит от корректности hero_involved semantics в derived.hand_eliminations. |
| `winnings_from_itm_stat` | `winnings_from_itm` | **OK** | Summary-only, выглядит корректно. |
| `winnings_from_ko_stat` | `winnings_from_ko_total` | **OK** | Summary-only (`total_payout - regular_prize`), выглядит корректно. |

## 8. Главные найденные проблемы

### CM-P0-01 — P0 — KO-credit semantics в side-pot кейсах, вероятно, противоречит официальному правилу GG

**Что нашёл.**
- `tracker_parser_core/src/normalizer.rs:403-490` — `build_elimination()` берёт union всех `resolved_by_pot_nos`, победителей и объёмы по всем pot'ам busted player.
- `mbr_stats_runtime/src/queries.rs:369-518, 720-747` — downstream count/money stats читают `hero_involved` и `hero_share_fraction` из `derived.hand_eliminations` как источник exact KO-credit.
- Официальное правило GG: если chips busted player есть в side pots, bounty делится только между winners последнего side pot.

**Что ломает.**
- Риск загрязнения `total_ko_event_count`, `avg_ko_event_per_tournament`, `early_ft_ko_*`, всех `ko_stage_*` event/money метрик, `ft_stack_conversion*`, `pre_ft_ko_count`, `big_ko_*`, `ko_contribution_adjusted_percent`, `ko_luck_money_delta`, `roi_adj_pct`.

### CM-P0-02 — P0 — `pre_ft_chipev` имеет явный bias bug на отсутствующем snapshot

**Что нашёл.**
- `mbr_stats_runtime/src/queries.rs:797-800` — `COALESCE(pre_ft_snapshot.hero_final_stack, 1000::bigint) - 1000::bigint`.

**Что ломает.**
- Искажается `pre_ft_chipev`: турниры без exact pre-FT snapshot превращаются в искусственный нулевой delta вместо исключения из denominator.

### CM-P1-03 — P1 — Attempt-based метрики сейчас опираются на heuristic query, а не на доказанный exact event model

**Что нашёл.**
- `mbr_stats_runtime/src/queries.rs:542-575` — attempt определяется через target all-in + shared pot eligibility + hero starting_stack >= target starting_stack.

**Что ломает.**
- Под риском `avg_ko_attempts_per_ft`, `ko_attempts_success_rate`, `ko_stage_*_attempts_per_tournament`, `ft_stack_conversion_*_attempts`.

### CM-P1-04 — P1 — Stage/helper/pre-FT ordering опирается на raw local timestamp strings и secondary sort по external ids

**Что нашёл.**
- `parser_worker/src/local_import.rs:1700-1704`, `1857-1860` — boundary/FT helper сортируют по строковым local timestamps.
- `mbr_stats_runtime/src/queries.rs:433-437`, `658-662`, `683-686`, `827-830` — deep FT, stage entries и pre-FT snapshot используют ту же ordering strategy.

**Что ломает.**
- Условный риск для всех FT/deep-FT/stage/pre-FT метрик вне текущего committed scope.

### CM-P1-05 — P1 — FT detection жёстко захардкожен как `max_players == 9`

**Что нашёл.**
- `parser_worker/src/local_import.rs:1706`, `1759-1765`.
- Для текущего GG MBR это выглядит совместимым с официальными правилами, но не должно жить как безымянная магическая константа по коду.

**Что ломает.**
- Риск тихого drift при future format changes / historical variants.

### CM-P1-06 — P1 — Документация и инвентарь отстают от реального кода

**Что нашёл.**
- `README.md` описывает runtime stat layer как очень ранний seed-safe slice.
- `docs/stat_catalog/mbr_stats_inventory.yml` помечает все 31 модуля как `inventory_only`.
- Frozen spec и runtime code уже содержат 31 mapped module / 60 keys.

**Что ломает.**
- Высокий риск неверных решений для следующего разработчика или код-агента.

### CM-P1-07 — P1 — Нет жёсткого end-to-end golden gate на полный canonical snapshot

**Что нашёл.**
- `mbr_stats_runtime/src/queries.rs` содержит хорошие formula tests, но не заменяет import→DB→query golden proof на полный 60-key snapshot.
- `docs/STATUS_ASSESSMENT.md` прямо говорит, что extended real corpus за пределами committed pack пока не доказан runtime-исполнением.

**Что ломает.**
- Риск тихих регрессий при правках KO semantics, stage ordering и helper logic.


## 9. Самые важные технические выводы по алгоритмам статов

### 9.1. По frozen spec runtime сделан гораздо дальше, чем кажется по README

На уровне **формул и key coverage** картина хорошая:
- canonical snapshot реально собирает весь frozen набор;
- `31` module / `60` key parity выглядит полной;
- event-count и money metrics в spec уже разведены;
- Big KO трактуется как expected-value family, а не как fake exact.

То есть проблема сейчас **не в том, что stat-engine “ещё не начат”**.

### 9.2. Самая опасная зона — не сами формулы, а семантика KO-credit

Это важный нюанс.

По frozen spec многие формулы корректны **при условии**, что `derived.hand_eliminations` честно говорит:
- кто реально получил KO-credit;
- в каком размере;
- относится ли это к split;
- относится ли это к side-pot KO.

Но в текущем коде именно этот substrate, скорее всего, искажается в части side-pot semantics.

Именно поэтому много статов у меня помечены как **РИСК**, даже если их формула по YAML/spec выглядит правильной.

### 9.3. `pre_ft_chipev` — уже не «спорный», а просто багнутый по denominator contract

Тут проблема жёстче, чем “возможная эвристика”.
`COALESCE(..., 1000) - 1000` означает, что отсутствие snapshot превращается в synthetic `0`.
Такой fallback не соответствует смыслу frozen spec и должен быть удалён.

### 9.4. Attempt-based метрики сейчас переобъявлены как слишком точные

`avg_ko_attempts_per_ft`, `ko_attempts_success_rate`, `ko_stage_*_attempts_per_tournament`, `ft_stack_conversion_*_attempts` хотят быть `coverage_limited_exact`.  
Но текущая query-логика пока не доказывает это exactness-утверждение.

Пока не появится formal `hand_ko_attempts` contract, эти метрики нужно считать как минимум условными.

## 10. Что делать прямо сейчас

Жёсткий приоритет такой:

1. **Не расширять UI/popup/new stats раньше P0 fix'ов.**
2. **Сначала починить KO-credit semantics.**
3. **Потом починить `pre_ft_chipev`.**
4. **Потом закрепить parity/spec/golden gates.**
5. **Потом уже масштабировать stage/attempt/extended corpus.**

Иначе проект начнёт ускорять слой представления поверх не до конца доказанного data-core.

## 11. План для код-агента по фазам

Ниже — тот самый рабочий план, который можно отдавать код-агенту как backlog.  
Задачи упорядочены по фазам и приоритетам.


## F0. Фиксация истины и автоматических контрактов

### F0-T1 — P0 — Добавить автоматическую проверку parity: frozen spec ↔ runtime keys

**Зачем.** Сейчас статический аудит показывает полное совпадение 31 модулей / 60 ключей, но это не зафиксировано в CI и может сломаться тихо.

**Файлы / зоны кода.**
- `backend/crates/mbr_stats_runtime/tests/spec_parity.rs`
- `docs/stat_catalog/mbr_stats_spec_v1.yml`
- `backend/crates/mbr_stats_runtime/src/models.rs`
- `backend/crates/mbr_stats_runtime/src/queries.rs`

**Шаги.**
- Прочитать `docs/stat_catalog/mbr_stats_spec_v1.yml` в тесте и извлечь полный список `new_stat_keys`.
- Построить runtime-список канонических ключей из seed snapshot + canonical snapshot + Big KO bucket keys.
- Падать тестом при любом missing/extra key, дубликате ключа или изменении числа модулей/ключей без явного обновления spec.
- Добавить этот тест в обязательный backend check.

**Чек-лист приёмки.**
- [ ] CI падает при любом расхождении между frozen spec и runtime surface.
- [ ] Тест явно подтверждает `31` модуль и `60` уникальных stat keys.
- [ ] Нет missing keys и нет extra keys относительно frozen spec.

### F0-T2 — P1 — Синхронизировать README / inventory / status / audit docs с реальным кодом

**Зачем.** Документация уже отстаёт: README всё ещё описывает runtime stat layer как очень ранний seed-safe slice, inventory помечает все 31 модуля как `inventory_only`, а код и frozen spec уже реализуют полную первую каноническую tranche.

**Файлы / зоны кода.**
- `README.md`
- `docs/STATUS_ASSESSMENT.md`
- `docs/QUALITY_GATES.md`
- `docs/stat_catalog/mbr_stats_inventory.yml`
- `docs/check_mate_audit_2026-03-25_ru.md`
- `docs/mbr_audit.md`

**Шаги.**
- Выбрать один источник истины для текущего runtime status (`mbr_stats_spec_v1.yml`).
- Обновить README: убрать формулировку про 'seed-only slice', явно описать текущую первую canonical tranche и её блокеры.
- Либо обновить `status` в `mbr_stats_inventory.yml`, либо явно переименовать поле, чтобы оно не притворялось runtime-status.
- Удалить или пометить как исторические все утверждения про отсутствие positions engine, action-legality engine и stage v2.

**Чек-лист приёмки.**
- [ ] Ни один документ больше не утверждает, что runtime stat layer — это только seed-safe slice, если в коде уже есть canonical 60-key snapshot.
- [ ] Inventory/status/spec больше не противоречат друг другу по текущему состоянию 31 модулей.
- [ ] Старые аудиты либо удалены, либо явно помечены как historical/stale.

### F0-T3 — P1 — Зафиксировать внешний upstream-срез MBR_Stats, если репозиторий станет доступен

**Зачем.** Текущий аудит пришлось делать по frozen spec внутри Check_Mate, потому что внешний `disikk/MBR_Stats` из этой среды не открылся. Для полной трассируемости нужен vendored snapshot или semantic diff.

**Файлы / зоны кода.**
- `docs/stat_catalog/upstream_mbr_stats_snapshot/`
- `docs/stat_catalog/upstream_diff_report.md`

**Шаги.**
- Если upstream снова доступен, сохранить snapshot исходного каталога в репозиторий или отдельный артефакт.
- Построить явный diff: legacy formulas/outputs ↔ frozen `mbr_stats_spec_v1.yml`.
- Зафиксировать все осознанные rename/split/merge decisions.

**Чек-лист приёмки.**
- [ ] Есть воспроизводимый upstream snapshot или machine-readable diff report.
- [ ] Каждый legacy stat-module имеет трассируемое соответствие frozen spec.

## F1. Исправление KO-credit / KO-money semantics

### F1-T1 — P0 — Пересобрать KO-credit semantics под официальное правило GG для side-pot кейсов

**Зачем.** Сейчас `build_elimination()` агрегирует все pot'ы, в которые внёсся busted player, и берёт winners по всем этим pot'ам. Официальное правило GG говорит иначе: если chips busted player есть в side pots, bounty делится только между winners последнего side pot.

**Файлы / зоны кода.**
- `backend/crates/tracker_parser_core/src/normalizer.rs`
- `backend/migrations/0010_hand_eliminations_ko_v2.sql`
- `backend/crates/tracker_parser_core/src/models.rs`
- `backend/crates/parser_worker/src/local_import.rs`

**Шаги.**
- Ввести явное понятие `ko_credit_pot_no` (или эквивалентное поле) в `derived.hand_eliminations`.
- Определять credit pot так: если busted player внёсся хотя бы в один side pot, использовать максимальный `pot_no`, содержащий его chips; иначе использовать main/единственный pot.
- Считать `hero_involved`, `hero_share_fraction`, `joint_ko`, `split_n`, `ko_involved_winners` только по credit pot, а не по union всех pot'ов.
- Оставить `resolved_by_pot_nos` как диагностический след, но не как основание для KO-credit.
- Сохранить reason-coded uncertainty для реально ambiguous winner mappings.

**Чек-лист приёмки.**
- [ ] Кейс 'Hero выиграл main pot, но проиграл последний side pot' больше не даёт Hero KO-credit.
- [ ] Кейс split на последнем side pot делит KO-credit только между winners этого last side pot.
- [ ] Поля `hero_involved`, `hero_share_fraction`, `joint_ko`, `is_sidepot_based` отражают именно KO-credit pot.
- [ ] Committed pack и synthetic edge-pack не получают новых invariant errors.

### F1-T2 — P0 — Переподключить все KO-derived stats к новой KO-credit модели и пересчитать goldens

**Зачем.** После исправления KO-credit потянутся downstream-изменения во всех event/money/adjusted/Big KO метриках.

**Файлы / зоны кода.**
- `backend/crates/mbr_stats_runtime/src/queries.rs`
- `backend/crates/mbr_stats_runtime/src/big_ko.rs`
- `backend/crates/mbr_stats_runtime/src/split_bounty.rs`
- `backend/crates/mbr_stats_runtime/src/models.rs`
- `backend/crates/mbr_stats_runtime/tests/`

**Шаги.**
- Аудировать все SQL loaders, которые читают `derived.hand_eliminations`: `load_tournament_ko_event_facts`, `load_stage_event_facts`, `load_tournament_ko_money_event_facts`.
- Убедиться, что event metrics и money metrics используют одну и ту же новую KO-credit semantics, но не смешивают count и money logic.
- Обновить формулы adjusted KO / ROI / Big KO только на supported exact Hero KO-credit events.
- Пересчитать golden snapshots для committed fixtures и новых synthetic fixtures.

**Чек-лист приёмки.**
- [ ] Список затронутых ключей задокументирован и покрыт тестами.
- [ ] Summary-only метрики (`roi_pct`, `itm_percent`, `winnings_from_*`, `ko_contribution_percent`) не меняются от KO-credit fix.
- [ ] Все KO-derived metric changes проходят через осознанный golden diff review.

### F1-T3 — P1 — Заменить heuristic KO-attempt query на formal exact attempt model

**Зачем.** Текущая логика attempt'ов (`all-in у target` + `shared pot eligibility` + `hero starting_stack >= target starting_stack`) недостаточна для claim'а `exact` и даёт риск false positives.

**Файлы / зоны кода.**
- `backend/crates/mbr_stats_runtime/src/queries.rs`
- `backend/crates/tracker_parser_core/src/normalizer.rs`
- `backend/migrations/*hand_ko_attempts*.sql`
- `docs/stat_catalog/mbr_stats_spec_v1.yml`

**Шаги.**
- Определить formal contract для exact KO attempt на уровне hand-target pair.
- Материализовать `derived.hand_ko_attempts` или эквивалентный derived surface с reason-coded exact/uncertain states.
- Исключить ложные попытки: hero folded, hero не live в KO-credit pot, hero не cover'ил relevant all-in contribution target'а.
- Если exactness доказать нельзя, downgrade exactness class attempt-based metrics или пометить их as estimated/provisional до доказательства.

**Чек-лист приёмки.**
- [ ] Кейс 'target jam, hero fold' не считается KO attempt.
- [ ] Кейс 'shared eligibility without real KO contest' не считается KO attempt.
- [ ] Каждая попытка трассируется до hand_id, target_seat_no и credit pot.
- [ ] Attempt-based metrics больше не заявляют `exact`, если exactness не доказан формально.

## F2. Stage/boundary/pre-FT hardening

### F2-T1 — P0 — Исправить `pre_ft_chipev`: убрать synthetic zero fallback и починить denominator contract

**Зачем.** Сейчас отсутствие pre-FT snapshot превращается в `COALESCE(hero_final_stack, 1000) - 1000`, что даёт искусственный 0 chip delta и искажает среднее.

**Файлы / зоны кода.**
- `backend/crates/mbr_stats_runtime/src/queries.rs`
- `backend/crates/parser_worker/src/local_import.rs`
- `docs/stat_catalog/mbr_stats_spec_v1.yml`
- `backend/crates/mbr_stats_runtime/tests/`

**Шаги.**
- Убрать fallback `1000::bigint` из `load_pre_ft_chip_facts`.
- В denominator включать только турниры с реальным exact pre-FT snapshot или с доказанным no-FT snapshot, а не просто helper-row.
- При uncertain boundary исключать турнир из denominator, а не занулять.
- При необходимости добавить explicit field `has_pre_ft_snapshot_exact` на helper/derived layer.

**Чек-лист приёмки.**
- [ ] `pre_ft_chipev` не меняется из-за отсутствующего snapshot через искусственный 0.
- [ ] Турниры без exact pre-FT coverage исключаются из denominator.
- [ ] Есть тесты на 4 кейса: exact boundary, uncertain boundary, no-FT with snapshot, no-FT without snapshot.

### F2-T2 — P1 — Перестать опираться на raw local timestamp strings как на primary ordering substrate

**Зачем.** Boundary, first FT hand, deep FT entry, stage-entry и pre-FT snapshot сейчас завязаны на сортировку строковых local timestamps + external hand id. Это хрупко и не даёт честного proof-level ordering outside committed scope.

**Файлы / зоны кода.**
- `backend/crates/parser_worker/src/local_import.rs`
- `backend/crates/mbr_stats_runtime/src/queries.rs`
- `backend/migrations/*hand_order*.sql`
- `backend/crates/tracker_parser_core/src/models.rs`

**Шаги.**
- Ввести стабильный ordering/provenance key для рук внутри турнира: source-member order, hand index in file, normalized parsed timestamp, optional cross-file tie class.
- Использовать этот key во всех stage/helper/stat queries вместо raw string sort.
- Сохранять ambiguity, если несколько столов реально неупорядочиваемы, а не придумывать exact order.

**Чек-лист приёмки.**
- [ ] Нет stat-critical `ORDER BY hand_started_at_local` без явного stable order key.
- [ ] Boundary/entry/pre-FT helpers используют deterministic ordering substrate.
- [ ] Если порядок доказать нельзя, state остаётся `uncertain`, а не превращается в guessed exact.

### F2-T3 — P1 — Вынести FT/stage detection в rulepack, а не держать по коду `max_players == 9`

**Зачем.** Для текущего GG MBR правило 9-player final table выглядит корректным, но жёсткое хардкодирование формата в нескольких местах сделает будущий drift или historical variants опасно невидимыми.

**Файлы / зоны кода.**
- `backend/crates/parser_worker/src/local_import.rs`
- `backend/crates/mbr_stats_runtime/src/registry.rs`
- `backend/crates/tracker_parser_core/src/models.rs`
- `backend/seeds/0001_reference_data.sql`

**Шаги.**
- Добавить room/rulepack abstraction для MBR stage semantics.
- Сконцентрировать FT detection и stage predicates в одном месте.
- Задать current GG rulepack: FT starts when 9 players remain; rush→shootout edge-cases описаны явно.
- Заменить прямые `max_players == 9` checks на rulepack calls.

**Чек-лист приёмки.**
- [ ] В stat-critical коде нет дублированных raw checks `max_players == 9`.
- [ ] Current GG committed fixtures дают те же stage labels после refactor.
- [ ] Historical/variant formats либо поддержаны, либо явно reason-coded as unsupported.

## F3. Доказательство корректности через fixtures и goldens

### F3-T1 — P1 — Расширить synthetic fixture suite на KO-credit, boundary и pre-FT edge cases

**Зачем.** Ключевые риски лежат именно в tricky semantic cases, которых может не быть в committed pack.

**Файлы / зоны кода.**
- `backend/crates/tracker_parser_core/tests/`
- `backend/crates/parser_worker/src/local_import.rs`
- `backend/crates/mbr_stats_runtime/tests/`
- `backend/fixtures/synthetic/`

**Шаги.**
- Добавить synthetic HH/TS cases: main-only split KO, last-sidepot KO, hero wins main not last sidepot, multiple busted players, same-timestamp boundary ambiguity, exact/no-FT pre-FT snapshot cases.
- Для каждого кейса зафиксировать expected `hand_eliminations`, stage helper rows и canonical stats deltas.
- Сделать round-trip import tests до Postgres.

**Чек-лист приёмки.**
- [ ] Каждый P0/P1 edge case имеет свой fixture и regression test.
- [ ] Synthetic suite проверяет и raw derived rows, и конечные canonical stats.
- [ ] Новые tricky cases не остаются только на уровне unit tests без import round-trip.

### F3-T2 — P1 — Добавить end-to-end golden tests для полного canonical snapshot (60 keys)

**Зачем.** Сейчас есть хорошие formula tests, но нет жёсткого интеграционного proof, что после import всё сходится на полном наборе stat keys.

**Файлы / зоны кода.**
- `backend/crates/mbr_stats_runtime/tests/canonical_snapshot_golden.rs`
- `backend/crates/parser_worker/tests/`
- `backend/scripts/run_backend_checks.sh`

**Шаги.**
- Поднять test DB, импортировать committed fixtures + synthetic edge fixtures.
- Вызвать `query_canonical_stats` и сохранить полный 60-key snapshot как golden output.
- Проверять не только значения, но и nullability/zero-denominator behavior на selected scenarios.
- Добавить diff-friendly golden review workflow.

**Чек-лист приёмки.**
- [ ] CI валит любые неожиданные изменения в canonical 60-key snapshot.
- [ ] Golden tests покрывают committed pack и synthetic edge-pack.
- [ ] Изменения goldens проходят явный review с объяснением причины.

### F3-T3 — P1 — Прогнать extended real corpus и собрать uncertainty report

**Зачем.** Даже хорошие committed fixtures не доказывают корректность на реальном широком корпусе GG.

**Файлы / зоны кода.**
- `backend/scripts/`
- `docs/runtime_uncertainty_report.md`
- `docs/STATUS_ASSESSMENT.md`

**Шаги.**
- Прогнать import + canonical stats на расширенном реальном корпусе.
- Собрать top classes: parse issues, uncertain winner mappings, boundary ambiguities, unsupported syntactic variants.
- Открыть backlog по реально встретившимся классам, а не только по синтетике.

**Чек-лист приёмки.**
- [ ] Есть отчёт по extended corpus с абсолютными counts и top reason codes.
- [ ] Нет silent ignore: каждый непройденный кейс либо supported, либо reason-coded unsupported/uncertain.
- [ ] После отчёта backlog обновлён фактами, а не предположениями.

## F4. Только после этого — public/API/UI productization

### F4-T1 — P2 — Очистить public stat/API surface и добавить provenance/exactness metadata

**Зачем.** После исправления core нужно научить downstream-слой честно сообщать пользователю exactness, coverage limits и estimation.

**Файлы / зоны кода.**
- `backend/crates/mbr_stats_runtime/src/models.rs`
- `backend/crates/mbr_stats_runtime/src/registry.rs`
- `frontend/*`
- `docs/api_stats_contract.md`

**Шаги.**
- Для каждого stat key возвращать value + exactness class + coverage note + dependency/version info.
- Не показывать runtime-only debug surfaces как public canonical stats.
- Маркировать estimated money/Big KO metrics соответствующим provenance badge.

**Чек-лист приёмки.**
- [ ] Любой stat key можно объяснить через provenance metadata без чтения кода.
- [ ] Runtime-only surfaces (`ft_stage_bucket` и т.п.) не притворяются public stat API.
- [ ] Estimated stats визуально и программно отличимы от coverage-limited exact stats.

### F4-T2 — P2 — Продолжать UI / popup / продуктовые фичи только после прохождения F1–F3

**Зачем.** Главный риск проекта — ускорить UI и новые статы раньше, чем data-core станет честно доказуемым.

**Файлы / зоны кода.**
- `frontend/`
- `docs/ROADMAP.md`
- `docs/QUALITY_GATES.md`

**Шаги.**
- Зафиксировать в roadmap зависимость: product/UI work не блокирует core hardening, а идёт после него.
- Сначала завершить KO semantics, pre-FT contract и regression gates.
- Только потом расширять popup, filters и school-facing API.

**Чек-лист приёмки.**
- [ ] В roadmap явно прописано, что F1–F3 — blocker для новых сложных stat/UI surfaces.
- [ ] Нет новых user-facing stat cards без пройденных provenance/exactness gates.


## 12. Непереговорные правила для следующей разработки

Это нужно принять как guardrails:

- нельзя использовать `ft_stage_bucket` как primary semantic substrate для новых canonical stats;
- нельзя выдавать `boundary_ko_*` как exact при non-exact boundary resolution;
- нельзя импутировать отсутствие snapshot константой;
- нельзя смешивать event-count metrics и money metrics в один и тот же output key;
- нельзя “догадкой” materialize-ить exact winners/KO там, где честный state — `uncertain`;
- нельзя добавлять новые user-facing stat surfaces, пока P0/P1 не закрыты regression-gates.

## 13. Итоговый управленческий вывод

Если смотреть трезво, проект сейчас находится в хорошем месте **для hardening core**, но в плохом месте **для ускорения продукта**.

Правильная стратегия продолжения разработки:
- не “дописывать ещё 20 статов”;
- не “ускорять фронт”;
- не “пилить popup поверх всего подряд”;
- а сначала сделать так, чтобы:
  1. KO-credit semantics были доказуемо верны;
  2. pre-FT denominator contracts были честны;
  3. canonical snapshot был прибит golden-гейтами;
  4. docs/spec/runtime больше не расходились.

Только после этого текущий stat-runtime можно будет считать не просто интересным foundation, а реально надёжной аналитической основой.

## 14. Коротко в одной фразе

**Check_Mate уже ушёл сильно дальше раннего seed-safe фундамента, но до доверяемого MBR analytics layer ему сейчас мешают не “отсутствующие статы”, а несколько очень конкретных semantic blockers: side-pot KO-credit, attempt exactness, pre-FT denominator и слабые regression-gates.**
