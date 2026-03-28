# Check_Mate — уточнённый план рефакторинга после второй оценки

Дата: 2026-03-28

## Короткий вывод

Вторая оценка в целом полезная и попадает в реальные болевые точки.

Что я **подтверждаю и включаю** в план:
- декомпозицию `backend/crates/parser_worker/src/local_import.rs` как приоритет №1;
- механический split `src/index.css` на несколько файлов без смены стилевой модели;
- вынесение жизненного цикла загрузки из `UploadHandsPage.jsx` в отдельный хук;
- упрощение section routing через единый registry/map;
- удаление подтверждённого мёртвого фронтенд-кода;
- консолидацию реально дублирующихся числовых форматтеров;
- консолидацию реально дублирующихся backend math helper'ов;
- вынос общих тестовых утилит.

Что я **считаю полезным, но с поправками**:
- `FtAnalyticsPage.jsx` можно облегчить, но это не такой срочный приоритет, как `UploadHandsPage.jsx`;
- CSS лучше делить не на 10 мелких файлов, а на 5–6 понятных файлов, чтобы не получить новую фрагментацию;
- test-support нужен, но основная боль там не в "3+ mock executor", а в повторяющемся DB test harness в нескольких integration test файлах.

Что я **не стал бы переносить в план как есть**:
- тезис про дублирование `formatFileSize` — в live-коде не подтверждён;
- тезис, что `cents_to_money` продублирован и в `ft_dashboard.rs` — не подтверждён;
- тезис, что новая секция сейчас требует правок в 3 местах — фактически это 2 точки (`sections.js` + `App.jsx`);
- идею тащить test support через `#[cfg(test)] pub mod` в библиотеку — для integration tests лучше общий `tests/support`.

---

## Подтверждённые факты, которые влияют на архитектуру

- `backend/crates/parser_worker/src/local_import.rs` — 10 144 строк.
- `backend/crates/tracker_web_api/src/lib.rs` — 901 строка.
- `backend/crates/tracker_ingest_runtime/src/lib.rs` — 1 426 строк.
- `src/index.css` — 1 597 строк.
- `src/components/UploadHandsPage.jsx` — 403 строки.
- `src/components/FtAnalyticsPage.jsx` — 179 строк.

Отсюда главный вывод: основной долг здесь не в нехватке абстракций, а в нескольких слишком крупных и плохо разграниченных точках.

---

## Что из второй оценки я принимаю без изменений

### 1. Разбиение `local_import.rs`

Это действительно must-do. Файл слишком велик и смешивает несколько разных причин для изменений:
- импорт и orchestration;
- подготовку row-структур;
- конвертацию доменных данных в persistence shape;
- hand-level persistence;
- tournament / economics / MBR stage persistence;
- timezone related operations;
- вспомогательные SQL-вставки.

### 2. Удаление мёртвого фронтенд-кода

Подтверждено, что как минимум эти файлы не участвуют в live-потоке UI и должны быть убраны до дальнейшего рефакторинга:
- `src/components/MbrStatsPanel.jsx`
- `src/components/FtDistribution.jsx`
- `src/data/ftAnalyticsMock.js`
- `src/services/mockHandUpload.js`

После этого нужно дочистить мёртвые экспорты в `src/data/mockData.js` и селекторы в стилях, если они обслуживали только удалённые компоненты.

### 3. Разделение CSS

Проблема реальна. Один `index.css` на 1 597 строк уже неудобен для навигации и создаёт лишнюю стоимость изменений.

Но я бы делал это только как **механический split**, без переименования классов и без смены подхода к стилям.

### 4. Упрощение section routing

Проблема реальна, но я бы формулировал её аккуратнее:
- сейчас секции описаны в `src/navigation/sections.js`;
- отображение активной страницы задаётся отдельной логикой в `src/App.jsx`.

Это не катастрофа, но для роста проекта нужен единый registry.

---

## Что из второй оценки я принимаю, но уточняю

### 1. Консолидация форматтеров — да, но только для live-дублей

Имеет смысл вынести **только реально общие и живые** числовые форматтеры.

Что стоит вынести:
- `formatInteger`
- `formatDecimal`
- `formatPercent`
- `formatSignedPercent`
- `formatSignedMoney`
- `formatCurrencyFromCents`
- `formatSampleSize`

Куда:
- `src/utils/numberFormat.js`

Что не надо тащить туда без причины:
- `formatFileSize`, пока он живёт в одном месте и не дублируется в live-коде.

Принцип простой: сначала убрать мёртвый код, потом вынести только то, что реально переиспользуется.

### 2. Custom hooks — да, но приоритет разный

`useUploadSession` — высокий приоритет.

Потому что `UploadHandsPage.jsx` уже смешивает:
- загрузку session context;
- orchestration upload;
- websocket lifecycle;
- binding view state;
- UI-рендер.

`useFtDashboard` — допустимо, но вторым шагом.

Причина: `FtAnalyticsPage.jsx` уже заметно легче и опирается на вынесенные сервисы. Её можно упростить, но это не тот же уровень срочности.

### 3. Backend math helpers — да, но узко

Смысл есть вынести только реально дублирующиеся helper-функции из `mbr_stats_runtime` в `math.rs`.

Я бы не превращал это в большой shared util dump.

Подход:
- перенести только helper'ы, которые уже повторяются минимум в двух местах;
- не переносить всё подряд ради "чистоты";
- оставить модуль `pub(crate)`.

### 4. Test support — да, но не так, как предложено

Проблема реальна, но фокус не только в executor mock'ах.

Главная повторяемость сейчас — это integration-test harness:
- поиск миграций;
- накатывание миграций;
- подготовка test DB;
- очистка runtime-таблиц;
- seed базовых данных.

Это лучше вынести в:
- `backend/crates/tracker_ingest_runtime/tests/support/mod.rs`

А не в `#[cfg(test)] pub mod` внутри `src/lib.rs`.

---

## Что я добавляю в план сверх второй оценки

Это важно: вторая оценка хорошо попала в часть проблем, но не покрыла ещё две большие архитектурные точки.

### 1. Декомпозиция `tracker_web_api/src/lib.rs`

Этот файл уже слишком широк по ответственности. В нём сейчас смешаны:
- сборка приложения и state;
- session handlers;
- upload/create bundle flow;
- file classification / archive handling;
- dashboard endpoints;
- websocket stream;
- DTO mapping;
- error mapping.

Это тоже нужно разбивать на внутренние модули. Иначе после `local_import.rs` следующая архитектурная боль переместится сюда.

### 2. Декомпозиция `tracker_ingest_runtime/src/lib.rs`

Файл уже перерос формат одного `lib.rs`.

Без изменения внешнего API его надо разделить хотя бы на:
- `model.rs` / `types.rs`
- `enqueue.rs`
- `snapshot.rs`
- `events.rs`
- `runner.rs`
- `status.rs`
- `repository.rs` или `queries.rs` — если действительно помогает чтению

Смысл не в новых crate, а в том, чтобы внутри crate было проще искать ответственность.

### 3. Синхронизация README с реальным кодом

README уже отстаёт от текущего устройства репозитория. После рефакторинга это надо исправить, иначе документация продолжит врать про границы системы.

---

## Обновлённый план для код-агента

Ниже — последовательность, которая даёт максимальный эффект при минимуме лишних абстракций.

### Фаза 0. Зафиксировать текущее поведение перед крупными переносами

Цель: сделать большие механические переносы безопасными.

Шаги:
1. Не менять поведение.
2. Перед backend split'ами добавить или усилить smoke/integration coverage в наиболее рискованных местах:
   - ingest bundle happy path;
   - bundle snapshot/events path;
   - FT dashboard endpoint;
   - local import happy path.
3. Для фронтенда убедиться, что сборка проходит и нет неявной зависимости на удаляемые mock-компоненты.

Критерий завершения:
- есть baseline, который позволит ловить регрессии после переносов файлов.

---

### Фаза 1. Удалить подтверждённый мёртвый фронтенд-код

Шаги:
1. Удалить:
   - `src/components/MbrStatsPanel.jsx`
   - `src/components/FtDistribution.jsx`
   - `src/data/ftAnalyticsMock.js`
   - `src/services/mockHandUpload.js`
2. Проверить `src/data/mockData.js` и убрать экспорты, оставшиеся только для этих файлов.
3. Проверить стили и удалить селекторы, обслуживавшие только эти компоненты.
4. Пересобрать фронтенд.

Критерий завершения:
- проект собирается;
- live UI не потерял функциональность;
- legacy/mock хвосты убраны до следующих фаз.

---

### Фаза 2. Нормализовать live-форматтеры

Шаги:
1. Создать `src/utils/numberFormat.js`.
2. Перенести туда только live-formatters, реально используемые минимум в двух местах.
3. В `src/services/ftDashboardState.js` заменить локальные helper'ы на импорт.
4. В `src/components/FtChartPanel.jsx` заменить локальный `formatChartNumber` на общий formatter, если сигнатуры совпадают.
5. Не трогать formatter'ы, которые пока живут в одном месте.

Критерий завершения:
- у числового форматирования один источник правды;
- нет util dump из случайных функций.

---

### Фаза 3. Облегчить page-level orchestration на фронтенде

#### Фаза 3A. `useUploadSession`

Шаги:
1. Создать `src/hooks/useUploadSession.js`.
2. Перенести туда:
   - session fetch;
   - upload orchestration;
   - WebSocket lifecycle;
   - state transitions для upload flow.
3. Оставить `UploadHandsPage.jsx` render-oriented компонентом.
4. Не создавать новые абстракции поверх уже существующих `uploadApi` / `uploadState`.

#### Фаза 3B. `useFtDashboard` — только если после Фазы 3A страница всё ещё тяжёлая

Шаги:
1. Создать `src/hooks/useFtDashboard.js`.
2. Перенести fetch/abort/loading/error lifecycle.
3. Оставить адаптацию snapshot там, где она уже логически живёт, если это не ухудшает читаемость.

Критерий завершения:
- page-компоненты читаются как UI, а не как orchestration-скрипты.

---

### Фаза 4. Механический split CSS

Рекомендуемая структура без переусложнения:
- `src/styles/base.css`
- `src/styles/layout.css`
- `src/styles/dashboard-errors.css`
- `src/styles/ft-analytics.css`
- `src/styles/upload-settings.css`
- `src/styles/utilities.css`
- `src/styles/index.css` как единая точка импорта

Шаги:
1. Разрезать `src/index.css` по существующим смысловым блокам.
2. Не переименовывать классы.
3. Не менять подход к темам и CSS variables.
4. Заменить импорт в `src/main.jsx` на `./styles/index.css`.

Критерий завершения:
- стили те же;
- навигация по ним заметно проще;
- нет новой системы ради системы.

---

### Фаза 5. Декомпозировать `parser_worker/src/local_import.rs`

Предпочтительная форма: папка-модуль `parser_worker/src/local_import/`.

Структура:
- `mod.rs` — публичный вход, orchestration, `import_path`, `LocalImportExecutor`, top-level flow
- `rows.rs` — внутренние row-структуры
- `converters.rs` — чистые преобразования в persistence shape
- `persist_hand.rs` — hand-level insert/upsert logic
- `persist_tournament.rs` — tournament summary / economics
- `persist_mbr.rs` — MBR-stage related persistence and helpers
- `persist_common.rs` — общие insert helper'ы и мелкие SQL utilities
- `timezone.rs` — timezone related DB operations
- `tests.rs` или подпапка тестов — по ситуации

Правила:
- внешний API crate не менять;
- все новые модули — `pub(crate)` или приватные;
- не строить repository pattern;
- не пытаться “обобщить” SQL раньше времени.

Критерий завершения:
- чтение hand persistence больше не требует пролистывать MBR stage logic;
- top-level orchestration остаётся читаемой точкой входа;
- поведение не меняется.

---

### Фаза 6. Декомпозировать `tracker_web_api/src/lib.rs`

Рекомендуемая структура:
- `src/lib.rs` — wiring / re-export only
- `src/app.rs` — `build_app`, shared state/config
- `src/session.rs` — session endpoints and mapping
- `src/upload.rs` — create bundle + upload handling
- `src/upload_classification.rs` — archive/flat file classification
- `src/dashboard.rs` — FT dashboard endpoints
- `src/ws.rs` — websocket bootstrap/streaming
- `src/dto.rs` — transport DTOs and mapping
- `src/error.rs` — transport error mapping

Правила:
- не добавлять отдельный application layer, если логика не переиспользуется ещё и в CLI;
- сначала модульная декомпозиция, потом смотреть, нужен ли отдельный service/use-case слой.

Критерий завершения:
- transport слой снова становится обозримым;
- добавление нового endpoint не требует копаться во всём файле.

---

### Фаза 7. Декомпозировать `tracker_ingest_runtime/src/lib.rs`

Рекомендуемая структура:
- `types.rs` / `model.rs`
- `enqueue.rs`
- `snapshot.rs`
- `events.rs`
- `runner.rs`
- `status.rs`
- `finalize.rs` — если после split это действительно отдельная ответственность

Правила:
- не создавать новый crate;
- не разносить случайно связанную логику по слишком мелким модулям;
- сохранить чёткую точку входа через `lib.rs`.

Критерий завершения:
- runtime легче читать и тестировать;
- изменение snapshot/event path не тянет чтение enqueue/run path.

---

### Фаза 8. Узко вынести общие backend math helper'ы

Шаги:
1. Добавить `backend/crates/mbr_stats_runtime/src/math.rs`.
2. Перенести туда только реально повторяющиеся helper'ы для ratio/ROI/average/money conversion, если они уже используются более чем в одном модуле.
3. Сделать модуль `pub(crate)`.
4. Не превращать `math.rs` в свалку несвязанных функций.

Критерий завершения:
- повторяющиеся вычислительные helper'ы определены один раз;
- новых “общих util-помоек” не появилось.

---

### Фаза 9. Упростить section registry и убрать hardcoded session-данные из visual-компонентов

Шаги:
1. Ввести единый registry для секций:
   - `id`
   - `title`
   - `icon`
   - `component`
2. `App.jsx` должен выбирать активный компонент через map/registry, а не через цепочку условий.
3. `Sidebar.jsx` должен получать данные пользователя из session source / mock session source, а не хранить их внутри себя.

Критерий завершения:
- новая секция добавляется в одном концептуальном месте;
- visual component не хранит хардкодные пользовательские данные.

---

### Фаза 10. Вынести общий integration-test support

Шаги:
1. Создать `backend/crates/tracker_ingest_runtime/tests/support/mod.rs`.
2. Перенести туда общие штуки:
   - поиск миграций;
   - применение миграций;
   - подготовка test DB;
   - очистка runtime-таблиц;
   - базовые seed helper'ы;
   - при необходимости общий executor factory.
3. Перевести integration tests на этот support.

Критерий завершения:
- тесты короче;
- повторяемость уменьшается;
- тестовая инфраструктура централизована, но не течёт в production code.

---

### Фаза 11. Обновить README и архитектурные документы

Шаги:
1. Обновить root README под реальную структуру проекта.
2. Явно указать текущие crate и их роли.
3. Убрать устаревшие утверждения о том, чего "ещё нет", если это уже реализовано.
4. При необходимости коротко описать новые внутренние модульные границы после рефакторинга.

Критерий завершения:
- документация снова совпадает с кодом.

---

## Что код-агенту делать НЕ надо

Не делать следующие вещи, если не появится отдельное сильное основание:
- не вводить ORM;
- не вводить generic repository pattern;
- не тянуть Redux/Zustand;
- не мигрировать на TypeScript;
- не переходить на CSS Modules / CSS-in-JS;
- не вводить React Router только ради 5 секций без deep linking;
- не строить общий application layer между web и CLI, пока нет реального совместного use-case reuse;
- не делать "утилитарные" модули, куда сваливается всё подряд.

---

## Рекомендуемый порядок выполнения

1. Фаза 0 — зафиксировать поведение.
2. Фаза 1 — удалить мёртвый фронтенд-код.
3. Фаза 2 — нормализовать live-formatters.
4. Фаза 3A — вынести `useUploadSession`.
5. Фаза 4 — механически split CSS.
6. Фаза 5 — разрезать `local_import.rs`.
7. Фаза 6 — разрезать `tracker_web_api/src/lib.rs`.
8. Фаза 7 — разрезать `tracker_ingest_runtime/src/lib.rs`.
9. Фаза 8 — вынести узкие math helper'ы.
10. Фаза 9 — section registry + session cleanup.
11. Фаза 10 — test support.
12. Фаза 11 — docs sync.

Если нужно максимизировать пользу на ранних шагах, то practical priority такой:
- `local_import.rs`
- `tracker_web_api/src/lib.rs`
- `UploadHandsPage.jsx`
- `tracker_ingest_runtime/src/lib.rs`
- остальное после этого

---

## Проверка после каждой фазы

Минимум:
- frontend build проходит;
- backend тесты и линтеры проходят;
- не меняется публичное поведение API/CLI;
- smoke path на upload и FT dashboard работает как раньше.

Для крупных backend split'ов отдельно:
- happy path local import работает;
- ingest bundle создаётся и обновляет snapshot/events;
- FT dashboard endpoint возвращает прежнюю структуру данных.

---

## Финальный архитектурный ориентир

Цель рефакторинга не в том, чтобы сделать "красивую архитектуру на диаграмме".

Цель такая:
- один файл = одна доминирующая причина изменений;
- transport не содержит лишнюю доменную и инфраструктурную кашу;
- shared helper'ы появляются только после реального повторения;
- новые фичи добавляются за счёт расширения понятных модулей, а не через рост нескольких гигантских файлов.

Это как раз соответствует твоему требованию: **никаких абстракций ради абстракций, только то, что реально уменьшает стоимость будущих изменений**.
