# Backend foundation

Это backend-ядро нового проекта для покерной школы. Текущая ветка и этот архив ориентированы только на **GG Mystery Battle Royale**.

## Что входит в backend-срез

- `migrations/` — схема PostgreSQL source-of-truth;
- `seeds/` — reference data, включая current GG MBR economics tables;
- `fixtures/mbr/hh/` — committed GG HH;
- `fixtures/mbr/ts/` — committed GG TS;
- `crates/tracker_parser_core/` — parser + normalizer + street strength foundation;
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
- `source_files` дедуплицируются по `(player_profile_id, room, file_kind, sha256)`, а bundle membership живёт отдельно, так что повторные импорты не дублируют exact rows;
- текущий entrypoint всё ещё dev-only: `parser_worker import-local` работает с локальными файлами и локальным runner loop, но это ещё не web upload pipeline;
- runtime materializer теперь запускается один раз на bundle finalize, а не после каждого file job;
- `hero_exact_ko_event_count` сейчас трактуется как число KO-событий, а не как KO-share/эквивалент полного KO;
- generic hand/street query contract теперь живёт в `tracker_query_runtime`, возвращает стабильные `hand_id`-наборы и уже поддерживает `made hand / draw / missed draw / is_nut_hand / is_nut_draw`;
- boundary KO persistence пока ограничена boundary v1 point estimate;
- pure `big_ko` decoder ещё не подключён к final stat/materialization contract.
- real upload/status vertical slice уже поднят:
  - `tracker_web_api` принимает `.txt/.hh/.zip`, создаёт ingest bundles и стримит snapshot/events через WebSocket;
  - `tracker_ingest_runner` — отдельный process-style runner, который дренирует queued ingest jobs;
  - `UploadHandsPage` во фронте больше не использует `mockHandUpload.js`.

## Что нужно следующим слоем

1. hand/drilldown HTTP/API слой поверх `tracker_query_runtime`;
2. реальная аналитическая интеграция FT/dashboard вместо mock data;
3. auth / true session / cleanup / retention hardening для upload slice.

## Основные команды

```bash
cd backend
cargo test
bash scripts/run_wide_corpus_triage.sh
cargo run -p parser_worker -- "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- import-local --player-profile-id <uuid> "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- import-local --player-profile-id <uuid> "fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- set-user-timezone --user-id <uuid> --timezone Asia/Krasnoyarsk
cargo run -p parser_worker -- clear-user-timezone --user-id <uuid>
cargo run -p tracker_web_api --
cargo run -p tracker_ingest_runner -- --once
```

Canonical first-run path для проекта всё равно идёт **из корня репозитория** через:
- `docker-compose.yml`
- `scripts/`
- `Makefile`
