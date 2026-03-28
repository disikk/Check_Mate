# Аудит репозитория `disikk/Check_Mate`

Дата: 2026-03-27

## 1. Короткий вывод

### 1.1. На какой стадии проект сейчас

Моя оценка:

- **Как продукт целиком**: примерно **30–35%**. Это всё ещё **backend-first foundation / narrow alpha**, а не beta и тем более не production.
- **Как узкое backend-ядро под GG Mystery Battle Royale**: примерно **60–70%**.
- **Как exact postflop-модуль `street_strength`**:
  - **best-hand evaluator** — выглядит **очень сильным и близким к надёжному ядру**;
  - **draw / nut / missed-draw слой** — в текущем контракте выглядит **в целом корректным**, но ещё **недостаточно доказан независимым proof-surface**;
  - **preflop** — **не реализован** в active contract.

### 1.2. Главный вывод по корректности street-strength

После сверхтщательной проверки я **не нашёл конкретной ошибки** в текущем **postflop**-алгоритме определения лучшей руки.

По draw-слою вывод тоже осторожно положительный:

- текущая реализация **уже improvement-aware**;
- ordinary draw действительно строится через **live next-card outs**, а не через голую rank-shape эвристику;
- pure redraw внутри того же класса руки **не считается ordinary draw**;
- pure redraw в `straight_flush` **не маскируется** под обычный straight/flush draw.

Но я **не готов честно сказать “ошибок точно нет вообще”** по всей теме `street_strength`, потому что в репозитории пока нет достаточно сильного **независимого** proof-контура, который бы давал право на такой абсолютный вывод в CI-режиме.

### 1.3. Что реально блокирует вывод “там нет ни единой ошибки”

Это не текущий найденный P0-баг в live-коде постфлопа. Блокеры другие:

1. **Требование “на каждой улице” сейчас не выполнено**, потому что active contract покрывает только **flop / turn / river**.
2. **Reference/differential harness не является по-настоящему независимым**: он слишком сильно зеркалит production-логику, поэтому способен пропустить симметричную ошибку.
3. **Документация частично дрейфует**: в репозитории одновременно есть актуальный контракт и устаревшие audit/status-тексты, которые уже не соответствуют текущему коду.

---

## 2. Что именно я проверил

### 2.1. По репозиторию и стадии разработки

Проверены:

- корневой README;
- `backend/README.md`;
- дерево `backend/crates`;
- наличие `tracker_web_api`, `tracker_ingest_runner`, `tracker_query_runtime`, `mbr_stats_runtime`, `tracker_parser_core`;
- текущий `backend/docs/street_strength_contract.md`;
- устаревший `docs/hand_cat.md`;
- фронтенд-дерево и факт существования уже не только мокового, но и реального backend/API-среза.

### 2.2. По алгоритмам силы руки

Проверены:

- вычисление лучшей 5-карточной комбинации;
- выбор лучшей руки из 5/6/7 карт;
- корректность wheel-стрита `A-2-3-4-5`;
- tie-break внутри каждого класса руки;
- made-hand categories (`pair / two_pair / trips / set / straight / flush / full_house / quads / straight_flush` и rule-based pair-layer);
- ordinary straight draws (`gutshot / open_ended / double_gutshot`);
- ordinary `flush_draw`;
- `backdoor_flush_only`;
- исключение board-only pseudo-draw;
- `is_nut_hand`;
- `is_nut_draw`;
- `missed_flush_draw` / `missed_straight_draw`;
- материализация только на достигнутых улицах;
- seat-scope: Hero + showdown-known opponents.

### 2.3. Дополнительная независимая проверка, которую я сделал

Я отдельно прогнал собственную независимую проверку логики.

#### A. Best-hand evaluator

- **Полное exhaustive-сравнение всех 2,598,960 5-карточных наборов** между независимым эталонным оценщиком и текущей логикой.
- **100,000 случайных 7-карточных наборов** для проверки выбора лучшей руки из 7 карт.

**Результат: расхождений не найдено.**

#### B. Straight-draw shape logic

Я отдельно проверил straight-draw классификацию на уровне rank-state:

- **41,392 валидных flop rank-state**;
- **165,295 валидных turn rank-state**.

Проверялась именно граница между:

- `open_ended`
- `double_gutshot`
- `gutshot`
- `none`

**Результат: расхождений не найдено.**

#### C. Suited / draw / nut-поверхность

Я дополнительно сравнил текущую реализацию с отдельной согласованной reference-логикой на случайных suited-state.

- случайные flop state — без расхождений на проверенной выборке;
- случайные turn state — без расхождений на проверенной выборке;
- отдельная сверка `is_nut_hand` / `is_nut_draw` на случайных state — без расхождений на проверенной выборке.

Важно: это **сильная проверка**, но не математическое доказательство для всех 52-карточных postflop state. Именно поэтому ниже я ставлю задачу на настоящий независимый proof harness.

---

## 3. Что считаю корректным уже сейчас

### 3.1. С высокой уверенностью корректно

#### 3.1.1. Exact best-hand evaluator

С высокой уверенностью корректны:

- `high_card`
- `pair`
- `two_pair`
- `trips`
- `straight`
- `flush`
- `full_house`
- `quads`
- `straight_flush`
- wheel straight
- выбор лучшей руки из 5/6/7 карт
- tie-break внутри класса

### 3.2. В рамках текущего active contract выглядит корректно

#### 3.2.1. Materialization scope

- rows создаются только для **postflop streets**;
- только для **Hero** и **showdown-known opponents**;
- guessed / partial opponents не materialize-ятся.

#### 3.2.2. Draw semantics

Текущий ordinary draw-контракт выглядит внутренне согласованным:

- ordinary draw строится **только** из **legal unseen next-card outs**;
- учитываются только outs, которые **повышают именно `best_hand_class`**;
- board-only pseudo-draw отсекается;
- redraw внутри того же класса руки не считается ordinary draw;
- pure redraw к `straight_flush` не materialize-ится как обычный straight/flush draw.

#### 3.2.3. Nut fields

`is_nut_hand` и `is_nut_draw` в текущем коде **реально активны и реализованы**.

#### 3.2.4. Missed-draw layer

В текущем контракте это **исторические ordinary-draw flags**, а не “на этой улице у меня по факту не было made-hand”.

Это важно:

- `missed_*` у этого проекта — не бытовая метка “draw никогда не доехало”;
- это именно **history-aware exact descriptor** по выбранному контракту.

---

## 4. Найденные проблемы

Ниже только реальные проблемы. Я разделяю их на:

- **проблема покрытия / объёма**;
- **проблема proof-surface / доверия**;
- **проблема документации / сопровождения**;
- **проблема общей зрелости проекта**.

### Проблема P0-1. Требование “на каждой улице” сейчас не выполнено

#### Суть

Текущий active exact contract `street_strength` — **только postflop**.

То есть:

- `flop`
- `turn`
- `river`

Префлоп в active contract отсутствует.

#### Почему это P0

Потому что твоя постановка задачи была именно “на **каждой улице**”.

В текущем виде репозиторий это требование **не закрывает**.

#### Мой вердикт

Это **не баг постфлоп-логики**, а **незакрытый объём**. Но как product-gap это именно **P0**, если целевой контракт должен покрывать все улицы.

---

### Проблема P0-2. Нет по-настоящему независимого proof harness для `street_strength`

#### Суть

В репозитории есть тест, который описан как reference/differential layer. Но по факту support-код reference-слоя слишком близко повторяет production-алгоритм.

Это значит:

- текущие тесты хорошие;
- но они **не дают права утверждать “здесь гарантированно нет ни одной ошибки”**;
- зеркальная ошибка в алгоритме и в reference helper может пройти незамеченной.

#### Почему это P0

Потому что твой критерий очень жёсткий: “там не должно быть ни единой ошибки”. Для такого требования нужен независимый verifier, а не только acceptance tests + mirrored reference.

#### Мой вердикт

Это **не обязательно текущий баг в данных**, но это **критический дефект quality-gate-а**.

---

### Проблема P1-1. Документация частично дрейфует и противоречит текущему коду

#### Суть

В репозитории одновременно есть:

- **актуальный** `backend/docs/street_strength_contract.md`;
- **устаревший** `docs/hand_cat.md`.

`docs/hand_cat.md` уже не соответствует live-коду: он описывает старое состояние и старые выводы.

#### Риск

Это опасно для code-agent и для любого следующего разработчика:

- можно “чинить” уже исправленную проблему;
- можно сломать текущий contract, опираясь на старый текст;
- можно неверно оценить текущую стадию проекта.

#### Мой вердикт

Это **не косметика**, а реальный риск сопровождения.

---

### Проблема P1-2. Семантика `missed_*` и redraw-сценариев слишком легко трактуется неверно

#### Суть

Текущий contract уже вполне конкретный, но его очень легко прочитать неправильно.

Ключевые вещи, которые обязательно надо фиксировать в явном виде:

- `missed_*` — это именно **history-aware ordinary-draw fact**;
- `backdoor_flush_only` сам по себе не порождает `missed_flush_draw`;
- redraw внутри того же hand class не считается ordinary draw;
- same-class improvement тоже не входит в current exact contract.

#### Почему это P1

Потому что это уже не выглядит как live-bug, но это легко становится источником downstream-ошибок в аналитике, фильтрах и ожиданиях команды.

---

### Проблема P1-3. Префлоп не надо насильно впихивать в postflop `street_strength`

#### Суть

Если цель реально “каждая улица”, то префлоп **нужно закрывать отдельно и правильно**.

Префлоп — это не made-hand / draw слой в постфлоп-смысле.

Правильная exact-база для префлопа:

- `AA`, `AKs`, `QJo` и т.д.;
- в текущем v1 достаточно именно canonical starter-hand matrix class;
- дополнительные префлоп-descriptor поля можно обсуждать позже отдельно, если появится реальная downstream-потребность.

#### Почему это важно

Если наспех добавить “preflop_strength” в тот же contract, получится архитектурная каша.

---

### Проблема P2-1. Проект всё ещё далеко от production-grade system

#### Суть

Даже с учётом того, что backend уже не чисто моковый, остаются крупные системные пробелы:

- auth / true session hardening;
- RLS / multi-tenant security;
- object storage / retention / cleanup;
- широкий корпус parser coverage как CI gate;
- replay-grade hardening normalizer-а;
- hand/drilldown API и UI поверх real derived layer.

#### Мой вердикт

Это не мешает сейчас чинить и доводить `street_strength`, но важно правильно оценивать стадию проекта: **это ещё foundation alpha, а не интегрированный продукт**.

---

## 5. Итоговая оценка корректности по улицам

### Префлоп

- **Статус:** в active `street_strength` contract по-прежнему отсутствует, но отдельный preflop exact-layer для starter-hand matrix уже реализован.
- **Вердикт:** preflop закрывается не через `street_strength`, а через отдельный canonical contract с `starter_hand_class`.
- **Оценка:** базовый v1-target по exact preflop matrix filter surface закрыт.

### Флоп

- **Best hand:** выглядит корректно.
- **Pair/category layer:** выглядит корректно как rule-based descriptor layer.
- **Draw layer:** в текущем контракте выглядит корректно.
- **Уверенность:** высокая.

### Тёрн

- **Best hand:** выглядит корректно.
- **Draw layer:** выглядит корректно.
- **Missed-history source:** логически согласован с current contract.
- **Уверенность:** высокая.

### Ривер

- **Best hand:** выглядит корректно.
- **`missed_*`:** выглядит согласованно с текущим history-aware contract, но требует особенно явной документации, чтобы downstream-слой не трактовал поле как “draw точно никогда не доехало”.
- **Уверенность:** средне-высокая.

---

## 6. Стоп-лист перед тем, как модуль можно назвать “надёжно закрытым”

До выполнения пунктов ниже я **не рекомендую** считать тему `street_strength` окончательно закрытой:

1. Закрыть P0 по **proof harness**.
2. Закрыть P0 по **решению preflop-contract**.
3. Убрать документный дрейф.
4. Превратить текущий audit-результат в **повторяемый CI gate**.
5. Зафиксировать канонический semantic decision по `missed_*`, redraw и preflop scope.

---

# 7. План работ для код-агента

Ниже — не общий wishlist, а **конкретный рабочий план**, который можно сразу отдавать код-агенту.

---

## Фаза 0 — заморозить контракт и доказуемость

### CM-P0-01

**Приоритет:** P0  
**Название:** Зафиксировать scope: postflop-only или full-street contract

#### Цель

Устранить двусмысленность “на каждой улице”.

#### Что должен сделать код-агент

1. Принять одно из двух решений и оформить его в явном виде:
   - **Вариант A:** `street_strength` официально остаётся **postflop-only**.
   - **Вариант B:** для префлопа добавляется **отдельный exact-layer**, а не псевдо-натягивание постфлопных полей на префлоп.
2. Если выбран вариант A:
   - синхронизировать все документы, чтобы нигде не оставалось намёка на “каждую улицу” внутри текущего `street_strength`.
3. Если выбран вариант B:
   - спроектировать отдельный exact-layer для preflop descriptors;
   - не использовать `made_hand_category` / `draw_category` как псевдо-префлоп-поля.

#### Где менять

- `backend/docs/street_strength_contract.md`
- `docs/hand_cat.md`
- `README.md`
- `backend/README.md`
- при варианте B: схема БД + materializer + query layer

#### Чек-лист приёмки

- [ ] В репозитории есть **один канонический ответ**, входит ли префлоп в текущий contract.
- [ ] Нет ни одного живого документа, который противоречит этому решению.
- [ ] Если префлоп не входит — это явно написано в README и contract-doc.
- [ ] Если префлоп входит — для него есть отдельная схема exact descriptors.

---

### CM-P0-02

**Приоритет:** P0  
**Название:** Построить действительно независимый verifier для `street_strength`

#### Цель

Сделать так, чтобы корректность слоя доказывалась не mirrored reference helper-ом, а реально независимой проверкой.

#### Что должен сделать код-агент

1. Добавить **второй независимый evaluator / verifier**, который не копирует production helper-стек.
2. Разделить proof surface на 4 уровня:
   - **exhaustive 5-card evaluator differential**;
   - **random 7-card best-hand differential**;
   - **exhaustive rank-state straight-draw proof** для flop/turn;
   - **suited-state differential** для `flush_draw`, `backdoor_flush_only`, `combo_draw`, `is_nut_hand`, `is_nut_draw`, `missed_*`.
3. Важно: production-код и verifier не должны делить одну и ту же внутреннюю helper-логику.
4. Verifier должен запускаться как отдельная команда/скрипт и быть пригоден для CI.

#### Где менять

- `backend/crates/tracker_parser_core/tests/`
- новый отдельный verifier script или отдельный test-support module
- CI workflow
- `backend/docs/street_strength_contract.md` (proof-surface section)

#### Чек-лист приёмки

- [ ] Есть отдельный verifier, который не reuse-ит production helpers для core-логики.
- [ ] Есть exhaustive 5-card pass.
- [ ] Есть random 7-card pass с фиксированным seed и reproducible output.
- [ ] Есть exhaustive flop/turn rank-state straight-draw pass.
- [ ] Есть dedicated suited-state pass для flush/backdoor/nut/missed surface.
- [ ] Verifier встроен в CI или release-gate.

---

### CM-P0-03

**Приоритет:** P0  
**Название:** Убрать опасный документный дрейф

#### Цель

Исключить ситуацию, когда code-agent или новый разработчик опирается на устаревший текст и “чинит” уже исправленное поведение.

#### Что должен сделать код-агент

1. Пометить `docs/hand_cat.md` как **obsolete / historical**, либо убрать его из активной зоны документации.
2. Сделать `backend/docs/street_strength_contract.md` единственным каноническим документом по `street_strength`.
3. Привести root README и backend README к одному и тому же фактическому состоянию проекта.
4. Удалить из активной документации любые утверждения, которые уже не соответствуют live-коду.

#### Где менять

- `docs/hand_cat.md`
- `backend/docs/street_strength_contract.md`
- `README.md`
- `backend/README.md`
- возможно `docs/plans/*`

#### Чек-лист приёмки

- [ ] `docs/hand_cat.md` не может быть ошибочно воспринят как active contract.
- [ ] Есть один canonical contract-doc для `street_strength`.
- [ ] Root README и backend README описывают одну и ту же стадию проекта.
- [ ] Ни один документ не говорит, что `is_nut_hand` / `is_nut_draw` не реализованы, если код уже активен.

---

## Фаза 1 — усилить семантику и закрыть объём

### CM-P1-01

**Приоритет:** P1  
**Название:** Зафиксировать controversial semantics вокруг `missed_*` и redraw

#### Цель

Убрать смысловую двусмысленность в тех местах, которые downstream-слой почти наверняка поймёт неправильно.

#### Что должен сделать код-агент

1. Добавить в contract-doc и tests **явные примеры** для кейсов:
   - ordinary straight draw на flop → к river не straight-family;
   - ordinary flush draw на flop/turn → к river не flush-family;
   - backdoor-only на flop → ordinary flush draw позже появился / не появился;
   - made stronger hand на промежуточной улице и historical missed-факт к river;
   - redraw внутри того же класса руки.
2. Для каждого спорного кейса явно решить:
   - это intended behavior;
   - или это надо поменять.
3. После фиксации решения сделать tests не описательными, а **контрактными**.

#### Где менять

- `backend/docs/street_strength_contract.md`
- `backend/crates/tracker_parser_core/tests/street_hand_strength.rs`
- corpus/golden tests

#### Чек-лист приёмки

- [ ] Для каждого спорного кейса есть минимум один явный acceptance test.
- [ ] В contract-doc есть примеры, а не только общие фразы.
- [ ] Downstream-разработчик может однозначно понять смысл `missed_*` без чтения production-кода.

---

### CM-P1-02

**Приоритет:** P1  
**Название:** Если нужен full-street target — сделать отдельный preflop exact-layer

#### Цель

Закрыть пользовательский target “каждая улица” без архитектурного мусора.

#### Что должен сделать код-агент

1. Спроектировать отдельный `preflop_exact` / `preflop_combo_features` слой.
2. В v1 ограничиться exact-полем `starter_hand_class` (`AA`, `AKs`, `QJo` ...).
3. Добавить materialization и query support через `street = 'preflop'`.
4. Не смешивать префлоп exact descriptors с postflop `made_hand_category` / `draw_category`.
5. Вторичные префлоп-descriptor поля оставить вне текущего scope.

#### Где менять

- схема БД
- materializer/runtime
- `tracker_query_runtime`
- docs
- frontend filters later

#### Чек-лист приёмки

- [ ] У префлопа отдельный exact contract.
- [ ] Нет искусственного `preflop_draw` / `preflop_made_hand_category`.
- [ ] Query layer умеет фильтровать по canonical starter classes.
- [ ] На known hole-cards префлоп descriptors materialize-ятся детерминированно.

---

### CM-P1-03

**Приоритет:** P1  
**Название:** Расширить corpus-backed golden suite по `street_strength`

#### Цель

Перевести текущую высокую уверенность в воспроизводимый regression shield.

#### Что должен сделать код-агент

1. Собрать curated edge-pack из реальных и синтетических рук для:
   - wheel straight;
   - wheel open-ended;
   - board-only draw suppression;
   - flush-made + straight-flush redraw suppression;
   - backdoor promotion;
   - nut vs dominated draw;
   - historical `missed_*`;
   - split-pot / chop nut cases.
2. Держать golden snapshots обновляемыми только через явный флаг.
3. Отдельно сделать aggregated sweep по committed fixtures.

#### Где менять

- `backend/crates/tracker_parser_core/tests/street_strength_corpus_golden.rs`
- fixtures/golden snapshots
- docs/test runbook

#### Чек-лист приёмки

- [ ] Есть curated edge-pack.
- [ ] Есть full-pack sweep.
- [ ] Любой рефакторинг `street_strength` ловится golden diff-ом.
- [ ] Golden update требует явного подтверждения.

---

### CM-P1-04

**Приоритет:** P1  
**Название:** Синхронизировать stage-assessment проекта

#### Цель

Чтобы repo сам себя описывал честно и не сбивал приоритеты разработки.

#### Что должен сделать код-агент

1. Привести в порядок stage-блоки в корневом README.
2. Явно развести:
   - что уже реально поднято (`tracker_web_api`, ingest runner, FT dashboard slice);
   - что всё ещё не production-grade.
3. Убрать формулировки, которые уже устарели.

#### Где менять

- `README.md`
- `backend/README.md`
- `docs/plans/*` и status docs

#### Чек-лист приёмки

- [ ] Из README можно понять реальную стадию проекта без противоречий.
- [ ] Не создаётся ложное впечатление, что frontend всё ещё только mock-only, если реальные вертикальные slice уже существуют.
- [ ] Но и не создаётся ложное впечатление, что проект уже beta/production.

---

## Фаза 2 — продолжение разработки продукта

### CM-P2-01

**Приоритет:** P2  
**Название:** Сделать wide-corpus parser/normalizer gate обязательным

#### Цель

Перевести parser/normalizer из “хорошо на committed pack” в режим измеряемой устойчивости на широком реальном корпусе.

#### Что должен сделать код-агент

1. Поднять reproducible wide-corpus pipeline как обязательный quality gate.
2. Отдельно считать:
   - parse coverage;
   - parse issue breakdown;
   - normalizer ambiguity / reject surface.
3. Развести “accepted exact” vs “quarantined” кейсы.

#### Чек-лист приёмки

- [ ] Есть регулярный отчёт по широкому корпусу.
- [ ] Есть пороги, при которых CI/релиз блокируется.
- [ ] Regression по coverage нельзя пропустить молча.

---

### CM-P2-02

**Приоритет:** P2  
**Название:** Поднять hand/street drilldown HTTP/API слой поверх `tracker_query_runtime`

#### Цель

Сделать derived-layer реально доступным для UI и аналитики.

#### Что должен сделать код-агент

1. Добавить page/API слой для hand/street explorer.
2. Поддержать фильтры по:
   - made hand;
   - draw;
   - missed draw;
   - nut hand / nut draw;
   - preflop starter-hand matrix.
3. Стабилизировать response contract.

#### Чек-лист приёмки

- [ ] Есть API для hand/street drilldown.
- [ ] Реальный derived-layer читается без моков.
- [ ] UI может строить выборки по street-strength surface.

---

### CM-P2-03

**Приоритет:** P2  
**Название:** Довести upload/API/system hardening до production-contour

#### Цель

Закрыть системные пробелы, чтобы проект перестал быть только локальным foundation-срезом.

#### Что должен сделать код-агент

1. true session / auth hardening;
2. object storage + retention/cleanup;
3. upload queue / retries / observability;
4. RLS / tenant isolation;
5. безопасное bundle/job lifecycle управление.

#### Чек-лист приёмки

- [ ] Сессии и доступы не dev-only.
- [ ] Upload pipeline не опирается на локальный сценарий как основной.
- [ ] Есть cleanup/retention strategy.
- [ ] Есть tenant-safe data access.

---

### CM-P2-04

**Приоритет:** P2  
**Название:** Продолжить миграцию stat/runtime слоя и real UI

#### Цель

Довести проект от data-foundation к реально полезному аналитическому продукту.

#### Что должен сделать код-агент

1. Расширять runtime stat-layer поверх уже существующего derived/exact surface.
2. Добавить hand/street explorer UI.
3. Продолжить перенос и материализацию stat-каталога.

#### Чек-лист приёмки

- [ ] Derived-layer реально потребляется UI.
- [ ] Стат-модули не остаются только в inventory.
- [ ] Пользовательские фильтры строятся поверх real backend, а не локального mock math.

---

## 8. Что я бы делал первым, если приоритет — именно надёжность алгоритмов

Если цель сейчас именно “закрыть тему корректности `street_strength`”, порядок такой:

1. **CM-P0-02** — независимый verifier.
2. **CM-P0-03** — убрать документный дрейф.
3. **CM-P1-01** — зафиксировать спорные semantics в тестах и contract-doc.
4. **CM-P1-03** — расширить corpus/golden shield.
5. Только после этого решать, делать ли preflop как отдельный layer.

---

## 9. Итоговый вердикт

### По проекту в целом

Проект находится на стадии **узкого backend-first alpha foundation**.

### По `street_strength`

- **Postflop best-hand evaluator** сейчас выглядит **сильным и корректным**.
- **Postflop draw/nut/missed слой** выглядит **в целом корректным в рамках current contract**.
- **Concrete live-bug в текущем postflop evaluator я не нашёл**.
- Но утверждение “ошибок точно нет” пока нельзя считать закрытым **из-за отсутствия достаточно независимого proof harness**.

### Главные практические выводы

- Исправлять сейчас надо не “какой-то найденный явный баг в лучшей руке”, а **контур доказуемости, документацию и scope**.
- Если нужен target “все улицы”, то **префлоп надо проектировать отдельно**, а не натягивать на postflop semantics.
