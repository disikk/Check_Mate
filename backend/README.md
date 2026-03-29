# Backend foundation

Это backend-ядро нового проекта для покерной школы. Текущая ветка и этот архив ориентированы только на **GG Mystery Battle Royale**.

## Что входит в backend-срез

- `migrations/` — схема PostgreSQL source-of-truth;
- `seeds/` — reference data, включая current GG MBR economics tables;
- `fixtures/mbr/hh/` — committed GG HH;
- `fixtures/mbr/ts/` — committed GG TS;
- `crates/tracker_parser_core/` — parser + normalizer + street strength foundation;
- `crates/tracker_ingest_prepare/` — shared prepare-layer для directory/ZIP scan, quick HH/TS sniffing, pairing и reject-report;
- `crates/tracker_query_runtime/` — generic typed hand/street query engine over exact and derived filter surface;
- `crates/parser_worker/` — dev-only CLI importer;
- `crates/mbr_stats_runtime/` — первый materializer и seed-safe stat queries.

## Что подтверждено по коду

### Уже хорошо

- схема БД уже отделяет `auth / org / import / core / derived / analytics`;
- parser и normalizer разделены правильно;
- exact elimination трактуется через pot-winner mapping, а не через legacy эвристику;
- persisted split/side-pot признаки уже есть;
- street hand strength foundation уже реализован и пишется в БД;
- отдельный exact preflop starter-hand matrix layer теперь тоже реализован и пишется в `derived.preflop_starting_hands`;
- runtime-слой уже может materialize первый набор hand-features;
- tournament economics уже вынесены в explicit `ref.mbr_*` tables;
- importer уже раскладывает `regular_prize_money` и `mystery_money_total` для current listed GG Royal buy-ins;
- schema v2 hardening уже добавил `player_aliases`, `source_file_members`, `job_attempts` и minimal analytics catalogs;
- GG committed syntax surface теперь зафиксирован в `docs/COMMITTED_PACK_SYNTAX_CATALOG.md`;
- offline `wide_corpus_triage` pipeline теперь есть: committed quarantine sample + optional local bulk corpus дают воспроизводимый parser coverage report;
- parse issues теперь materialize-ятся со structured severity (`warning` / `error`);
- pure non-greedy Big KO decoder уже есть как foundation-модуль.

### Уже есть, но пока только как foundation

- `derived.mbr_stage_resolution`;
- `derived.street_hand_strength`;
- `analytics.player_hand_*_features`;
- idempotent replacement child-rows на повторном HH import;
- ignored integration tests под local PostgreSQL.

## Что пока не доведено до production-grade

1. **Parser coverage**
   - committed GG pack покрывается чисто;
   - committed syntax catalog, quarantine sample и offline triage runner уже есть;
   - широкий реальный корпус теперь можно мерить числом, но он ещё не подключен как production/CI gate;
   - summary seat-result lines ещё не превращаются в полноценную структурную модель результата.

2. **Normalizer**
   - на покрытых тестами кейсах выглядит хорошим;
   - для общего tracker-core ещё не replay-grade вне committed pack;
   - pot winner mapping всё ещё опирается на search over aggregated collect amounts, но ambiguous mappings теперь не materialize-ят guessed `hand_pot_winners`.

3. **MBR-specific layer**
   - `played_ft_hand` есть и это хорошо;
   - boundary-zone больше не placeholder;
   - boundary v1 уже пишет candidate-based `boundary_ko_ev / min / max`;
   - это пока legacy-compatible point estimate, а не финальный uncertainty-aware resolver.

4. **Tournament economics**
   - `total_payout_money`, `regular_prize_money` и `mystery_money_total` уже materialize'ятся;
   - reference tables seeded под current public GG Royal buy-ins `$0.25 / $1 / $3 / $10 / $25`;
   - big-KO decoder уже существует, но пока не доведён до stat-card materialization layer.

5. **Web/system readiness**
   - это пока CLI importer, а не сервис для школы;
   - нет API, auth, upload queue, object storage, RLS.

## Важные технические ограничения текущего состояния

- `hand_started_at` и `started_at` для GG теперь вычисляются только из `auth.users.timezone_name`; без настроенной IANA timezone canonical UTC остаётся `NULL`, а raw/local нельзя трактовать как глобально надёжное время;
- `parser_worker import-local` больше не поднимает dev-контекст автоматически и требует явный `--player-profile-id`;
- `tracker_ingest_runtime` уже даёт воспроизводимый DB-backed ingest contour: `bundle -> bundle_file -> file_ingest jobs -> single bundle_finalize`;
- `tracker_ingest_runtime` теперь ещё и dependency-aware внутри bundle: `import.import_jobs.depends_on_job_id` делает `TS -> HH` явным контрактом, а не побочным эффектом `file_order_index`;
- `source_files` дедуплицируются по `(player_profile_id, room, file_kind, sha256)`, а bundle membership живёт отдельно, так что повторные импорты не дублируют exact rows;
- текущий entrypoint всё ещё dev-only: `parser_worker import-local` работает с локальными файлами и локальным runner loop, но это ещё не web upload pipeline;
- runtime materializer теперь запускается один раз на bundle finalize, а не после каждого file job, и пересчитывает только турниры, затронутые текущим bundle, а не весь профиль целиком;
- materializer analytics writes и hot-path child-table persistence в `parser_worker` теперь идут через batched multi-values `INSERT`, чтобы убрать row-by-row round-trips;
- `parser_worker import-local` теперь возвращает `stage_profile` с фазами `parse/normalize/persist/materialize/finalize`;
- `bulk_local_import` теперь возвращает `file_jobs`, `finalize_jobs`, `hands_persisted`, `runner_elapsed_ms`, `hands_per_minute`, `stage_profile`;
- `hero_exact_ko_event_count` сейчас трактуется как число KO-событий, а не как KO-share/эквивалент полного KO;
- generic hand/street query contract теперь живёт в `tracker_query_runtime`, возвращает стабильные `hand_id`-наборы и уже поддерживает `made hand / draw / missed draw / is_nut_hand / is_nut_draw`, а также preflop starter-hand matrix whitelist filters через `preflop/starter_hand_class`;
- boundary KO persistence пока ограничена boundary v1 point estimate;
- pure `big_ko` decoder ещё не подключён к final stat/materialization contract.
- real upload/status vertical slice уже поднят:
  - `tracker_web_api` принимает `.txt/.hh/.zip`, но теперь использует два upload-контракта:
  - одиночный flat `.txt/.hh` остаётся debug-friendly path и поддерживает TS-only partial bundle;
  - `.zip` и multipart batch из нескольких файлов проходят через shared `tracker_ingest_prepare`, enqueue-ят только валидные HH+TS пары, кладут их в dependency-aware member queue (`TS -> HH`) и пишут reject-диагностику в `import.ingest_events`;
  - `tracker_ingest_runner` — отдельный process-style runner, который теперь умеет `--workers <n>` и по умолчанию использует `min(available_parallelism, 8)` worker-ов для dependency-aware queue path;
  - `parser_worker dir-import --player-profile-id <uuid> [--workers <n>] <path>` теперь тоже идёт по pair-first path: использует shared prepare-layer, materialize-ит synthetic archive с member dependencies, дренирует runtime через тот же dependency-aware runner contract и возвращает честный e2e report с `rejected_by_reason`, `prep_elapsed_ms`, `runner_elapsed_ms`, `e2e_elapsed_ms`, `hands_per_minute_runner`, `hands_per_minute_e2e` и nested `e2e_profile`;
  - после реального `MIHA` profiling выяснилось, что hot-path claim нельзя связывать с unconditional `ingest_bundles` update: runtime теперь не мутирует bundle row на каждом `claim_next_job`, а claim-time event payload строится из read-only derived snapshot, чтобы multi-worker запуск внутри одного большого bundle не сериализовался на одной строке;
  - внутри HH runtime profile теперь разделён на `parse_ms`, `normalize_ms`, `derive_hand_local_ms`, `derive_tournament_ms`, `persist_db_ms`, `materialize_ms`, `finalize_ms`, а legacy `stage_profile` сохранён как совместимый aggregated view;
  - prepare-layer теперь не валится на одиночных не-UTF8 артефактах в реальных директориях: такие файлы и ZIP members попадают в `unsupported_source` reject-report и не блокируют импорт остальных валидных пар;
  - старый `bulk_local_import` остаётся legacy serial helper для list-file сценариев; новый happy-path benchmark и дальнейшая dev-отладка должны идти через `dir-import`;
  - `UploadHandsPage` во фронте больше не использует `mockHandUpload.js`.
- real FT analytics slice тоже уже поднят:
  - `tracker_web_api` теперь отдаёт `GET /api/ft/dashboard` как page-specific MBR/FT snapshot endpoint;
  - runtime считает FT dashboard поверх `mbr_stats_runtime`, а не через generic hand/street query layer;
  - фронт больше не пересчитывает FT mock math локально: `FtAnalyticsPage` использует `ftDashboardApi.js` + `ftDashboardState.js` и честно показывает `ready / empty / partial / blocked`.

## Что нужно следующим слоем

1. добить реальный `MIHA` full-corpus profiling и решить, нужно ли выносить HH compute из долгих транзакций до DB persist;
2. отдельная алгоритмическая волна по `street_strength`, если честный e2e profile подтвердит его как главный CPU bottleneck;
3. hand/drilldown HTTP/API слой поверх `tracker_query_runtime`;

## Основные команды

```bash
cd backend
cargo test
bash scripts/run_wide_corpus_triage.sh
cargo run -p parser_worker -- "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- import-local --player-profile-id <uuid> "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- import-local --player-profile-id <uuid> "fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- dir-import --prepare-only fixtures/mbr/quarantine_sample
cargo run -p parser_worker -- dir-import --player-profile-id <uuid> --workers 4 fixtures/mbr/quarantine_sample
cargo run -p parser_worker --bin bulk_local_import -- --player-profile-id <uuid> --list-file /tmp/files.txt --chunk-size 2
cargo run -p parser_worker -- set-user-timezone --user-id <uuid> --timezone Asia/Krasnoyarsk
cargo run -p parser_worker -- clear-user-timezone --user-id <uuid>
cargo run -p tracker_web_api --
cargo run -p tracker_ingest_runner -- --once
cargo run -p tracker_ingest_runner -- --once --workers 4
bash ../scripts/run_ingest_v2_bench.sh <player-profile-id>
bash ../scripts/run_ingest_v2_mixed_scan.sh [/absolute/path/to/local/mixed-root]
```

Canonical first-run path для проекта всё равно идёт **из корня репозитория** через:
- `docker-compose.yml`
- `scripts/`
- `Makefile`
