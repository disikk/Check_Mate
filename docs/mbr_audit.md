# Аудит репозитория `disikk/Check_Mate`

Дата: 2026-03-25  
Формат: технический аудит текущего состояния, корректности реализованного, особенно алгоритмов расчёта статов и базовых примитивов для набора MBR Stats.

---

## 1. Итоговый вердикт

Проект находится на стадии **фундамента / узкой альфы**, а не на стадии готового трекера и не на стадии полноценной миграции MBR Stats.

Что уже выглядит сильным:
- есть неплохой фундамент exact-core для разбора истории рук, нормализации и материализации признаков;
- уже есть рабочая база для tournament economics: buy-in / rake / regular prize / total payout / mystery money;
- есть аккуратная модель неопределённости: где система не уверена, она часто предпочитает не придумывать «точный» ответ;
- есть CI, миграции, seeds, backend-разделение на crates, committed fixture pack.

Что ещё **не готово**:
- полный набор MBR Stats **не реализован**;
- stage/boundary-логика вокруг перехода Rush Stage → Final Table реализована через жёсткий эвристический костыль;
- Big KO / KO-money модель пока не годится как корректный production-алгоритм;
- текущие 5 сидированных stat-метрик — это не миграция legacy MBR, а только безопасный стартовый слой прокси-метрик.

Моя общая оценка состояния:
- **архитектура ядра**: 7/10 для ранней фазы;
- **корректность exact-части по hand parsing / elimination foundation**: 7/10 в рамках узкого корпуса;
- **корректность stage/boundary части**: 3/10;
- **готовность набора MBR Stats**: 2/10;
- **готовность проекта как продукта**: 2/10.

Итог: проект имеет **хороший инженерный старт**, но ключевой доменный слой MBR Stats ещё в значительной степени **не построен**.

---

## 2. Что именно я проверял

Я проверял четыре слоя:

1. **Стадия проекта**: структура репозитория, документация, миграции, backend crates, seeds, CI, frontend-заготовка.  
2. **Корректность уже сделанного**: parser, normalizer, elimination attribution, stage resolution, economics, runtime materialization.  
3. **Супер тщательно — алгоритмы для MBR Stats**: что реально реализовано, что только inventory-only, где формулы корректны, а где пока только прокси.  
4. **План продолжения**: какие задачи нужно отдавать код-агенту, в каком порядке, с какими критериями приёмки.

Важно: я делал **технический аудит кода и структуры**, а не доверял README на слово. Местами README и docs чуть оптимистичнее реального состояния runtime.

---

## 3. Краткая оценка текущей стадии разработки

### 3.1. Что видно по репозиторию

Сейчас это именно **foundation snapshot**:
- есть базовая структура проекта;
- есть backend с несколькими crates;
- есть миграции и seeds;
- есть fixture pack;
- есть CI;
- есть frontend-скелет;
- но нет полноценного завершённого stat engine и нет завершённой продуктовой интеграции.

### 3.2. Практическая стадия

Если назвать стадию честно, то это:

**«Доменный backend foundation + частичная exact-аналитика + неготовая миграция legacy stat-layer»**.

То есть:
- это уже **не пустой прототип**;
- но это ещё **не beta stat-engine**;
- и точно **не production-ready tracker**.

Моё резюме по стадии:

- parser/normalizer foundation: **ранняя альфа**;
- tournament economics foundation: **ранняя альфа с хорошим направлением**;
- stat engine для MBR Stats: **пред-альфа**;
- UI/frontend: **демо-заготовка / scaffolding**.

---

## 4. Что уже реализовано и что в этом корректно

## 4.1. Exact-core foundation — это сильная часть репозитория

В репозитории уже есть достаточно продуманный базовый слой:
- разбор hand history;
- разбор tournament summary;
- normalizer;
- derived-таблицы;
- materialization seed-features;
- runtime-запросы стартовых stat-метрик.

Это важный плюс: проект строится не поверх хаотичного SQL-слоя, а через явную промежуточную доменную модель.

### Что здесь хорошо

1. **Есть нормальное разделение ответственности**.  
   Parser не пытается сразу считать все статы. Сначала строится нормализованная модель раздачи, потом derived-слой, потом stat runtime.

2. **Есть осторожность в неоднозначных ситуациях**.  
   Там, где маппинг winners/pots неоднозначен, код скорее оставляет exact-слой пустым/uncertain, чем генерирует фальшивую точность.

3. **Есть миграции exact-core schema v2**.  
   Видно движение к более жёсткой схеме: player aliases, source file members, job attempts, каталоги аналитики, composite FK, dedupe по source files.

4. **Есть тестовое покрытие на фундаментальном слое**.  
   Это не гарантия корректности статов, но это хороший сигнал по инженерной дисциплине.

Итог: exact-core слой — **одна из лучших частей проекта**.

---

## 4.2. Tournament economics — неожиданно сильнее, чем можно подумать по верхнему описанию

Сейчас в проекте уже есть логика, которая:
- получает из tournament summary buy-in/rake/bounty/entrants/finish/payout;
- сопоставляет finish place и entrants с reference payout tables;
- выделяет **regular prize**;
- считает **mystery_money = total_payout - regular_prize**;
- сохраняет regular prize, total payout и mystery money в tournament entries.

### Почему это важно

Это значит, что проект уже умеет корректно получить:
- обычный ITM-доход;
- суммарный mystery/KO-доход на уровне турнира.

Даже если Big KO-decomposition по отдельным envelope ещё не готов, **турнирный итог денег уже можно считать достаточно надёжно**, если reference tables совпадают с реальными правилами формата.

### Моя оценка

Здесь база сделана **хорошо**. Это одна из опорных частей для дальнейшей миграции stats.

---

## 4.3. Elimination / side-pot attribution — здесь есть реально правильная идея

Очень важный момент.

В коде normalizer-а логика привязки вылета к pot-у строится через выбор **последнего pot-а, в который busted player ещё внёс фишки**. Для side-pot KO это очень сильная идея, потому что для Mystery Battle Royale правило награды привязано именно к winners того pot/side-pot, которым был фактически закрыт busted player.

### Почему это хорошо

Это не наивное «кто собрал главный pot, тому и KO».  
Это попытка вычислить именно тот pot, который релевантен устранению игрока. И концептуально это очень близко к реальной механике payout attribution.

### Оценка

Это **хороший фундамент**, который точно не надо выбрасывать. Его надо сохранить и доработать, а не переписывать с нуля.

---

## 4.4. Exact FT hand detection по 9-max header — для текущего GG-формата это нормальная основа

Сейчас exact FT hand определяется по header.max_players == 9.

Для текущего целевого формата это выглядит разумно как практический exact-сигнал для факта, что раздача уже игралась за финальным столом.

### Что важно

Это **не решает boundary case** между финалом Rush Stage и первым реальным FT hand.  
Но как признак того, что конкретная рука уже FT-hand — это хорошая опора.

### Вывод

Этот кусок логики стоит **оставить**. Проблема не здесь. Проблема в том, что вокруг этого сигнала пока плохо построена переходная зона.

---

## 5. Что реально реализовано из MBR Stats, а что нет

## 5.1. Главный вывод

В `docs/stat_catalog/mbr_stats_inventory.yml` перечислен legacy inventory, но фактическая реализация runtime пока намного уже.

По сути сейчас реализовано:
- **9 hand-level features**;
- **5 seed stat metrics**.

Это не «реализован набор MBR Stats», а только стартовая инфраструктура.

## 5.2. Что есть как hand features

Текущий набор seed/features примерно такой:
- `played_ft_hand`
- `has_exact_ko`
- `has_split_ko`
- `has_sidepot_ko`
- `ft_table_size`
- `hero_exact_ko_count`
- `hero_split_ko_count`
- `hero_sidepot_ko_count`
- `ft_stage_bucket`

### Оценка

Это нормальный стартовый feature-layer, но:
- он слишком узкий для полной миграции MBR Stats;
- часть признаков названа так, будто они готовы к прямому использованию в stats, хотя на деле там ещё скрыты важные ограничения;
- stage-бакетизация слишком грубая для legacy stage-модулей.

## 5.3. Какие seed-stats реально есть сейчас

Сейчас сидируются примерно такие метрики:
- `roi_pct`
- `avg_finish_place`
- `final_table_reach_percent`
- `total_ko`
- `avg_ko_per_tournament`

### Важнейшее замечание

Эти метрики **не надо называть полноценной миграцией MBR Stats**. Это безопасные стартовые прокси.

---

## 6. Супер тщательный аудит алгоритмов расчёта статов

## 6.1. ROI — формула сама по себе корректна, но надо жёстко определить coverage contract

Текущая идея:

`ROI = (total_payout - total_buyin) / total_buyin * 100`

### Формально

Как базовая формула ROI — это корректно.

### Где проблема

Проблема не в арифметике, а в **контракте покрытия**:
- если stat считается по summary-covered tournaments — это одно;
- если по hand-history-covered tournaments — это другое;
- если UI не объясняет это явно, пользователь решит, что это «полный реальный ROI по базе».

### Вердикт

- формула: **корректна**;
- риск: **возможен misleading из-за смешения coverage semantics**.

### Что нужно сделать

Для каждого stat-а фиксировать:
- denominator universe;
- source of truth;
- coverage label;
- exactness class.

---

## 6.2. Average Finish Place — формула корректна

Текущая идея:

`avg_finish_place = sum(finish_place) / count(tournaments)`

### Вердикт

Если брать только турниры с валидным finish place, формула корректна.

### Что не хватает

Нужно только жёстко определить:
- count по каким именно турнирам идёт;
- как обрабатываются пропуски;
- попадают ли туда partial imports.

---

## 6.3. Final Table Reach % — формула условно корректна, но только как HH-covered exact/proxy metric

Текущая идея опирается на факт FT reach через exact hand coverage.

### Что хорошо

Если FT reach выводится из реально импортированных рук и логики exact FT hand, это лучше, чем гадать по косвенным данным.

### Где риск

Если denominator — не весь universe всех турниров из summary, а только HH-covered subset, то stat должен так и называться.

### Вердикт

- как `HH-covered exact FT reach %`: **нормально**;
- как «финальная migrated MBR FT metric без оговорок»: **пока нет**.

---

## 6.4. Total KO и Avg KO per Tournament — сейчас это event count, а не полноценная money-aware KO-модель

Это очень важный участок.

Сейчас `hero_exact_ko_count` по сути считает **события KO**, а не экономическую ценность KO и не нормализованную долю bounty в split-case.

### Что здесь корректно

Если трактовать показатель как **число exact KO events**, то это валидный stat.

### Что здесь некорректно, если назвать это просто `total_ko`

Пользователь может ожидать одно из трёх:
1. число KO events;
2. число KO с учётом split semantics;
3. KO contribution в деньгах.

Сейчас реализовано только (1), и то в ограниченном exact-варианте.

### Вердикт

- как `total_ko_event_count_exact`: **да**;
- как полноценный legacy `total_ko`: **нет, ещё не готово**.

---

## 6.5. FT stage bucket — слишком грубый и опасный как основной слой для stage-based stats

Текущая bucketization примерно такая:
- not_ft
- ft_7_9
- ft_5_6
- ft_3_4
- ft_2_3

### Почему этого недостаточно

Legacy MBR-модули обычно хотят более точные stage-срезы. В частности, нужны диапазоны, которые нельзя безопасно выразить только через текущий bucket.

Например:
- stage 4–5;
- stage 6–9;
- boundary/pre-FT semantics;
- early FT vs deep FT.

### Вывод

`ft_stage_bucket` можно оставить как вспомогательный UI/feature слой, но **строить на нём весь stage-stat engine нельзя**.

Нужен другой фундамент:
- exact `players_remaining` / `ft_table_size`;
- отдельный tournament-level FT helper layer;
- явная boundary classification.

---

## 7. Самая проблемная часть: stage / boundary logic

Это главный P0-блокер.

## 7.1. Что сейчас делает код

Текущая логика примерно такая:
- сортирует руки по raw timestamp;
- находит первую FT руку как первую руку с `max_players == 9`;
- затем выбирает boundary hand как **последнюю предыдущую руку с `max_players == 5`**;
- именно эту руку помечает как вход в boundary zone / candidate last rush hand.

## 7.2. Почему это серьёзно неправильно

Это жёстко вшитая эвристика под один частный паттерн, а не корректная модель перехода формата.

### Проблема №1: boundary hand не обязан быть 5-max

При переходе Rush Stage → FT последняя релевантная раздача перед формированием FT может быть не только 5-max. Реально она может быть 2-max / 3-max / 4-max / 5-max / и т.д. в зависимости от структуры стола/шут-аута/конфигурации последних рук.

Следовательно правило «последняя 5-max перед первой 9-max» — это **не доменное правило**, а просто временный костыль.

### Проблема №2: код изображает уверенность там, где её нет

Схема содержит `boundary_ko_min / ev / max`, но текущая реализация записывает туда по сути **одно и то же точечное значение**.

Это означает: формально схема заявляет диапазон неопределённости, но фактически рантайм его не использует.

### Проблема №3: тесты закрепляют неправильную эвристику

Если в тестах зафиксирована логика вида «выбираем last five-max before first final table», то тесты сейчас защищают не корректность, а костыль.

## 7.3. Вердикт

Stage/boundary часть в текущем виде **нельзя считать корректной основой** для:
- pre-FT KO stats;
- stage 6–9 / 4–5 / boundary metrics;
- FT conversion stats;
- KO luck / ROI adjusted / Big KO слоя.

Это **первый обязательный P0-ремонт**.

---

## 8. Big KO / KO-money модель — в текущем виде не production-ready

Это второй главный P0-блокер.

## 8.1. Что сейчас уже хорошо

Есть попытка построить decoder, который:
- принимает total mystery money;
- принимает список hero KO shares;
- перебирает возможные envelope allocations;
- умеет выдавать exact / ambiguous / infeasible.

То есть архитектурно это уже **не игрушка**, а реальный зародыш solver-а.

## 8.2. Почему этого недостаточно

### Проблема №1: используется неустойчивый `hero_share_fraction`

Сейчас доля героя выводится из chip-share внутри релевантного pot-а. Это может быть разумно как **provenance indicator**, но это не гарантированно корректная money-share модель для реального bounty payout.

Особенно опасны split pots и odd-chip распределения.

### Проблема №2: cent-divisibility ломает реалистичные случаи

Если decoder требует, чтобы `envelope * share` давал идеальные integer cents, то реальные split-сценарии могут ложно считаться infeasible.

Особенно это критично для split 1/3, 1/5 и любых значений envelope, не делящихся «красиво».

### Проблема №3: frequencies пока не используются как вероятностная модель

Даже если есть таблица envelope payout + frequency, decoder сейчас опирается в основном на **feasibility**, а не на posterior ranking.

То есть он отвечает на вопрос «что вообще возможно?», но не на вопрос «что наиболее вероятно?».

### Проблема №4: ограничение количества решений само по себе не страшно, но сейчас нет хорошего explanation layer

Если решений много, нужно не просто отрезать после 64, а уметь:
- ранжировать;
- агрегировать posterior mass;
- объяснять пользователю confidence.

## 8.3. Вердикт

Текущий Big KO decoder — это **хороший исследовательский прототип**, но **не корректный финальный двигатель stats**.

Его нельзя использовать как базу для production-версий:
- `big_ko`
- `ko_luck`
- `roi_adj`
- любых adjusted KO contribution metrics

без полной переработки модели.

---

## 9. Normalizer / elimination layer — что там хорошо, а что надо исправить

## 9.1. Что хорошо

1. Есть replay-based reconstruction.  
2. Есть pot-building по уровням вкладов.  
3. Есть сопоставление winners через collect events.  
4. Есть безопасное поведение при ambiguity.  
5. Есть сильная база для side-pot elimination attribution.

Это реально качественная часть для ранней фазы.

## 9.2. Что пока слабее

1. Модель winner allocation может оставлять больше uncertain-кейсов, чем хотелось бы, на более широком корпусе.  
2. `hero_share_fraction` нельзя напрямую превращать в денежную долю bounty.  
3. Нет завершённой спецификации split-bounty rounding.  
4. Часть бизнес-логики домена ещё живёт как эвристический bridge, а не как formalized ruleset.

## 9.3. Вывод

Normalizer не надо ломать. Его надо:
- сохранить;
- отделить event semantics от money semantics;
- усилить спецификацией split KO;
- использовать как ядро для более строгого stat engine.

---

## 10. Дополнительные проблемы и недочёты

## 10.1. `ft_table_size = 0` на non-applicable руках — плохой sentinel

Если non-FT hand получает `ft_table_size = 0`, это может тихо отравить агрегации, если кто-то забудет явно фильтровать applicability.

### Правильнее

Либо:
- хранить `NULL` / отсутствующее значение;
- либо иметь отдельный applicability flag и никогда не отдавать 0 как «как будто нормальное значение измерения».

## 10.2. Названия признаков вводят в заблуждение

Пример: `hero_exact_ko_count` лучше переименовать в нечто вроде `hero_exact_ko_event_count`.

Иначе дальше кто-то неизбежно начнёт использовать его как будто это уже готовая canonical KO metric.

## 10.3. Парсер покрывает не все заявленные enum-семантики

Если в моделях есть action variants вроде `Muck` или `PostDead`, но parser реально их не эмитит, это признак рассинхрона между декларацией модели и фактическим покрытием парсера.

Это не P0, но это надо вычистить.

## 10.4. Summary seat-result lines не структурированы как отдельный доменный слой

Это ограничивает возможности более глубоких cross-check и пост-аналитики.

## 10.5. Нет canonical UTC timestamps

Для текущего узкого GG-скоупа это не критично, но нужно честно понимать: полноценной time-model тут пока нет. Есть raw/local ordering contract, этого достаточно для локального импорта, но не более.

## 10.6. Materialization пока full-refresh

Это естественно для фундамента, но при росте корпуса станет узким местом.

---

## 11. Что НЕ надо переписывать сейчас

Это важно, чтобы код-агент не начал разрушать сильные части.

### Сохранять

1. **Exact FT hand по 9-max header** как один из сигналов FT-hand.  
2. **Логику выбора последнего релевантного side-pot-а** через max contributed pot busted player-а.  
3. **Политику ambiguity-safe non-materialization** вместо выдумывания exact winners.  
4. **Tournament-level economics split**: regular prize vs mystery money.  
5. **Общую layered architecture**: parser → normalizer → derived → stats runtime.

### Не делать сейчас

- не переписывать весь parser с нуля;
- не выбрасывать existing normalizer;
- не пытаться сразу закрыть все 31 stats одним большим коммитом;
- не строить stats поверх грубого `ft_stage_bucket` без формальной спецификации.

---

## 12. Реальный статус по MBR-модулям: что можно сделать уже сейчас, а что блокировано

Ниже — практическая разбивка.

## 12.1. Можно реализовать относительно быстро после формализации спецификации

Эти статы уже близки к реализуемым на текущем exact/tournament foundation:
- ROI
- Avg Finish Place
- ITM %
- Winnings from ITM
- Winnings from KO (как tournament-level mystery total)
- Final Table Reach %
- Avg Finish Place FT
- Avg Finish Place No FT
- ROI on FT
- Total KO Event Count
- Avg KO Event per Tournament
- Incomplete FT %
- Возможно: Avg FT Initial Stack (если формально закрепить first FT hand selection)

### Важное условие

Они должны выходить в систему **не как «legacy fully migrated stats»**, а как stat keys с явным exactness/coverage contract.

## 12.2. Блокированы boundary/stage проблемой

Эти модули нельзя честно выпускать до ремонта boundary logic:
- pre_ft_ko
- pre_ft_chipev
- early_ft_bust
- early_ft_ko
- ko_stage_4_5
- ko_stage_6_9
- любые FT conversion / boundary-dependent stats

## 12.3. Блокированы KO-money / Big KO model

Эти модули нельзя корректно выпускать до переработки bounty-split модели:
- big_ko
- ko_luck
- roi_adj
- adjusted KO contribution
- любые envelope/posterior-aware показатели

## 12.4. Блокированы отсутствием formal formula spec

Даже если данные уже есть, выпуск нельзя делать, пока не определены:
- точные формулы;
- denominator contracts;
- treatment of partial coverage;
- exact vs estimated status;
- mapping legacy-module → new stat keys.

Это касается **всех** модулей без исключения.

---

## 13. Список найденных проблем

Ниже — проблемы в порядке практической критичности.

### P0-проблемы

#### P0-1. Нет formal stat spec для всех legacy MBR modules

Сейчас inventory описывает набор, но не задаёт полноформатную, машинно-однозначную спецификацию для реализации.

**Почему это критично:** без этого код-агент либо начнёт гадать формулы, либо реализует несовместимые версии одного и того же stat-а.

---

#### P0-2. Boundary logic построена на жёсткой эвристике `last 5-max before first 9-max`

**Почему это критично:** ломает stage-aware stats и любой честный анализ переходной зоны.

---

#### P0-3. `boundary_ko_min / ev / max` по факту не выражают реальную неопределённость

**Почему это критично:** схема обещает диапазон, а runtime выдаёт фальшивую точечную уверенность.

---

#### P0-4. KO event semantics смешаны с KO money semantics

`hero_exact_ko_count` и `hero_share_fraction` пока не разведены в безопасную модель.

**Почему это критично:** неизбежно приведёт к неправильным money-based stat-ам.

---

#### P0-5. Big KO decoder не готов для production-статов

Причины:
- нет корректной split-money модели;
- нет posterior ranking;
- есть проблемы с cent rounding/divisibility;
- нельзя честно строить adjusted KO stats.

---

#### P0-6. Текущие 5 seed stats можно ошибочно принять за реализованный MBR pack

**Почему это критично:** это создаст ложное ощущение завершённости и приведёт к неправильным продуктовым решениям.

---

### P1-проблемы

#### P1-1. `ft_stage_bucket` слишком груб для legacy stage-модулей

---

#### P1-2. `ft_table_size = 0` как sentinel для non-applicable rows

---

#### P1-3. Названия признаков не отражают ограниченную семантику (`*_ko_count` vs `*_ko_event_count`)

---

#### P1-4. Parser/model mismatch по action variants

---

#### P1-5. Summary seat-result lines не превращаются в структурированный слой

---

#### P1-6. Нет завершённого tournament-level FT helper layer

---

### P2-проблемы

#### P2-1. Full-refresh materialization

---

#### P2-2. Нет завершённой backend→API→frontend интеграции stat engine

---

#### P2-3. Нет зрелой explanation/debug surface для ambiguous KO/Big KO cases

---

## 14. Полный план исправления и продолжения разработки

Ниже — план именно в том виде, в котором его можно отдавать код-агенту.

Принцип порядка такой:
1. сначала фиксируем semantics и contracts;
2. потом ремонтируем boundary;
3. потом разводим event-модель и money-модель KO;
4. потом строим Big KO;
5. потом быстро выпускаем tranche тех статов, которые уже можно сделать честно;
6. потом расширяем stage-aware слой;
7. потом уже product/perf.

---

# ФАЗА 0 — Зафиксировать спецификацию

## Задача F0-T1 — Ввести formal stat spec для всего набора MBR Stats

**Приоритет:** P0  
**Цель:** прекратить неоднозначность формул и coverage semantics.

### Что сделать

Создать новый файл, например:
- `docs/stat_catalog/mbr_stats_spec_v1.yml`

Для **каждого** legacy module зафиксировать:
- `legacy_module_id`
- `new_stat_key` или список `new_stat_keys`
- `title`
- `formal_formula`
- `numerator_definition`
- `denominator_definition`
- `units`
- `grain` (tournament / hand / ko_event / ft_session и т.д.)
- `exactness_class` (`exact`, `coverage_limited_exact`, `estimated`, `blocked`)
- `source_dependencies`
- `nullability rules`
- `partial coverage behavior`
- `fixture expectations`
- `acceptance query examples`

### Что НЕ допускать

- prose-only descriptions без формул;
- скрытые denominator assumptions;
- смешение event count и money semantics;
- неявное соответствие legacy → new.

### Чек-лист приёмки

- [ ] Для всех legacy modules есть формальное описание.  
- [ ] Не осталось модулей со статусом «на словах понятно, в коде разберёмся».  
- [ ] Для каждого stat-а зафиксирован coverage contract.  
- [ ] Для каждого stat-а зафиксирован exactness class.  
- [ ] Для каждого stat-а определён единый canonical output key.  
- [ ] Документ пригоден как единственный source of truth для код-агента.

---

## Задача F0-T2 — Ввести единый glossary для KO/event/bounty semantics

**Приоритет:** P0

### Что сделать

Создать документ, например:
- `docs/architecture/ko_semantics_glossary.md`

Зафиксировать термины:
- `ko_event`
- `exact_ko_event`
- `split_ko_event`
- `sidepot_ko_event`
- `boundary_ko`
- `mystery_money_total`
- `regular_prize_money`
- `ko_money_realized`
- `ko_money_estimated`
- `posterior_big_ko`
- `event_count`
- `share_fraction_provenance`
- `money_share`

### Чек-лист приёмки

- [ ] Все неоднозначные термины определены.  
- [ ] В документации нет мест, где один и тот же термин используется в двух значениях.  
- [ ] Код-агент сможет по словарю понять, где event, а где money.

---

# ФАЗА 1 — Починить stage/boundary core

## Задача F1-T1 — Убрать эвристику `last 5-max before first 9-max`

**Приоритет:** P0  
**Цель:** перестать выдавать заведомо слабую доменную догадку как будто это нормальная модель.

### Что сделать

Переписать boundary resolver так, чтобы он:
- не был привязан к 5-max;
- использовал последовательность рук турнира как ordered timeline;
- искал последнюю non-FT candidate hand(s) перед первым exact FT hand;
- умел маркировать **несколько допустимых boundary candidates**, если exact boundary не доказывается однозначно;
- умел сохранять uncertainty state честно.

### Минимально допустимая новая модель

Вместо одного жёсткого `boundary_hand_id` должны появиться как минимум:
- `boundary_resolution_state`
- `boundary_candidate_count`
- `boundary_hand_id_exact` (nullable)
- `boundary_hand_id_min` / `max` или эквивалентная модель диапазона
- `resolution_method`
- `confidence_class`

### Чек-лист приёмки

- [ ] В коде больше нет правила «ищем последнюю 5-max руку».  
- [ ] Добавлены synthetic tests для 2-max / 3-max / 4-max / 5-max boundary cases.  
- [ ] Если boundary не доказывается, runtime не выдаёт fake exact.  
- [ ] Старые тесты, закреплявшие 5-max-костыль, удалены или переписаны.  
- [ ] Документирована новая resolution policy.

---

## Задача F1-T2 — Построить tournament-level FT helper layer

**Приоритет:** P0

### Что сделать

Добавить derived/helper слой с полями вида:
- `reached_ft_exact`
- `first_ft_hand_id`
- `first_ft_hand_started_local`
- `first_ft_table_size`
- `ft_started_incomplete`
- `deepest_ft_size_reached`
- `hero_ft_entry_stack_chips`
- `hero_ft_entry_stack_bb`
- `entered_boundary_zone`
- `boundary_resolution_state`

### Зачем

Чтобы stats считались не напрямую из хаотичных hand features, а из стабилизированного tournament helper слоя.

### Чек-лист приёмки

- [ ] Для каждого турнира есть единый FT helper record.  
- [ ] first FT hand выбирается детерминированно.  
- [ ] incomplete FT определяется формально, а не на глаз.  
- [ ] helper layer покрыт тестами на committed corpus и synthetic edge cases.

---

## Задача F1-T3 — Сделать stage predicates формальными, а не bucket-only

**Приоритет:** P0

### Что сделать

Вместо reliance на `ft_stage_bucket` ввести формальные predicates:
- `is_ft_hand`
- `ft_players_remaining_exact`
- `is_stage_2`
- `is_stage_3_4`
- `is_stage_4_5`
- `is_stage_5_6`
- `is_stage_6_9`
- `is_boundary_hand`
- `is_pre_ft_boundary_window`

### Чек-лист приёмки

- [ ] Любой legacy stage-module можно выразить через формальные predicates.  
- [ ] Для stage 4–5 и 6–9 не нужны хаки поверх грубого bucket-а.  
- [ ] Все predicates протестированы на synthetic fixtures.

---

# ФАЗА 2 — Разделить KO event layer и KO money layer

## Задача F2-T1 — Зафиксировать отдельные сущности для event count и money share

**Приоритет:** P0

### Что сделать

В схеме/derived/runtime развести:
- `ko_event_count`
- `ko_event_exact_count`
- `ko_event_split_count`
- `ko_event_sidepot_count`
- `ko_winner_set_size`
- `ko_pot_resolution_type`
- `money_share_model_state`
- `money_share_exact_fraction` (если когда-либо доказуемо exact)
- `money_share_estimated_min/max/ev`

### Важное правило

`hero_share_fraction`, вычисленная из chip-share pot-а, **не должна** автоматически считаться финальной money share для bounty.

### Чек-лист приёмки

- [ ] Event-модель и money-модель разведены по данным и именам.  
- [ ] Ни один money-stat больше не использует pot chip-share напрямую без явного adapter layer.  
- [ ] Существующие `*_ko_count` переименованы в `*_ko_event_count`, если это именно event semantics.

---

## Задача F2-T2 — Формализовать rule-set для split bounty rounding

**Приоритет:** P0

### Что сделать

Сделать отдельный документ/модуль с политикой:
- как делится bounty при нескольких winners;
- как учитывать odd chips / неравные pot shares;
- как округляются центы;
- допускается ли точная money-proportional split или платформа применяет другое правило;
- что делать, если exact payout rule не восстанавливается из HH/TS.

### Если exact platform rule недоказуем

Нужно ввести явную estimated-модель с posterior/interval, а не подменять её фиктивным exact.

### Чек-лист приёмки

- [ ] Есть письменная спецификация split-bounty money semantics.  
- [ ] Есть synthetic tests для 2-way и 3-way split.  
- [ ] Есть кейсы с «некрасивым» делением по центам.  
- [ ] Ни один валидный кейс не становится infeasible только из-за искусственного требования целых центов без оговорённой rounding policy.

---

# ФАЗА 3 — Перестроить Big KO decoder

## Задача F3-T1 — Сделать Big KO decoder вероятностным, а не только feasibility-checker

**Приоритет:** P0

### Что сделать

Переделать decoder так, чтобы он:
- учитывал `frequency_per_100m` как вероятность/вес;
- выдавал ranked solutions;
- умел агрегировать posterior mass;
- умел возвращать `exact`, `high_confidence`, `ambiguous`, `infeasible`;
- поддерживал rounding policy из F2-T2.

### Минимальный ожидаемый интерфейс

На выходе должно быть что-то вроде:
- `decode_state`
- `top_solutions[]`
- `top_solution_posterior`
- `posterior_entropy` или аналогичная мера ambiguity
- `expected_big_ko_cents`
- `min_big_ko_cents`
- `max_big_ko_cents`

### Чек-лист приёмки

- [ ] Exact-case определяется корректно.  
- [ ] Ambiguous-case не просто перечисляется, а ранжируется.  
- [ ] Веса envelope distribution реально участвуют в вычислении.  
- [ ] Есть тесты для 1-way, 2-way, 3-way, odd-cent split cases.  
- [ ] Есть explain/debug output для анализа решений.

---

## Задача F3-T2 — Отделить tournament-level mystery money от decoded envelope-level big KO

**Приоритет:** P0

### Что сделать

В stat runtime и каталоге stat-ов развести два разных семейства:
- то, что известно точно на уровне турнира (`mystery_money_total`);
- то, что является inferred/posterior на уровне envelope allocation.

### Чек-лист приёмки

- [ ] В API и runtime невозможно перепутать exact tournament mystery total и inferred Big KO decomposition.  
- [ ] Каждый stat помечен как exact или inferred/posterior-based.

---

# ФАЗА 4 — Быстро выпустить честный первый tranche статов

## Задача F4-T1 — Реализовать tranche «можно сделать уже сейчас»

**Приоритет:** P0

### Список stat-ов для первой волны

Реализовать после F0 + F1 helper layer:
- ROI
- Avg Finish Place
- ITM %
- Winnings from ITM
- Winnings from KO (tournament-level mystery total)
- Final Table Reach %
- Avg Finish Place FT
- Avg Finish Place No FT
- ROI on FT
- Total KO Event Count
- Avg KO Event per Tournament
- Incomplete FT %
- Avg FT Initial Stack Chips
- Avg FT Initial Stack BB

### Принципы реализации

- никаких скрытых denominator assumptions;
- каждый stat помечен exactness/coverage metadata;
- все названия отражают семантику;
- если stat coverage-limited — это видно в каталоге и ответе API.

### Чек-лист приёмки

- [ ] Все перечисленные статы доступны через единый stat registry.  
- [ ] Для каждого stat-а есть regression tests.  
- [ ] Для каждого stat-а есть doc entry в formal spec.  
- [ ] В outputs нет misleading названий.  
- [ ] API/SQL/runtime возвращают exactness/coverage alongside value.

---

## Задача F4-T2 — Добавить stat metadata surface

**Приоритет:** P0

### Что сделать

Для любого stat output возвращать не только `value`, но и:
- `stat_key`
- `value`
- `units`
- `exactness_class`
- `coverage_class`
- `sample_size`
- `denominator_count`
- `blocked_reason` (если stat unavailable)

### Чек-лист приёмки

- [ ] Любой stat можно интерпретировать без чтения кода.  
- [ ] Невозможно silently перепутать exact stat и coverage-limited proxy.  
- [ ] UI сможет честно показывать состояние метрики.

---

# ФАЗА 5 — Stage-aware KO и FT-статы

## Задача F5-T1 — Реализовать stage-aware KO tranche

**Приоритет:** P1

### Реализовать

После ремонта boundary и predicate layer:
- `ko_stage_2_3`
- `ko_stage_3_4`
- `ko_stage_4_5`
- `ko_stage_5_6`
- `ko_stage_6_9`
- `pre_ft_ko`
- `early_ft_ko`
- `early_ft_bust`

### Чек-лист приёмки

- [ ] Ни один stage-stat не опирается только на грубый bucket.  
- [ ] Boundary cases покрыты synthetic tests.  
- [ ] Если stage exact не доказывается, stat помечается blocked/estimated по спецификации.

---

## Задача F5-T2 — Реализовать FT conversion layer

**Приоритет:** P1

### Что сделать

Формально определить conversion metrics:
- FT reached → top 6
- FT reached → top 4
- FT reached → top 3
- FT reached → win
- boundary/entry stack conversion metrics

### Чек-лист приёмки

- [ ] Каждая conversion metric имеет formal formula.  
- [ ] Есть tests на incomplete FT.  
- [ ] Есть корректная обработка турниров без FT coverage.

---

# ФАЗА 6 — Попытки KO / attack-layer / richer stat substrate

## Задача F6-T1 — Специфицировать и реализовать KO attempt model

**Приоритет:** P1

### Что сделать

Сначала в спецификации определить, что считается `KO attempt`:
- all-in against shorter stack?
- all-in with elimination risk?
- call/jam into player who can bust?
- multiway contest where hero covers busted player?

После фиксации формулы строить attempt-layer.

### Чек-лист приёмки

- [ ] Формула KO attempt зафиксирована до кодинга.  
- [ ] Есть derived table для attempts.  
- [ ] Есть synthetic fixtures успеха/неуспеха/мультивея.  
- [ ] Любые derived attempt-stats воспроизводимы и объяснимы.

---

# ФАЗА 7 — Очистка модели и парсеров

## Задача F7-T1 — Устранить parser/model drift

**Приоритет:** P1

### Что сделать

Либо реально парсить, либо удалить из модели то, что не эмитится. В частности проверить:
- `Muck`
- `PostDead`
- все другие enum variants

### Чек-лист приёмки

- [ ] Для каждого enum variant есть либо parser support, либо явное удаление.  
- [ ] Нет «мертвых» доменных вариантов.  
- [ ] Добавлены fixture tests на недостающие action types.

---

## Задача F7-T2 — Структурировать summary seat-result layer

**Приоритет:** P1

### Что сделать

Парсить seat-result lines tournament summary в отдельную структуру, пригодную для cross-check и будущих stats.

### Чек-лист приёмки

- [ ] Seat-result строки доступны в structured form.  
- [ ] Можно cross-check-ить summary ranking против tournament result layer.  
- [ ] Добавлены tests на реальные summary-примеры.

---

## Задача F7-T3 — Убрать `ft_table_size = 0` как скрытый sentinel

**Приоритет:** P1

### Что сделать

Заменить на:
- `NULL`, либо
- отдельную applicability-модель.

### Чек-лист приёмки

- [ ] Не-applicable rows не выглядят как валидное численное измерение.  
- [ ] Агрегаты по FT size больше нельзя случайно испортить нулями.  
- [ ] Все downstream-запросы обновлены.

---

## Задача F7-T4 — Нормализовать naming статов и фич

**Приоритет:** P1

### Что сделать

Примеры:
- `hero_exact_ko_count` → `hero_exact_ko_event_count`
- `total_ko` → `total_ko_event_count` или другой честный canonical key

### Чек-лист приёмки

- [ ] Имена отражают реальную семантику.  
- [ ] В коде нет misleading alias-ов.  
- [ ] Legacy mapping описан отдельно и явно.

---

# ФАЗА 8 — Производительность и продуктовая интеграция

## Задача F8-T1 — Перейти от full-refresh к incremental materialization

**Приоритет:** P2

### Чек-лист приёмки

- [ ] Новые imports обновляют только затронутые сущности.  
- [ ] Есть backfill mode и incremental mode.  
- [ ] Regression tests гарантируют одинаковый результат full vs incremental.

---

## Задача F8-T2 — Построить API surface и UI contract для stat outputs

**Приоритет:** P2

### Чек-лист приёмки

- [ ] UI получает вместе со stat value метаданные exactness/coverage.  
- [ ] Есть способы показать blocked/inferred/ambiguous состояния.  
- [ ] Пользователь не видит misleading «точных» чисел там, где модель probabilistic.

---

## 15. Приоритеты в одной таблице

| ID | Задача | Приоритет |
|---|---|---|
| F0-T1 | Formal spec для всех MBR Stats | P0 |
| F0-T2 | Glossary для KO/event/bounty semantics | P0 |
| F1-T1 | Удалить эвристику last-5-max и переписать boundary resolver | P0 |
| F1-T2 | Построить tournament-level FT helper layer | P0 |
| F1-T3 | Ввести формальные stage predicates | P0 |
| F2-T1 | Развести KO event layer и money layer | P0 |
| F2-T2 | Формализовать split-bounty rounding rules | P0 |
| F3-T1 | Переделать Big KO decoder в вероятностный | P0 |
| F3-T2 | Развести exact mystery total и inferred Big KO | P0 |
| F4-T1 | Выпустить честный first tranche статов | P0 |
| F4-T2 | Добавить stat metadata surface | P0 |
| F5-T1 | Реализовать stage-aware KO tranche | P1 |
| F5-T2 | Реализовать FT conversion metrics | P1 |
| F6-T1 | Специфицировать и реализовать KO attempt model | P1 |
| F7-T1 | Устранить parser/model drift | P1 |
| F7-T2 | Структурировать summary seat-result layer | P1 |
| F7-T3 | Убрать ft_table_size=0 sentinel | P1 |
| F7-T4 | Нормализовать naming stat-ов и фич | P1 |
| F8-T1 | Перейти на incremental materialization | P2 |
| F8-T2 | Построить API/UI contract для stat outputs | P2 |

---

## 16. Рекомендуемый порядок работы код-агента

Если отдавать это код-агенту, порядок должен быть именно такой:

1. **F0-T1 + F0-T2**  
   Сначала заморозить спецификацию. Без этого дальше кодить опасно.

2. **F1-T1 + F1-T2 + F1-T3**  
   Потом починить boundary и FT helper layer. Это главный structural blocker.

3. **F2-T1 + F2-T2**  
   Затем развести KO events и KO money.

4. **F3-T1 + F3-T2**  
   Потом перестроить Big KO.

5. **F4-T1 + F4-T2**  
   После этого быстро выкатить первую честную волну статов.

6. **F5 / F6 / F7**  
   Затем stage-aware stats, attempts, cleanup.

7. **F8**  
   Только после этого идти в perf/UI integration.

---

## 17. Что я бы считал критериями “проект реально вышел из foundation phase”

Проект можно будет считать вышедшим из текущей стадии, когда одновременно выполнены условия:

- есть formal spec на весь MBR pack;
- boundary logic больше не костыльная;
- Big KO layer имеет корректную probabilistic модель;
- минимум 10–15 ключевых stat-ов реализованы честно и протестированы;
- каждый stat выдаётся с exactness/coverage metadata;
- API/UI не маскируют estimated/inferred величины под точные.

До этого момента я бы называл проект именно **доменным фундаментом, а не готовой stat-системой**.

---

## 18. Короткое резюме

### Что уже хорошо

- архитектура ядра;
- normalizer foundation;
- side-pot elimination idea;
- tournament economics;
- тестовая дисциплина и migration hygiene.

### Что самое слабое

- boundary logic;
- отсутствие formal stat spec;
- смешение KO event и KO money semantics;
- Big KO decoder;
- слишком ранний stage статов относительно заявленного inventory.

### Главная рекомендация

Не пытаться «дописать ещё несколько stat-ов поверх текущей модели».  
Сначала нужно зафиксировать спецификацию и починить boundary/KO-money foundation. Это даст намного лучший результат, чем серия локальных патчей.

---

## 19. Источники, которые использовались при аудите

### Репозиторий
- `https://github.com/disikk/Check_Mate`
- README, docs, migrations, seeds, backend crates, parser/runtime code.

### Внешняя проверка доменных правил и economics
- Публичная страница GGPoker по Mystery Battle Royale / prize structure / rush-stage-final-table transition.

---

