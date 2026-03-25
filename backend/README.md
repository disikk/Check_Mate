# Backend foundation

Это backend-ядро нового проекта для покерной школы. Текущая ветка и этот архив ориентированы только на **GG Mystery Battle Royale**.

## Что входит в backend-срез

- `migrations/` — схема PostgreSQL source-of-truth;
- `seeds/` — reference data, включая current GG MBR economics tables;
- `fixtures/mbr/hh/` — committed GG HH;
- `fixtures/mbr/ts/` — committed GG TS;
- `crates/tracker_parser_core/` — parser + normalizer + street strength foundation;
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
   - committed syntax catalog и fixture tests уже есть;
   - широкий реальный корпус ещё не доказан;
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

- `hand_started_at` и `started_at` пока фактически не нормализуются и не используются как надёжное время для аналитики;
- raw/local/provenance timestamp contract уже пишется, но canonical UTC time всё ещё intentionally `NULL`;
- `import-local` жёстко завязан на dev-контекст (`Hero`, `Check Mate Dev Org`);
- `source_files` уже дедуплицируются по `(player_profile_id, room, file_kind, sha256)`, но ingest всё ещё dev-only и не заменяет будущий web/API pipeline;
- runtime materializer делает полный refresh feature-rows для игрока после каждого импорта;
- `hero_exact_ko_count` сейчас трактуется как число KO-событий, а не как KO-share/эквивалент полного KO;
- `is_nut_hand` и `is_nut_draw` в street strength v1 намеренно остаются `NULL`;
- boundary KO persistence пока ограничена boundary v1 point estimate;
- pure `big_ko` decoder ещё не подключён к final stat/materialization contract.

## Что нужно следующим слоем

1. street hand strength v2;
2. feature registry + stat registry + AST engine;
3. web/API ingest.

## Основные команды

```bash
cd backend
cargo test
cargo run -p parser_worker -- "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- import-local "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
cargo run -p parser_worker -- import-local "fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"
```

Canonical first-run path для проекта всё равно идёт **из корня репозитория** через:
- `docker-compose.yml`
- `scripts/`
- `Makefile`
