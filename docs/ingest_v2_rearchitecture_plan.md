# План переработки ingestion v2 для GG -> MBR

## 1. Краткое резюме

Текущая волна оптимизации уже дала реальный прирост: после коммита `2fc90a1` ingestion перестал делать full-refresh всего `player_profile` на каждом `bundle_finalize`, materializer стал scoped, а горячий путь записи в `parser_worker` и в runtime analytics перешёл на пакетные вставки. Это дало примерно `1.45x` ускорения на committed MBR fixture pack.

Но как продуктовый результат этого недостаточно.

Главная проблема уже не в одном локальном SQL-узком месте. Проблема архитектурная:

1. текущий контракт импорта ориентирован на список файлов, а не на пользовательский сценарий `директория/архив -> автоматически собрать пары HH+TS -> отбросить неполные турниры -> загрузить только валидное`;
2. текущий pre-run читает, хеширует и классифицирует все файлы до старта runner;
3. runner по факту serial: один цикл, один `claim`, одна job за раз;
4. в HH-пайплайне CPU-тяжёлые вычисления и DB-запись всё ещё тесно смешаны;
5. на mixed corpus benchmark загрязняется невалидными входами, потому что HH без TS доходят до execution и падают уже внутри ingest.

Именно поэтому следующая волна должна быть не «ещё одна микроправка SQL», а **переработка ingestion-контракта и контура исполнения**.

Цель новой волны:

- получить пользовательский import-контракт `directory/archive import`;
- автоматически находить и спаривать `HH + TS` по `tournament_id`;
- отбраковывать неполные и конфликтные турниры до постановки в очередь;
- дать безопасный параллелизм между независимыми турнирами;
- сохранить детерминизм, доменную семантику и parity результата;
- выйти на уровень `2000+ hands/min` уже по честной **end-to-end** метрике на валидном paired corpus.

---

## 2. Что именно сейчас не устраивает

### 2.1. Неправильный продуктовый контракт импорта

Сейчас удобный для разработчика путь есть, но правильного пользовательского контракта нет.

Вместо импорта директории или архива с автоматическим pairing pipeline всё ещё построен вокруг `bulk_local_import`, которому нужен `list-file` с путями. Такой контракт неудобен сам по себе и не соответствует реальному UX.

Следствие:
- пользовательский импорт приходится готовить вручную;
- pairing `HH + TS` не является обязательной частью ingestion;
- неполные турниры попадают в очередь и ломают картину по статусам и скорости.

### 2.2. Смешение валидных и невалидных входов в одном потоке

На mixed corpus видно, что часть архива вообще не является happy-path MBR corpus. Там есть:
- paired турниры;
- неполные пары;
- обычные GG HH, не относящиеся к нужному сценарию;
- дубли и потенциально конфликтные файлы.

Сейчас это не отсекается заранее. В результате очередь загрязняется job-ами, которые заведомо нельзя успешно довести до `core.hands`, а потом они падают с сообщениями вроде «сначала импортируй TS».

Это не просто UX-проблема. Это искажает benchmark:
- время тратится на мусорные входы;
- throughput по полезным рукам не виден;
- bundle/job telemetry становится менее информативной.

### 2.3. Честный end-to-end throughput сейчас не виден

Текущие поля `runner_elapsed_ms` и `hands_per_minute` полезны, но они измеряют не весь процесс. Они стартуют после того, как файлы уже прочитаны, классифицированы, захешированы и поставлены в очередь.

Значит для directory/archive import текущий `hands_per_minute` — это не честный показатель пользовательского опыта. Пользователь ждёт не «когда стартовал runner», а «когда закончилось всё».

### 2.4. Runner сейчас архитектурно однопоточный

Даже после сделанных ускорений сам runtime-дренаж очереди остаётся последовательным:
- один loop;
- один `claim_next_job`;
- один `LIMIT 1`;
- одна исполняемая job за раз.

Пока так, верхняя граница throughput ограничена не только SQL и CPU внутри одной job, но и самим фактом serial queue drain.

### 2.5. CPU-вычисления в HH job ещё недостаточно разведены от записи в БД

`persist_ms` сейчас включает не только реальный DB I/O, но и существенную часть derived/exact вычислений, особенно:
- normalizer;
- `street_strength`;
- `preflop_starting_hands`;
- подготовку зависимых derived row-set’ов.

Из-за этого profile недостаточно честно показывает, где именно уходит время, а сам pipeline хуже поддаётся распараллеливанию.

### 2.6. Опасность наивного многопоточного запуска

Просто добавить несколько worker-потоков поверх текущего queue contract нельзя.

Причина: сейчас claim идёт по общему списку queued jobs с сортировкой по bundle/order, но без явной зависимости `HH job зависит от TS job`. Если один worker уже залочил ранний TS, другой worker сможет взять более поздний HH из той же bundle. Это потенциально ломает ключевой порядок `TS -> HH`.

Значит безопасный multi-worker возможен только после введения dependency-aware очереди.

---

## 3. Что уже сделано и что нужно сохранить

Новая волна не стартует с нуля. Важно не сломать уже сделанные улучшения.

### Уже есть

1. DB-backed ingest contour:
   - `bundle -> bundle_file -> file_ingest jobs -> single bundle_finalize`.

2. Scoped materialization:
   - finalize больше не full-refresh-ит весь `player_profile`;
   - materialization ограничена турнирами текущего bundle.

3. Пакетные вставки:
   - runtime analytics rows пишутся chunked multi-values insert;
   - горячие child exact/derived inserts в `parser_worker` тоже уже пакетированы.

4. Stage-level profiling:
   - `parse_ms`;
   - `normalize_ms`;
   - `persist_ms`;
   - `materialize_ms`;
   - `finalize_ms`.

5. Отдельный runtime/upload slice уже существует:
   - `tracker_web_api` принимает `.txt/.hh/.zip`;
   - `tracker_ingest_runner` дренирует queued jobs;
   - `parser_worker` остаётся dev-only CLI importer.

### Что принципиально нельзя потерять

1. Никакой смены доменной семантики.
2. Никакого дрейфа canonical MBR-контрактов.
3. Parity по:
   - `core.*`;
   - `derived.*`;
   - `analytics.*`;
   - `query_canonical_stats`;
   - `query_ft_dashboard`.
4. Детерминизм результата независимо от числа worker-ов.
5. Поддержку новых GG HH/TS форматов.

---

## 4. Главная идея новой архитектуры

Новая архитектура должна перейти от модели:

`список файлов -> слепой enqueue -> serial execution -> поздние terminal fail`

к модели:

`directory/archive -> scan/index -> pair HH+TS -> reject bad tournaments -> enqueue only valid pairs -> dependency-aware parallel execution -> scoped finalize -> structured result`

Ключевая мысль:
- **единица входа для пользователя** — директория или архив;
- **единица валидации** — турнирная пара `HH + TS`;
- **единица безопасного параллелизма** — независимый турнир;
- **единица итоговой materialization/finalize семантики** — пользовательский import bundle.

То есть пользователь импортирует «одну загрузку», но внутри неё система исполняет много независимых tournament-pair job-цепочек.

---

## 5. Проектные принципы

### 5.1. Pair-first, а не file-first

Пока в очередь попадают отдельные файлы, система неизбежно будет сталкиваться с поздними ошибками по неполным турнирам. Для MBR happy-path корректный контракт — сначала собрать pair, потом исполнять.

### 5.2. Общий prepare-layer для CLI и web upload

Нельзя строить это только вокруг `parser_worker`, потому что он dev-only. Pairing, scan и reject-report должны быть общими и использоваться:
- и в `parser_worker dir-import`;
- и в `tracker_web_api` upload path;
- и в будущих служебных задачах rehydrate/backfill.

### 5.3. Параллелизм только там, где есть независимость

Параллелить можно:
- между разными турнирами;
- внутри HH-job — hand-local compute.

Нельзя наивно параллелить всё подряд:
- `TS -> HH` внутри одной пары должно быть упорядочено;
- турнирный reduce-слой должен оставаться детерминированным;
- finalize по bundle должен оставаться однозначным.

### 5.4. Сначала контракт и наблюдаемость, потом агрессивные алгоритмические оптимизации

До тех пор пока нет:
- честной e2e метрики;
- pair-aware queue;
- детального профиля CPU vs DB;
- parity между 1 и N worker,

не стоит лезть в рискованные low-level оптимизации с сильным влиянием на exact semantics.

---

## 6. Целевой продуктовый контракт

### Входы

Система должна принимать:

1. директорию;
2. архив;
3. отдельный файл (как частный случай, полезный для отладки).

### Поведение

1. Сканирует содержимое.
2. Быстро определяет тип источника.
3. Извлекает `tournament_id` без полного тяжёлого разбора.
4. Группирует по `tournament_id`.
5. Строит пары `TS + HH`.
6. Выявляет:
   - `missing_ts`;
   - `missing_hh`;
   - `conflicting_ts`;
   - `conflicting_hh`;
   - `unsupported_source`;
   - `missing_tournament_id`.
7. В очередь ставит **только валидные пары**.
8. По отвергнутым турнирам формирует структурированный report.

### Выходы

Итог импорта должен возвращать/сохранять:

- количество просканированных файлов;
- количество валидных paired tournaments;
- количество отвергнутых турниров по каждой причине;
- `prep_elapsed_ms`;
- `runner_elapsed_ms`;
- `e2e_elapsed_ms`;
- `hands_per_minute_runner`;
- `hands_per_minute_e2e`;
- stage-profile;
- parity/consistency summary;
- список reject-диагностик для фронта.

---

## 7. Фазовый план переработки

## Фаза 0. Зафиксировать baseline, сценарии и quality gates

### Зачем

Перед крупной переработкой нельзя опираться только на один fixture pack и один mixed corpus. Иначе потом будет непонятно:
- ускорили ли мы happy-path или просто изменили состав входов;
- где regression по данным, а где просто другой набор файлов;
- насколько `2000 hands/min` относится к честному e2e сценарию.

### Что делаем

1. Формализуем два класса benchmark-сценариев:
   - **happy-path paired corpus** — только валидные `HH + TS`;
   - **mixed corpus** — реальный грязный архив для проверки scan/pair/reject/report.

2. Фиксируем acceptance baseline:
   - parity row-counts;
   - parity canonical query outputs;
   - wall-clock и e2e throughput;
   - одинаковый результат на `1 worker` и `N workers`.

3. Добавляем benchmark harness:
   - прогон happy-path через новый `dir-import`;
   - прогон mixed corpus через scan/pair/reject без требования успешного ingest для всех файлов.

### Почему это отдельная фаза

Потому что без этого следующие фазы невозможно честно оценить. Нужна не «одна цифра скорости», а воспроизводимая система измерения.

### Артефакты

- `scripts/run_ingest_v2_bench.sh`
- `scripts/run_ingest_v2_mixed_scan.sh`
- документ `docs/INGEST_V2_BENCHMARKS.md`

### Критерий готовности

Есть воспроизводимые baseline-замеры:
- happy-path throughput;
- mixed corpus reject-report;
- parity наборов данных и запросов.

---

## Фаза 1. Общий prepare-layer: scan, classify, pair, reject

### Зачем

Сейчас система слишком поздно понимает, что часть входов невалидна. Pairing должен стать не «побочным эффектом runtime», а первым обязательным шагом ingest.

### Что делаем

Создаём общий слой подготовки импорта, условно `tracker_ingest_prepare`.

Он должен уметь:

1. сканировать директорию рекурсивно;
2. читать архив и перечислять его members;
3. быстро определять `source_kind` по первой непустой строке;
4. быстро извлекать `tournament_id` из заголовка;
5. строить карту `tournament_id -> кандидаты HH/TS`;
6. выявлять валидные пары и причины reject;
7. лениво считать полный `sha256` только там, где это действительно нужно.

### Почему это нужно делать именно так

Если по-прежнему сначала делать полный `read_to_string + sha256` для всех файлов, а потом уже решать, нужен ли файл вообще, мы продолжим терять много времени до старта runner. Поэтому здесь важен двухшаговый подход:

1. **быстрый header sniff** — минимальная стоимость на файл;
2. **полный хеш/полная подготовка** — только для кандидатов, которые реально войдут в ingest.

### Предлагаемая модель данных

```rust
pub enum PreparedSourceKind {
    HandHistory,
    TournamentSummary,
}

pub struct PreparedFileRef {
    pub path_or_member: String,
    pub source_kind: PreparedSourceKind,
    pub tournament_id: String,
    pub byte_size: i64,
    pub sha256: Option<String>,
}

pub struct PreparedTournamentPair {
    pub tournament_id: String,
    pub ts: PreparedFileRef,
    pub hh: PreparedFileRef,
}

pub enum RejectReasonCode {
    MissingTs,
    MissingHh,
    ConflictingTs,
    ConflictingHh,
    UnsupportedSource,
    MissingTournamentId,
    DuplicateSameContent,
}

pub struct RejectedTournament {
    pub tournament_id: Option<String>,
    pub files: Vec<PreparedFileRef>,
    pub reason_code: RejectReasonCode,
    pub reason_text: String,
}

pub struct PrepareReport {
    pub scanned_files: usize,
    pub paired_tournaments: Vec<PreparedTournamentPair>,
    pub rejected_tournaments: Vec<RejectedTournament>,
    pub scan_ms: u64,
    pub pair_ms: u64,
    pub hash_ms: u64,
}
```

### Где меняем код

1. `backend/crates/tracker_parser_core/src/file_kind.rs`
   - добавить `quick_detect_source_kind(...)`;
   - добавить `quick_extract_gg_tournament_id(...)`.

2. Новый crate или модуль:
   - `backend/crates/tracker_ingest_prepare/`
   - либо `backend/crates/tracker_ingest_runtime/src/prepare.rs`

3. `parser_worker`
   - новый dev CLI `dir-import`.

4. `tracker_web_api`
   - перевод upload path на тот же prepare-layer.

### Что считаем правильным поведением

- Для одного `tournament_id` должен быть ровно один валидный `TS` и один валидный `HH`.
- Если найдено несколько одинаковых файлов с одинаковым содержимым, это можно схлопывать как дубликаты.
- Если найдено несколько разных файлов на одну и ту же роль (`HH` или `TS`) с разным содержимым, турнир отвергается как конфликтный.
- Неполные пары не должны попадать в ingest queue вообще.

### Риски

1. Неправильное быстрое извлечение `tournament_id` на новых форматах.
2. Ложное схлопывание дубликатов.
3. Переход от file-first к pair-first затронет CLI и web upload одновременно.

### Как страхуемся

- golden tests на quick header parsers;
- corpus tests на смешанных директориях;
- отдельные тесты на конфликтные дубликаты;
- compare against full parser for sample corpus.

### Критерий готовности

На mixed corpus:
- есть корректный pair/reject report;
- HH без TS не доходят до `failed_terminal` внутри runner;
- в очередь ставятся только валидные пары.

---

## Фаза 2. Pair-aware queue contract и dependency-гейтинг job-ов

### Зачем

Без этого безопасный multi-worker невозможен.

Сейчас порядок внутри bundle не является достаточно сильной гарантией для параллельного исполнения. Нужна явная зависимость: HH-job можно исполнять только после успешного TS-job соответствующей пары.

### Что делаем

Добавляем в ingest runtime понятие зависимости job-а от другого job-а.

### Изменение схемы БД

Нужна миграция, условно `0022_pair_aware_ingest_queue.sql`.

Предлагаемое изменение:

```sql
ALTER TABLE import.import_jobs
ADD COLUMN depends_on_job_id uuid NULL REFERENCES import.import_jobs(id);

CREATE INDEX import_jobs_claim_dependency_idx
ON import.import_jobs(status, job_kind, depends_on_job_id);
```

### Новая семантика enqueue

При подготовке bundle:

- для каждой валидной пары создаётся TS file job;
- для соответствующего HH file job ставится `depends_on_job_id = ts_job_id`;
- `bundle_finalize` остаётся один на весь пользовательский import bundle.

### Новая семантика claim

`claim_next_job(...)` должен брать job только если:
- `status = 'queued'`;
- `job_kind` допустим для claim;
- `depends_on_job_id is null`
  **или**
  зависимая job уже имеет terminal success.

### Новая семантика propagation

Если TS-job завершился terminal fail, зависимый HH-job должен:
- либо автоматически переводиться в `failed_terminal` с кодом `dependency_failed`;
- либо считаться blocked и финализироваться как terminal fail при закрытии bundle.

Рекомендуемое поведение: переводить зависимый HH-job сразу в terminal fail с явным reason code, чтобы статус bundle и диагностика были прозрачны.

### Почему это лучше, чем просто полагаться на `file_order_index`

Потому что `file_order_index` работает только как слабая очередность при serial исполнении. Как только появляется несколько worker-ов и `SKIP LOCKED`, другой worker может обойти раннюю job, если она уже залочена кем-то ещё.

Явная зависимость делает порядок частью контракта, а не случайным свойством текущего scheduler-а.

### Где меняем код

1. новая миграция в `backend/migrations/`;
2. `backend/crates/tracker_ingest_runtime/src/lib.rs`
   - enqueue;
   - claim;
   - failure propagation;
   - bundle status refresh;
   - event emission.

3. `tracker_web_api`
   - статус blocked/dependency_failed должен корректно отображаться пользователю.

### Риски

1. Сложнее SQL claim query.
2. Нужна аккуратная логика перерасчёта bundle status.
3. Возможны edge-cases на retryable failure зависимой job.

### Как страхуемся

- unit tests на claim logic;
- integration tests:
  - TS success -> HH claimable;
  - TS running -> HH not claimable;
  - TS terminal fail -> HH dependency_failed;
  - несколько workers не могут нарушить порядок пары.

### Критерий готовности

На одной и той же bundle:
- при `1 worker` и при `N workers` результат идентичен;
- HH никогда не стартует раньше успешного TS своей пары.

---

## Фаза 3. Multi-worker runner

### Зачем

После dependency-гейтинга появляется возможность безопасно использовать несколько ядер и несколько DB-соединений для независимых турниров.

### Что делаем

Добавляем параллельный режим выполнения runner-а.

### Новый интерфейс

```rust
pub fn run_ingest_runner_parallel(
    database_url: &str,
    runner_name: &str,
    max_attempts: i32,
    worker_count: usize,
) -> Result<IngestRunProfile>;
```

### Принцип работы

- создаём `worker_count` потоков;
- каждый поток открывает собственный PostgreSQL client;
- каждый поток крутит свой loop:
  - `claim_next_job`;
  - execute;
  - commit;
  - повтор.

Параллелизм идёт:
- между независимыми tournament pairs;
- между bundle-ами;
- но не внутри одной dependency-цепочки `TS -> HH`.

### Почему finalize оставляем bundle-level

Потому что уже есть рабочая и семантически понятная модель:
- файловые job-и пишут exact/derived слой;
- после завершения всех file job-ов bundle делает один scoped finalize/materialize.

Это сохраняет deterministic materialization boundary.

### Где меняем код

1. `backend/crates/parser_worker/src/local_import.rs`
   - `run_ingest_runner_parallel(...)`.

2. `backend/crates/parser_worker/src/bin/bulk_local_import.rs`
   - опция `--workers`.

3. `backend/crates/parser_worker/src/main.rs`
   - опция `--workers` для `dir-import`.

4. `backend/crates/tracker_ingest_runner/`
   - продуктовый runner тоже должен получить режим `--workers N`.

### Какой дефолт

Практичный старт:
- `min(num_cpus::get(), 8)`.

Почему не «сколько угодно»:
- слишком много соединений и contention в PostgreSQL;
- на раннем этапе важнее предсказуемость, чем максимум на синтетическом железе.

### Риски

1. Рост DB contention.
2. Сложности с профилем: суммарное время worker-ов и wall-clock — это разные величины.
3. Более сложная отладка retry/claim race-ов.

### Как страхуемся

- отдельный профиль по worker-ам;
- parity tests на `1`, `2`, `4`, `8` worker;
- ограничение максимального `worker_count`;
- нагрузочные тесты на committed paired corpus.

### Критерий готовности

- `1 worker` и `N workers` дают одинаковые данные;
- wall-clock заметно снижается;
- queue drain становится реально многопоточным;
- нет нарушения `TS -> HH`.

---

## Фаза 4. Рефакторинг HH pipeline: compute отдельно, tournament-reduce отдельно, DB persist отдельно

### Зачем

Сейчас значимая доля времени скрыта внутри `persist_ms`, хотя это не только запись в БД. Пока CPU-часть и DB I/O смешаны, throughput труднее поднять и труднее честно измерять.

### Что делаем

Перестраиваем внутренний pipeline HH-job на три явные стадии.

### Новая структура HH-job

#### Шаг 1. Parse

- разрезать файл на hand fragments;
- получить canonical hand representation;
- это остаётся в основном последовательным и сравнительно дешёвым.

#### Шаг 2. Hand-local compute (параллельно)

Для каждой руки независимо:

- normalizer;
- positions;
- `evaluate_street_hand_strength`;
- `evaluate_preflop_starting_hands`;
- подготовка exact/derived row-set’ов, не требующих уже известного DB `hand_id`.

Это можно распараллелить через `rayon`.

#### Шаг 3. Tournament reduce (последовательно, детерминированно)

После hand-local compute выполняем то, что зависит от упорядоченного набора рук турнира:

- сортировка по `tournament_hand_order`;
- `StageHandFact`;
- `BoundaryResolution`;
- `MbrTournamentFtHelperRow`;
- прочие межручные derived-структуры.

Это сознательно оставляем последовательным, потому что здесь важнее детерминизм и ясность, чем агрессивное распараллеливание.

#### Шаг 4. Чистый DB persist

Теперь, когда все row-set’ы уже подготовлены:

- batched upsert `core.hands`;
- получение `hand_id`;
- batch insert child tables;
- batch insert derived tables.

### Почему именно такой split

Потому что здесь нельзя честно сказать «всё параллелим по рукам». В коде уже есть турнирный reduce-слой. Если его игнорировать, можно упростить картину на бумаге, но получить сложный и хрупкий runtime на практике.

### Предлагаемые структуры

```rust
pub struct ComputedHandLocal {
    pub hand_key: String,
    pub canonical: CanonicalParsedHand,
    pub normalized: NormalizedHand,
    pub positions: Vec<HandPositionRow>,
    pub street_strength_rows: Vec<StreetHandStrengthRow>,
    pub preflop_rows: Vec<PreflopStartingHandRow>,
    pub persistence: CanonicalHandPersistence,
    pub local_derived: HandLocalDerivedRows,
}

pub struct TournamentReduceOutput {
    pub stage_resolution_rows: Vec<StageResolutionRow>,
    pub boundary_rows: Vec<BoundaryResolutionRow>,
    pub ft_helper_rows: Vec<MbrTournamentFtHelperRow>,
}

pub struct PersistPlan {
    pub core_hand_rows: Vec<CoreHandRow>,
    pub child_rows: ChildRowSets,
    pub derived_rows: DerivedRowSets,
}
```

### Где меняем код

1. `backend/crates/parser_worker/Cargo.toml`
   - добавить `rayon`.

2. `backend/crates/parser_worker/src/local_import.rs`
   - рефакторинг `import_hand_history_registered(...)`.

3. По необходимости:
   - отдельный модуль `hh_compute.rs`;
   - отдельный модуль `hh_reduce.rs`;
   - отдельный модуль `hh_persist.rs`.

### Что важно не делать

1. Не смешивать hand-local parallel compute с DB-записью.
2. Не пытаться распараллелить stage/boundary reduce, пока нет доказательства, что это безопасно и реально окупается.
3. Не ломать существующие exact/derived контракты ради красивого профиля.

### Риски

1. Рост потребления памяти, если держать слишком много pre-built row-set’ов одновременно.
2. Сложность mapping `hand_key -> hand_id` после batch insert.
3. Возможный перекос: CPU ускорился, но база стала новым bottleneck.

### Как страхуемся

- ограничение размера hand-batch внутри одного HH файла;
- отдельные microbenchmarks на CPU compute;
- сравнение старого и нового набора row-set’ов на одинаковом fixture pack.

### Критерий готовности

- `persist_ms` начинает означать в основном DB I/O, а не вычисления;
- CPU-часть HH-job заметно ускоряется;
- parity по данным сохраняется.

---

## Фаза 5. Честное профилирование и новый output contract

### Зачем

После фаз 1–4 без нового профиля мы снова не увидим, где именно ускорение, а где просто перемещение времени из одной стадии в другую.

### Что делаем

Сохраняем текущий stage-profile для обратной совместимости, но добавляем второй уровень детализации.

### Предлагаемая модель

```rust
pub struct PrepareProfile {
    pub scan_ms: u64,
    pub pair_ms: u64,
    pub hash_ms: u64,
    pub enqueue_ms: u64,
}

pub struct ComputeProfile {
    pub parse_ms: u64,
    pub normalize_ms: u64,
    pub derive_hand_local_ms: u64,
    pub derive_tournament_ms: u64,
    pub persist_db_ms: u64,
    pub materialize_ms: u64,
    pub finalize_ms: u64,
}

pub struct IngestE2eProfile {
    pub prepare: PrepareProfile,
    pub runtime: ComputeProfile,
    pub prep_elapsed_ms: u64,
    pub runner_elapsed_ms: u64,
    pub e2e_elapsed_ms: u64,
}
```

### Какие метрики обязательно нужны

1. `hands_per_minute_runner`
2. `hands_per_minute_e2e`
3. `paired_tournaments`
4. `rejected_tournaments`
5. `rejected_by_reason`
6. `workers_used`
7. `db_round_trip_estimate` — опционально, если получится получить без большого шума

### Почему важно сохранить старые поля

Чтобы сравнение с уже сделанными замерами не разорвалось. Нужна преемственность:
- старый `stage_profile` остаётся;
- новый `e2e_profile` добавляется рядом.

### Где меняем код

1. `parser_worker`
2. `tracker_web_api`
3. `tracker_ingest_runner`
4. Документация `backend/README.md`, `CLAUDE.md`, runbooks

### Критерий готовности

Любой новый прогон отвечает сразу на три вопроса:
1. сколько ушло на подготовку входов;
2. сколько ушло на actual execution;
3. где именно внутри execution сидит bottleneck.

---

## Фаза 6. Алгоритмическая оптимизация `street_strength`

### Зачем

После ввода pair-first ingest, multi-worker и hand-local parallel compute следующим крупным bottleneck с высокой вероятностью останется `street_strength`, особенно nut-hand и nut-draw логика.

Это уже не «архитектурная» фаза, а именно алгоритмическая.

### Что делаем

Выделяем отдельную волну под оптимизацию `street_strength` без изменения смысла exact/derived contract.

### Основная идея

Вместо того чтобы многократно заново перебирать пространство возможных hole-card комбинаций оппонента для одного и того же board, вводим board-level caches:

- nut-rank cache;
- family cache;
- возможно, кеш пространства legal opponent combos для данного board и blocked cards.

### Почему это не надо делать раньше

Потому что это более рискованная оптимизация:
- сильно касается exact-поведения;
- требует особенно аккуратной parity-проверки;
- её настоящий выигрыш лучше измерять уже после того, как вычищены prep/queue/parallel bottleneck-и.

### Что именно можно сделать

1. Предвычислять «максимально возможную силу руки на этом board».
2. Не пересчитывать заново nut-check для каждого out через полный opponent-space проход.
3. Рассмотреть мемоизацию промежуточных `evaluate_best_hand(...)` на board-derived state.

### Что не делать в этой фазе

- не менять смысл `is_nut_hand`;
- не менять смысл `is_nut_draw`;
- не подменять exact logic эвристикой ради скорости.

### Риски

Это самая чувствительная фаза по semantic regression.

### Как страхуемся

- отдельные golden tests на `street_strength`;
- microbenchmark до/после;
- corpus parity на `derived.street_hand_strength`;
- сравнение downstream query outputs.

### Критерий готовности

`street_strength` перестаёт быть доминирующим CPU bottleneck без изменения exact результата.

---

## Фаза 7. Product integration и замена текущего «главного» контракта импорта

### Зачем

Даже если все низкоуровневые ускорения сделаны, задача не закрыта, пока продуктовый путь всё ещё мысленно крутится вокруг `bulk_local_import`.

### Что делаем

1. Новый основной контракт:
   - `dir-import` для CLI;
   - upload directory/archive path для web.

2. `bulk_local_import`
   - остаётся как benchmark/dev harness;
   - перестаёт рассматриваться как основной пользовательский способ ingestion.

3. Фронтенд
   - получает structured reject-report;
   - показывает по каждому отвергнутому турниру понятную причину;
   - отображает `accepted / rejected / running / completed`.

### Почему это важно

Нужно разделить:
- инженерный инструмент;
- продуктовый контракт.

Иначе архитектурные решения будут и дальше приниматься под dev-only путь.

### Критерий готовности

Пользовательский импорт директории/архива работает без ручной подготовки list-file и без поздних непонятных terminal fail по неполным парам.

---

## 8. Конкретный порядок реализации

Рекомендуемый порядок такой:

### Шаг 1. Фаза 0
Сначала фиксируем benchmark harness и quality gates.

### Шаг 2. Фаза 1
Делаем общий prepare-layer с pairing и reject-report.

Это сразу решает продуктовую боль и очищает benchmark от мусорных входов.

### Шаг 3. Фаза 2
Вводим dependency-aware queue contract.

Это обязательная предпосылка для безопасного multi-worker.

### Шаг 4. Фаза 3
Поднимаем multi-worker runner.

После этого появляется первый большой прирост wall-clock на paired corpus.

### Шаг 5. Фаза 4
Рефакторим HH-job на compute / reduce / persist.

Это даёт следующий крупный прирост за счёт параллельного CPU и более чистого DB hot path.

### Шаг 6. Фаза 5
Делаем честное e2e профилирование и новый output contract.

### Шаг 7. Фаза 6
Только после этого идём в алгоритмический hotspot `street_strength`.

### Шаг 8. Фаза 7
Закрываем product integration и закрепляем новый ingest contract как основной.

---

## 9. Что именно включено из внешнего плана и что скорректировано

Ниже — сводка того, что разумно взять, а что нужно поправить.

### Берём как правильное направление

1. directory import;
2. pairing `HH + TS`;
3. reject incomplete pairs;
4. hand-local распараллеливание через `rayon`;
5. multi-worker runner;
6. честное профилирование;
7. отдельную позднюю волну по hotspot-у `street_strength`.

### Исправляем по архитектуре

#### 1. Не делаем это parser_worker-centric

Pairing и prepare-layer должны использоваться не только в `parser_worker`, но и в `tracker_web_api`.

#### 2. Не запускаем multi-worker на текущем queue contract

Сначала dependency-aware queue, потом параллелизм.

#### 3. Не считаем, что весь derive-слой параллелится просто «по рукам»

Есть hand-local часть и есть tournament reduce.

#### 4. Не считаем текущий `hands_per_minute` честным e2e throughput

Нужно отдельно показывать prep и e2e.

---

## 10. Ключевые решения по схеме данных и интерфейсам

### 10.1. Bundle остаётся единицей пользовательского импорта

Не нужно создавать отдельный bundle на каждый турнир. Один пользовательский import должен по-прежнему иметь один bundle:
- так проще UX;
- проще bundle-level events;
- проще один итоговый finalize.

### 10.2. Но внутри bundle job-ы становятся dependency-aware

Это компромисс:
- снаружи один import;
- внутри множество независимых pair-цепочек.

### 10.3. Reject-турниры не ставим в queue

Это очень важно. Невалидный турнир — это не ingest job, а диагностический результат этапа подготовки.

### 10.4. Hashing делаем лениво

На больших директориях это может дать заметный выигрыш ещё до runner-а.

---

## 11. План проверки и приёмки

## 11.1. Unit tests

1. quick detect source kind;
2. quick extract tournament id;
3. duplicate/conflict pairing;
4. dependency claim rules;
5. batch row builders;
6. hand-local compute equivalence.

## 11.2. Integration tests

1. `dir-import` на committed paired fixtures;
2. mixed corpus scan/reject report;
3. `1 worker` vs `N workers`;
4. dependency failure propagation;
5. archive import parity vs directory import parity.

## 11.3. DB tests

1. parity по row-counts:
   - `core.hands`;
   - `derived.street_hand_strength`;
   - `derived.preflop_starting_hands`;
   - `analytics.player_hand_*`;
   - `analytics.player_street_*`.

2. parity по canonical query outputs:
   - `query_canonical_stats`;
   - `query_ft_dashboard`.

## 11.4. Performance tests

### Happy-path paired corpus
Измеряем:
- `prep_elapsed_ms`;
- `runner_elapsed_ms`;
- `e2e_elapsed_ms`;
- `hands_per_minute_runner`;
- `hands_per_minute_e2e`.

### Mixed corpus
Измеряем:
- скорость scan/pair;
- качество reject-report;
- отсутствие поздних HH-without-TS terminal fails.

---

## 12. Риски проекта и как их контролировать

### Риск 1. Семантический дрейф при агрессивной оптимизации
**Контроль:** parity-first rollout, golden tests, staged release.

### Риск 2. Параллелизм даст contention в PostgreSQL
**Контроль:** ограничение числа worker-ов, bench на разных значениях, индексы под claim/dependency.

### Риск 3. Рост памяти после разделения compute и persist
**Контроль:** размер внутренних batch-окон, потоковая обработка HH внутри файла, лимит pre-built row-set’ов.

### Риск 4. Быстрый detector `tournament_id` окажется слишком хрупким
**Контроль:** fallback на более дорогой parsing path для сомнительных случаев, corpus tests на новых форматах.

### Риск 5. Неправильная обработка дублей
**Контроль:** различать exact duplicate и conflicting duplicate, логировать обе ситуации отдельно.

---

## 13. Ожидаемый результат по фазам

### После фаз 1–2
- правильный продуктовый ingest contract;
- чистый pair-first queue;
- понятный reject-report;
- исчезновение поздних HH-without-TS terminal fails на mixed corpus.

### После фазы 3
- заметное уменьшение wall-clock за счёт multi-worker execution.

### После фазы 4
- ещё один крупный прирост за счёт параллельного CPU внутри HH-job и более чистого DB hot path.

### После фазы 5
- честная end-to-end наблюдаемость.

### После фазы 6
- дополнительное ускорение на самом тяжёлом алгоритмическом участке.

### После фазы 7
- продуктовый импорт директории/архива становится основным и понятным пользовательским сценарием.

---

## 14. Минимальный набор конкретных изменений по файлам

### Схема / миграции
- `backend/migrations/0022_pair_aware_ingest_queue.sql`

### Core parser / detection
- `backend/crates/tracker_parser_core/src/file_kind.rs`
- возможно `backend/crates/tracker_parser_core/src/lib.rs`

### Новый prepare-layer
- `backend/crates/tracker_ingest_prepare/src/lib.rs`
- `backend/crates/tracker_ingest_prepare/src/scan.rs`
- `backend/crates/tracker_ingest_prepare/src/pair.rs`
- `backend/crates/tracker_ingest_prepare/src/archive.rs`

### Ingest runtime
- `backend/crates/tracker_ingest_runtime/src/lib.rs`

### Parser worker / dev CLI
- `backend/crates/parser_worker/src/local_import.rs`
- `backend/crates/parser_worker/src/main.rs`
- `backend/crates/parser_worker/src/bin/bulk_local_import.rs`

### Product runner
- `backend/crates/tracker_ingest_runner/...`

### Web upload path
- `backend/crates/tracker_web_api/...`

### Hotspot optimization
- `backend/crates/tracker_parser_core/src/street_strength.rs`

### Тесты
- prepare-layer tests;
- dependency queue tests;
- multi-worker parity tests;
- street-strength golden tests;
- end-to-end dir-import tests.

---

## 15. Итоговый вывод

Сделанная волна ускорения была правильной и полезной, но она в основном вычистила локальные bottleneck-и внутри уже существующего контура. Этого недостаточно, чтобы получить действительно быстрый, устойчивый и продуктово правильный ingest для большого живого корпуса.

Следующий этап должен менять не только скорость отдельных SQL-операций, а **сам контракт ingestion**:

1. сначала scan/pair/reject;
2. потом dependency-aware enqueue;
3. потом безопасный multi-worker;
4. потом разделение compute и persist;
5. потом честное e2e профилирование;
6. потом algorithmic hotspot tuning.

Именно такая последовательность даёт шанс одновременно:
- поднять throughput до целевого класса;
- очистить UX;
- не потерять детерминизм;
- не поломать exact/derived/runtime parity.

---

## 16. Короткая формула плана

Если в одной фразе, то план такой:

**перевести ingestion с file-first serial importer на pair-first dependency-aware parallel import pipeline с честной e2e наблюдаемостью и отдельной поздней алгоритмической оптимизацией street_strength.**

---

## 17. Опорные текущие места в коде

Ниже — точки, на которые опирается этот план:

- `backend/crates/parser_worker/src/bin/bulk_local_import.rs`
- `backend/crates/parser_worker/src/local_import.rs`
- `backend/crates/tracker_ingest_runtime/src/lib.rs`
- `backend/crates/mbr_stats_runtime/src/materializer.rs`
- `backend/crates/tracker_parser_core/src/file_kind.rs`
- `backend/crates/tracker_parser_core/src/street_strength.rs`
- `backend/README.md`

Документ опирается на текущее состояние после волны ускорения со scoped finalize и batched persist, а также на выводы по поведению mixed corpus и на необходимость product-grade directory/archive import.
