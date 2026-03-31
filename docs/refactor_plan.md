# Check_Mate — архитектурная оценка и план рефакторинга

## Объем просмотра

Оценка ниже основана на просмотре структуры workspace, зависимостей `Cargo.toml`, ключевых backend-крейтов и главных frontend-файлов. Это не аудит каждой строки проекта, а архитектурный разбор по самым важным местам, где уже видно, насколько код масштабируем, тестируем и пригоден к развитию.

Просмотренные ключевые точки:

- `backend/Cargo.toml`
- `backend/crates/parser_worker/*`
- `backend/crates/tracker_web_api/*`
- `backend/crates/tracker_ingest_runtime/*`
- `backend/crates/mbr_stats_runtime/*`
- `backend/crates/tracker_query_runtime/*`
- `backend/crates/tracker_ingest_prepare/*`
- `backend/crates/tracker_parser_core/Cargo.toml`
- `src/App.jsx`
- `src/components/UploadHandsPage.jsx`
- `src/components/FtAnalyticsPage.jsx`
- `src/services/uploadApi.js`
- `src/services/uploadState.js`
- `src/services/ftDashboardApi.js`
- `src/services/ftDashboardState.js`

---

## Краткий вывод

Архитектура **не плохая по замыслу**, но **пока переходная по реализации**.

У проекта уже есть хороший каркас:

- backend разложен по осмысленным крейтам;
- `tracker_parser_core` отделен от web/БД;
- `tracker_ingest_prepare` отделен от runtime-очереди;
- аналитика вынесена в отдельный контекст;
- frontend уже начал отделять транспорт (`uploadApi.js`, `ftDashboardApi.js`) от преобразования состояния (`uploadState.js`, `ftDashboardState.js`).

Но текущая реализация **еще не соответствует** цели “низкая связанность + отсутствие дублирования + легкое добавление новых функций”, потому что главные use-case’ы размазаны по адаптерам и монолитным файлам.

Мой вердикт:

- каркас модульности: **7/10**
- низкая связанность: **4/10**
- отсутствие дублирования: **4/10**
- тестируемость: **6/10**
- удобство расширения: **5/10**

**Итог: 5/10.**

Это не архитектура, которую нужно переписать с нуля. Это архитектура, которую нужно **дочистить, правильно разложить ответственность и убрать “центры гравитации”**, не превращая код в фабрики, репозитории и абстракции ради абстракций.

---

## Что уже сделано правильно

### 1. Границы доменов в backend выбраны в целом верно

Текущие crate-границы логичны:

- `tracker_parser_core` — парсинг/нормализация/доменная логика;
- `tracker_ingest_prepare` — подготовка входных файлов и pairing;
- `tracker_ingest_runtime` — очередь, статусы, снимки, события, выполнение job’ов;
- `tracker_query_runtime` — фильтрация и hand-query логика;
- `mbr_stats_runtime` — аналитика и FT dashboard;
- `tracker_web_api` — web-адаптер;
- `parser_worker` / `tracker_ingest_runner` — процессные/CLI-адаптеры.

Это хорошая основа. Проблема не в том, что контексты не выделены. Проблема в том, что часть orchestration-кода живет **не там, где должна**.

### 2. `tracker_parser_core` выглядит самым здоровым архитектурным контекстом

По зависимостям видно, что `tracker_parser_core` не тащит web/БД слои. Это сильная сторона проекта. Такой контекст легко тестировать, проще расширять новыми правилами парсинга и безопаснее менять.

Его не надо делать “еще абстрактнее”. Его надо, максимум, местами дробить по модулям ради навигации, но это **не первый приоритет**.

### 3. Во frontend уже есть полезное разделение “API ↔ state adapter”

Например:

- `src/services/uploadApi.js` — транспорт;
- `src/services/uploadState.js` — чистые преобразования состояния;
- `src/services/ftDashboardApi.js` — запросы;
- `src/services/ftDashboardState.js` — адаптация снапшота в UI-модель.

Это хорошее направление. Здесь важнее не переписывать все, а **дожать структуру до конца**.

---

## Главные проблемы

## 1. Главный архитектурный узел — `parser_worker/src/local_import.rs`

Это сейчас главный источник связности и риска.

### Почему это проблема

Файл одновременно делает слишком много:

- orchestration end-to-end импорта;
- работу с БД;
- подготовку bundle/file inputs;
- чтение файлов и архивов;
- реализацию `JobExecutor`;
- materialization;
- runner-поведение;
- timezone flow;
- test helpers и legacy helpers;
- огромное количество SQL/persist logic.

### Симптомы

| Файл | Строк | Что внутри |
|---|---:|---|
| `backend/crates/parser_worker/src/local_import.rs` | **14206** | orchestration, БД, ingest runtime, materialize, файлы, архивы, timezone, тесты |

Дополнительно:

- в файле около **248 функций**;
- внутри файла живет **82 теста**;
- публичные use-case функции вынесены прямо сюда:
  - `import_path` — строка ~638
  - `dir_import_path` — строка ~702
  - `run_ingest_runner_until_idle` — строка ~847
  - `run_ingest_runner_until_idle_with_profile` — строка ~858
  - `run_ingest_runner_parallel` — строка ~872
  - `set_user_timezone` — строка ~1534
  - `clear_user_timezone` — строка ~1559

### Почему это мешает развитию

Когда один модуль знает про:

- доменную модель,
- очередь ingest jobs,
- SQL persistence,
- worker orchestration,
- файловую систему,
- materialization,
- timezone пересчет,

любая новая функция начинает “липнуть” туда же. Это и есть архитектурная гравитация. В результате новый код будет добавляться не в правильный bounded context, а в “самый удобный центральный файл”.

**Это главный риск проекта на будущее.**

---

## 2. Оркестрация живет в неправильном владельце

Сейчас по зависимостям видно:

```text
tracker_ingest_runner -> parser_worker
parser_worker -> { mbr_stats_runtime, tracker_ingest_prepare, tracker_ingest_runtime, tracker_parser_core, tracker_query_runtime }
tracker_web_api -> { mbr_stats_runtime, tracker_ingest_prepare, tracker_ingest_runtime, tracker_parser_core }
```

То есть crate с именем `parser_worker` фактически стал не “воркером”, а **прикладным слоем orchestration**.

Это неверное владение ответственностью.

### Почему это важно

- `tracker_ingest_runner` не должен зависеть от `parser_worker` как от центра всей импортной логики.
- `tracker_web_api` не должен сам повторять часть ingest-подготовки и диагностик.
- orchestration-слой должен жить **между адаптерами и runtime/domain crates**, а не внутри одного из адаптеров.

---

## 3. Есть реальное дублирование, не теоретическое

Ниже — не “возможная похожесть”, а конкретные повторяющиеся функции.

### Дубли между `tracker_web_api` и `parser_worker`

| Функция | `tracker_web_api/src/lib.rs` | `parser_worker/src/local_import.rs` | Что сделать |
|---|---:|---:|---|
| `sanitize_filename` | ~938 | ~1288 | вынести в общий ingest use-case модуль |
| `sha256_bytes_hex` | ~948 | ~1298 | вынести в общий ingest use-case модуль |
| `reject_reason_code_as_str` | ~1216 | ~1266 | вынести в модуль reject diagnostics |
| `read_prepared_file_bytes` | ~1095 | ~1210 | вынести в общий builder для prepared inputs |
| `build_reject_diagnostic` | ~1133 | ~1223 | вынести в модуль reject diagnostics |
| `rejected_file_display_path` | ~1168 | ~1246 | вынести в общий reject/helper модуль |
| `map_prepared_source_kind` | ~1208 | ~1258 | вынести в общий mapper |
| `validate_timezone_name` | ~718 | ~1670 | вынести в timezone/use-case модуль |

Часть из них совпадает почти буквально, часть — по сути и назначению.

### Мелкое, но показательное дублирование в аналитике

Есть и внутренние сигналы:

| Функция | Файлы |
|---|---|
| `ratio_to_float_f64` | `mbr_stats_runtime/src/queries.rs`, `mbr_stats_runtime/src/ft_dashboard.rs` |
| `roi_from_totals` | `mbr_stats_runtime/src/queries.rs`, `mbr_stats_runtime/src/ft_dashboard.rs` |

Это не катастрофа, но это хороший индикатор: **повторение начинается там, где не хватает маленьких локальных модулей**, а не “глобальных фреймворков”.

---

## 4. В рабочем модуле остался переходный legacy-код

В `parser_worker/src/local_import.rs` есть прямые комментарии, что legacy helper’ы временно оставлены:

- строки ~1851–1852:
  > The batch-persist path is the live contract, but we keep the legacy per-hand helpers around during this rollout...
- строки ~2409–2410:
  > Legacy single-hand persistence helpers remain only as temporary debug/test references...

Сам факт таких комментариев не плохой. Плохо то, что этот переходный слой остался внутри самого тяжёлого production-модуля.

### Почему это опасно

- люди перестают понимать, что “текущее”, а что “временное”;
- тесты начинают опираться на промежуточный код;
- будущие правки цепляют и старый, и новый путь.

Правильнее: либо быстро удалить, либо изолировать в отдельный test/debug модуль.

---

## 5. Адаптеры слишком толстые

### `tracker_web_api/src/lib.rs`

| Файл | Строк | Проблема |
|---|---:|---|
| `backend/crates/tracker_web_api/src/lib.rs` | **1411** | в одном файле: router, DTO, session, upload-spool, batch classification, валидация, DB transaction boundaries, WS stream, snapshot mapping |

Примеры смешения ответственности:

- маршруты API — ~234+
- загрузка файлов в spool — ~759+
- batch/file classification — ~793+
- archive handling — ~895+
- reject diagnostics — ~1133+
- валидация timezone и datetime — ~700+

Это не “ужас” для прототипа. Но для роста — уже плохо. Новый endpoint очень легко снова начнет тянуть все в этот же файл.

### `tracker_ingest_runtime/src/lib.rs`

| Файл | Строк | Проблема |
|---|---:|---|
| `backend/crates/tracker_ingest_runtime/src/lib.rs` | **2049** | в одном файле: модели, enqueue, snapshots, events, claim/retry/fail/succeed/finalize, executor contract |

Ключевые функции уже намекают, как файл надо резать:

- `enqueue_bundle` — ~348
- `load_bundle_events_since` — ~712
- `load_bundle_snapshot` — ~739
- `load_bundle_summary` — ~970
- `claim_next_job` — ~1044
- `mark_job_succeeded` — ~1200
- `mark_job_failed` — ~1259
- `retry_failed_job` — ~1333
- `maybe_enqueue_finalize_job` — ~1475
- `run_next_job` — ~1545

Это уже не “один модуль”, а фактически пакет модулей в одном файле.

---

## 6. Аналитика уже отделена концептуально, но еще не дорезана внутренне

| Файл | Строк |
|---|---:|
| `backend/crates/mbr_stats_runtime/src/queries.rs` | **2830** |
| `backend/crates/mbr_stats_runtime/src/materializer.rs` | **1481** |
| `backend/crates/mbr_stats_runtime/src/ft_dashboard.rs` | **1481** |
| `backend/crates/tracker_query_runtime/src/filters.rs` | **991** |

Это не значит, что границы выбраны плохо. Наоборот: **границы примерно правильные**. Но внутри каждого контекста слишком большие файлы.

То есть здесь проблема не в архитектуре уровня crate, а в архитектуре уровня модуля.

---

## 7. Тесты есть, но их локальность ухудшена

С тестами ситуация не нулевая — это плюс. Но есть две проблемы:

1. много тестов сидит внутри самых тяжелых файлов;
2. часть integration-поведения сильно сцеплена с крупными модулями и широким setup’ом.

Показательные сигналы:

- в `parser_worker/src/local_import.rs` — **82 теста**;
- `mbr_stats_runtime` и `tracker_web_api` в `dev-dependencies` тянут `parser_worker`.

Для end-to-end тестов это нормально. Для большинства регрессий — нет. При хорошем раскладе:

- pure/domain тесты живут рядом с небольшими модулями;
- use-case smoke tests живут в отдельном слое;
- end-to-end tests остаются, но не являются единственным способом проверить поведение.

---

## 8. Frontend пока терпим, но уже начинает копить связанность

### Где уже хорошо

- `uploadApi.js` и `ftDashboardApi.js` тонкие;
- `uploadState.js` — довольно чистый модуль преобразований;
- `ftDashboardState.js` — хотя и большой, но это хотя бы изолированный адаптер;
- `FtAnalyticsPage.jsx` пока еще контролируемый по размеру.

### Где проблемы

| Файл | Строк | Проблема |
|---|---:|---|
| `src/components/UploadHandsPage.jsx` | **404** | в одном компоненте: session fetch, socket lifecycle, upload start, drag/drop, error flow, UI |
| `src/services/ftDashboardState.js` | **386** | фильтры, карточки, inline stats, charts, formatting helpers в одном файле |
| `src/services/uploadState.js` | **233** | еще нормально, но уже стоит держать под контролем |
| `src/App.jsx` | **85** | пока небольшой, но routing/section dispatch сделан цепочкой `if/else` |

`UploadHandsPage.jsx` — главный кандидат на разрезание:

- `useEffect` с `fetchSessionContext()` — ~65+
- `beginUpload()` с orchestration upload/socket — ~94+
- потом сразу UI

Такой компонент сложно покрывать и легко раздувать дальше.

---

## Главная мысль

Основная проблема проекта — **не нехватка абстракций**, а:

1. **неправильное владение orchestration-кодом**;
2. **монолитные файлы**;
3. **дублирование helper/use-case логики между адаптерами**.

Значит и решение должно быть таким же приземленным:

- не строить новый “enterprise-фреймворк”;
- не прятать все за интерфейсами;
- не делать десять уровней `Repository/Service/Manager/Facade`.

Нужно сделать ровно то, что оправдано:

- один явный слой прикладных ingest use-case’ов;
- несколько нормальных модулей вместо гигантских файлов;
- вынос дублирующихся helper’ов в правильного владельца;
- истончение адаптеров.

---

## Целевая архитектура

## Минимальная целевая схема

```text
HTTP / CLI / runner adapters
    │
    ├── tracker_web_api  ───────────────► mbr_stats_runtime   (read-only dashboard запросы можно оставить напрямую)
    │
    ├── parser_worker (тонкий CLI)
    │
    └── tracker_ingest_runner (тонкий process adapter)
                     │
                     ▼
             tracker_ingest_app   ← единственный новый действительно нужный слой
                     │
                     ├── tracker_ingest_prepare
                     ├── tracker_ingest_runtime
                     ├── tracker_parser_core
                     ├── tracker_query_runtime   (если нужен hand-query/use-case)
                     └── mbr_stats_runtime       (materialize после ingest)
```

### Почему я предлагаю именно один новый слой

Потому что сейчас есть реальная дырка между:

- адаптерами (`tracker_web_api`, `parser_worker`, `tracker_ingest_runner`)
- и доменными/runtime crates.

Эта дырка уже заполняется вручную дублированием и толстым orchestration-кодом.

`tracker_ingest_app` — это **не абстракция ради абстракции**, а явный владелец use-case’ов ingest/import.

### Что НЕ надо делать

- не надо добавлять общий `shared_utils` crate для всего подряд;
- не надо оборачивать `mbr_stats_runtime::query_ft_dashboard()` в отдельный сервис просто “для единообразия”;
- не надо делать DI-контейнер;
- не надо делать generic `Repository<T>` поверх `postgres::GenericClient`;
- не надо тащить новый слой туда, где уже и так хороший API.

---

## Какой должна быть ответственность после рефакторинга

### `tracker_ingest_app` (новый crate)

Должен владеть:

- созданием `IngestFileInput`/`IngestBundleInput` из flat/zip/prepared inputs;
- orchestration single-file / dir import;
- runner use-case’ами;
- reject diagnostics и related mappers;
- timezone/update flow, пока не появится отдельный user/profile context;
- `LocalImportExecutor` как реализацией `tracker_ingest_runtime::JobExecutor`.

### `parser_worker`

Должен стать тонким CLI-адаптером:

- парсинг аргументов;
- вызов use-case функции;
- печать результата.

Сейчас `src/main.rs` уже близок к этому. Его не надо усложнять — наоборот, надо убрать тяжелую логику из библиотеки под ним.

### `tracker_ingest_runner`

Должен стать тонким process-адаптером:

- поднять runner loop;
- вызвать функции из `tracker_ingest_app`;
- завершиться с правильным кодом.

Он **не должен** зависеть от `parser_worker` как от центра импортной логики.

### `tracker_web_api`

Должен оставить у себя:

- router;
- HTTP/WS обработчики;
- DTO;
- API error mapping;
- multipart spool storage;
- минимальную request validation.

Но НЕ должен хранить в себе:

- сборку prepared batch archive;
- общие helper’ы для reject diagnostics;
- shared hash/file kind/source kind helpers;
- ingest orchestration.

### `tracker_ingest_runtime`

Никаких новых уровней над ним не нужно. Его надо просто разрезать по настоящим обязанностям:

- `models.rs`
- `enqueue.rs`
- `events.rs`
- `snapshot.rs`
- `queue.rs`
- `finalize.rs`
- `executor.rs`

### `mbr_stats_runtime`

Оставить как отдельный контекст, но разрезать внутренне:

- `queries/`
- `dashboard/`
- `materializer/`
- `math.rs`

### Frontend

Минимальная pragmatic-цель:

- `src/hooks/useUploadBundle.js`
- `src/hooks/useFtDashboard.js`
- `src/components/upload/*`
- `src/services/ftDashboardState/*` или хотя бы логический split внутри папки
- `src/App.jsx` — убрать ручной `if/else` dispatch в registry mapping

---

## Четкий план для code-agent

Ниже — не абстрактные пожелания, а рабочая последовательность.

## Фаза 0. Заморозить внешнее поведение перед переносами

### Цель

Сначала зафиксировать контракт, потом переносить код.

### Что сделать

1. Не менять бизнес-логику и SQL одновременно с перемещением кода.
2. Перед крупными переносами добавить/сохранить smoke tests для:
   - single-file import;
   - dir import;
   - upload batch;
   - FT dashboard snapshot;
   - WebSocket bundle updates.
3. Зафиксировать текущие публичные точки входа:
   - `parser_worker::local_import::*`
   - web API endpoints
   - FT dashboard response shape

### Критерий готовности

Можно безопасно переносить код между файлами/крейтами, не споря каждый раз, “это архитектурное изменение или изменение поведения”.

---

## Фаза 1. Убрать очевидное дублирование без смены поведения

### Цель

Сначала удалить самый дешевый архитектурный долг.

### Что сделать

Создать общий владелец helper/use-case функций для ingest flow. Если новый crate еще не создан, можно временно начать с нового модуля, который позже переедет в `tracker_ingest_app`.

Вынести из `tracker_web_api/src/lib.rs` и `parser_worker/src/local_import.rs`:

- `sanitize_filename`
- `sha256_bytes_hex`
- `reject_reason_code_as_str`
- `read_prepared_file_bytes`
- `build_reject_diagnostic`
- `rejected_file_display_path`
- `map_prepared_source_kind`
- `validate_timezone_name`

Дополнительно в `mbr_stats_runtime` вынести в локальный `math.rs`:

- `ratio_to_float_f64`
- `roi_from_totals`
- при желании другие маленькие математические helper’ы

### Как делать

- сначала вынести как есть;
- старые места временно превратить в тонкие вызовы нового модуля;
- после стабилизации удалить обертки.

### Критерий готовности

- точные дубли удалены;
- поведение не изменилось;
- web API и parser flow больше не имеют копий одних и тех же helper’ов.

---

## Фаза 2. Вынести ingest use-case слой в правильного владельца

### Цель

Убрать orchestration из `parser_worker` и сделать его переиспользуемым без инверсии зависимостей.

### Что сделать

Создать новый crate:

```text
backend/crates/tracker_ingest_app
```

Минимальный состав модулей на старте:

```text
tracker_ingest_app/
  src/
    lib.rs
    file_input_builder.rs
    rejects.rs
    runner.rs
    timezone.rs
    import_pipeline.rs
    archive_cache.rs
```

### Что перенести туда в первую очередь

Из `parser_worker/src/local_import.rs`:

- `import_path`
- `dir_import_path`
- `run_ingest_runner_until_idle`
- `run_ingest_runner_until_idle_with_profile`
- `default_runner_worker_count`
- `run_ingest_runner_parallel`
- `set_user_timezone`
- `clear_user_timezone`
- `LocalImportExecutor`
- реализацию `impl JobExecutor for LocalImportExecutor`
- подготовку input’ов для ingest
- общие helper’ы, связанные с prepared/archive/reject flow

### Важная деталь

`tracker_ingest_runtime` уже содержит правильный seam:

```rust
pub trait JobExecutor { ... }
```

Его **достаточно**. Новый слой не должен придумывать второй абстрактный executor-framework. Нужно просто перенести `LocalImportExecutor` в правильного владельца.

### Как временно сохранить совместимость

На переходный период:

- `parser_worker::local_import` может просто реэкспортить или вызывать `tracker_ingest_app`;
- CLI-код в `parser_worker/src/main.rs` почти не трогать.

### Критерий готовности

- `tracker_ingest_runner` больше не зависит от `parser_worker` как от логического центра ingest;
- orchestration живет в одном месте;
- `parser_worker` становится тонким CLI-адаптером.

---

## Фаза 3. Истончить адаптеры

### Цель

Сделать так, чтобы адаптеры принимали/отдавали данные, а не владели use-case логикой.

### 3.1 `tracker_web_api`

Разрезать `tracker_web_api/src/lib.rs` так:

```text
tracker_web_api/src/
  lib.rs               // собирает router
  errors.rs
  state.rs
  dto.rs
  validation.rs
  ws.rs
  upload_spool.rs      // только multipart -> spool_path
  routes/
    session.rs
    ingest.rs
    dashboard.rs
```

Что оставить в web API:

- HTTP/WS;
- request/response mapping;
- API errors;
- multipart spool storage;
- минимальную проверку параметров.

Что убрать из web API:

- prepared batch builder;
- shared reject helpers;
- shared file hash/source kind helpers;
- любую orchestration-логику, полезную вне HTTP.

### 3.2 `parser_worker`

Оставить только:

- парсинг CLI аргументов;
- форматирование JSON output;
- вызовы `tracker_ingest_app`.

### 3.3 `tracker_ingest_runner`

Переключить dependency:

```text
было: tracker_ingest_runner -> parser_worker
должно стать: tracker_ingest_runner -> tracker_ingest_app
```

### Критерий готовности

- адаптеры короткие;
- дублирования между адаптерами нет;
- новый endpoint/CLI command добавляется без захода в монолитные файлы.

---

## Фаза 4. Разрезать большие runtime-модули без новых лишних слоев

### 4.1 `tracker_ingest_runtime`

Разделить по обязанностям:

```text
tracker_ingest_runtime/src/
  lib.rs
  models.rs
  status.rs
  enqueue.rs
  events.rs
  snapshot.rs
  queue.rs
  finalize.rs
  executor.rs
```

Принцип:

- это **не новая архитектура**, а просто разложение существующего файла;
- публичный API можно сохранить через `pub use`.

### 4.2 `mbr_stats_runtime`

Разделить:

```text
mbr_stats_runtime/src/
  lib.rs
  math.rs
  queries/
    mod.rs
    seed.rs
    canonical.rs
    loaders_phase_a.rs
    loaders_phase_b.rs
    loaders_phase_c.rs
    formulas.rs
  dashboard/
    mod.rs
    options.rs
    cards.rs
    charts.rs
    coverage.rs
    state.rs
  materializer/
    mod.rs
    loaders.rs
    build_rows.rs
    persist.rs
```

### 4.3 `tracker_query_runtime`

Разделить `filters.rs` так:

```text
tracker_query_runtime/src/
  lib.rs
  filters/
    mod.rs
    model.rs
    validator.rs
    evaluator.rs
    loaders.rs
    sparse_features.rs
```

### Критерий готовности

- в runtime-слоях почти нет файлов > 800–1000 строк;
- `lib.rs` становится точкой сборки, а не свалкой логики;
- поиск кода по ответственности становится очевидным.

---

## Фаза 5. Улучшить тестируемость, не изобретая архитектуру заново

### Цель

Ускорить и локализовать регрессии.

### Что сделать

1. Для чистых helper/adapter функций добавить обычные unit tests рядом с модулем.
2. Для `tracker_ingest_app` добавить use-case smoke tests:
   - build upload input из flat file;
   - build upload input из zip;
   - dir import на минимальном фикстурном наборе;
   - runner проходит очередь до terminal state.
3. Для web API оставить end-to-end / integration tests, но не использовать их как единственный тип тестов.
4. По возможности уменьшить необходимость тянуть `parser_worker` в `dev-dependencies` там, где можно тестировать уже на уровне `tracker_ingest_app`.
5. Тесты, сидящие внутри `local_import.rs`, при переносе разложить по новым модулям или вынести в `tests/`.

### Что НЕ делать

- не вводить mock-слои на все подряд;
- не оборачивать `postgres::GenericClient` в десять trait’ов;
- не делать искусственные интерфейсы там, где достаточно чистой функции.

### Критерий готовности

- большинство регрессий можно проверить локально на уровне небольшого модуля;
- end-to-end тесты остаются, но становятся верхним слоем, а не единственным.

---

## Фаза 6. Frontend: небольшой, но полезный cleanup

### Цель

Остановить рост связности до того, как он станет backend-подобным.

### Что сделать

#### 6.1 `UploadHandsPage.jsx`

Разделить на:

```text
src/hooks/useUploadBundle.js
src/components/upload/
  UploadIntro.jsx
  UploadTimezoneBanner.jsx
  UploadDropzone.jsx
  UploadQueue.jsx
  UploadActivityLog.jsx
```

`useUploadBundle.js` должен владеть:

- `fetchSessionContext()`
- запуском upload
- подпиской на WebSocket
- reset/error lifecycle

Компоненты должны быть в основном презентационными.

#### 6.2 `FtAnalyticsPage.jsx`

Вынести fetch/orchestration в:

```text
src/hooks/useFtDashboard.js
```

Компоненту оставить:

- фильтры;
- рендер карточек/графиков;
- вывод статуса/ошибки.

#### 6.3 `ftDashboardState.js`

Разделить хотя бы логически:

```text
src/services/ftDashboardState/
  index.js
  filters.js
  cards.js
  inlineStats.js
  charts.js
  formatters.js
```

#### 6.4 `App.jsx`

Убрать ручной `if/else` dispatch и перейти на простой registry mapping компонентов по section id.

Здесь не нужен большой роутер, если приложение еще небольшое. Достаточно аккуратной таблицы отображения.

### Критерий готовности

- `UploadHandsPage.jsx` становится коротким и понятным;
- side effects уходят в hooks;
- UI-компоненты можно проверять отдельно от network/socket flow.

---

## Что рефакторить НЕ надо в первую очередь

Это важно, чтобы не уйти в красивую, но бесполезную работу.

### Не первый приоритет

1. **Не начинать с `tracker_parser_core`.**
   Его boundaries как раз выглядят лучше остальных. Если туда лезть первым, это будет потеря времени.

2. **Не переписывать SQL-логику при переносе модулей.**
   Сначала ownership и разрезание файлов. Потом, при желании, локальная чистка запросов.

3. **Не делать общий “core framework”.**
   Проекту не нужен свой мини-Spring/Nest/.NET внутри Rust-крейтов.

4. **Не делать глобальный `shared` crate для любых helper’ов.**
   Shared-мусорник почти всегда ухудшает архитектуру. Helper должен жить у владельца use-case’а.

5. **Не вводить отдельный слой ради FT dashboard, если `mbr_stats_runtime::query_ft_dashboard` и так уже дает нормальную точку входа.**
   Там лучше делать модульную нарезку, а не новый уровень абстракции.

---

## Правила, чтобы не скатиться в абстракции ради абстракций

Ниже — прямые инструкции для code-agent.

### Разрешено

- новые модули;
- **один** новый ingest use-case crate;
- `pub use` для сохранения старого API на переходный период;
- небольшие локальные helper-модули вроде `math.rs`, `rejects.rs`, `validation.rs`;
- plain structs и plain functions;
- reuse существующего `JobExecutor` trait как достаточной границы.

### Запрещено

- DI-контейнер;
- generic `Repository<T>`;
- `ServiceFactory`, `UseCaseExecutor`, `CommandBus`, если нет реальной второй реализации;
- тройные обертки над `postgres::GenericClient`;
- искусственные trait’ы ради тестов, когда можно тестировать чистые функции;
- слой `application` для всего проекта сразу.

### Правило выбора абстракции

Добавлять абстракцию только если выполняется хотя бы одно из трех:

1. есть минимум **два реальных вызывающих контекста**;
2. есть **реальное дублирование**;
3. есть **необходимость изолировать побочные эффекты** для тестов.

Если ни один пункт не выполняется — абстракция не нужна.

---

## Приоритеты по выгоде

### Очень высокий приоритет

1. `parser_worker/src/local_import.rs`
2. общий ingest use-case слой
3. удаление дублирования между `tracker_web_api` и `parser_worker`
4. `tracker_ingest_runner -> tracker_ingest_app`

### Высокий приоритет

5. модульная нарезка `tracker_web_api`
6. модульная нарезка `tracker_ingest_runtime`

### Средний приоритет

7. `mbr_stats_runtime` внутренняя нарезка
8. `tracker_query_runtime` внутренняя нарезка

### Ниже среднего, но полезно

9. frontend hooks/components cleanup
10. `App.jsx` registry mapping

---

## Практический порядок коммитов / PR для code-agent

Если делать это поэтапно, я бы шел так:

### PR 1
- добавить `mbr_stats_runtime::math`
- убрать мелкие дубли в аналитике
- без изменения поведения

### PR 2
- создать `tracker_ingest_app`
- перенести туда общие helper’ы ingest/reject/timezone
- старые места оставить как thin wrappers/reexports

### PR 3
- перенести orchestration из `parser_worker::local_import` в `tracker_ingest_app`
- `parser_worker` сделать thin adapter
- обновить `tracker_ingest_runner`

### PR 4
- разрезать `tracker_web_api`
- переключить endpoints на вызовы `tracker_ingest_app`

### PR 5
- разрезать `tracker_ingest_runtime`

### PR 6
- разрезать `mbr_stats_runtime`
- не менять метрики/формулы, только структуру

### PR 7
- frontend hooks/components cleanup

Такой порядок дает максимальный эффект при минимальном риске.

---

## Критерии готовности архитектуры после рефакторинга

После выполнения плана проект должен удовлетворять следующим условиям.

### По связанности

- `tracker_ingest_runner` больше не зависит от `parser_worker`.
- адаптеры не содержат shared use-case логику.
- orchestration ingest живет в одном владельце.

### По дублированию

- перечисленные выше дубли удалены;
- helper’ы не копируются между web API и worker/runner;
- общая математика аналитики вынесена локально.

### По поддерживаемости

- нет production-файлов на 10k+ строк;
- большие runtime-файлы разложены по обязанностям;
- новый разработчик может по имени модуля понять, где искать код.

### По масштабируемости

Добавление новой возможности происходит по понятному маршруту:

- новый формат/парсер → `tracker_parser_core` / `tracker_ingest_prepare`
- новый ingest use-case → `tracker_ingest_app`
- новое состояние очереди/job → `tracker_ingest_runtime`
- новая аналитическая метрика → `mbr_stats_runtime`
- новый UI flow → hook + page/component, без переписывания огромного контейнера

### По тестируемости

- pure logic тестируется отдельно;
- use-case тестируется на уровне `tracker_ingest_app`;
- web и end-to-end тесты остаются, но не единственные.

---

## Итоговый вердикт

У проекта хороший фундамент, но сейчас он страдает не от недостатка идей, а от **неправильной раскладки ответственности**.

Если делать рефакторинг прагматично, то я бы сформулировал задачу так:

> Не “переизобрести архитектуру”, а:
> 1) убрать централизованный god-модуль,
> 2) вынести ingest orchestration в правильного владельца,
> 3) истончить адаптеры,
> 4) разрезать большие файлы по реальным обязанностям,
> 5) не добавлять ни одного абстракционного слоя без конкретной причины.

Это даст заметный рост по всем нужным тебе критериям:

- меньше дублирования,
- ниже связанность,
- выше тестируемость,
- проще добавлять новые, даже заранее не запланированные функции,
- и при этом код не превратится в лабиринт из бессмысленных абстракций.
