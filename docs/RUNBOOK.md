# Подробная инструкция по запуску и работе

## Что это за snapshot

Это foundation-срез проекта, а не финальный продукт.

Что уже можно делать:
1. поднять PostgreSQL;
2. применить миграции и seed;
3. прогнать backend unit/integration gates на своей машине;
4. импортировать committed GG fixtures;
5. посмотреть canonical, derived и analytics-слой в БД;
6. поднять frontend prototype.

Что пока нельзя ожидать:
- реального логина;
- кабинета тренера на живом backend;
- upload через web;
- giant popup;
- полного каталога MBR Stats.

---

## 1. Требования

Обязательно:
- Docker Desktop или другой runtime с `docker` и `docker compose`;
- Rust stable toolchain (`cargo`, `rustc`);
- Node.js 22+;
- npm 10+.

Проверка:

```bash
docker --version
docker compose version
cargo --version
rustc --version
node --version
npm --version
```

---

## 2. Подготовка окружения

В корне проекта уже должен быть файл `.env.example`.

Создать `.env`:

```bash
cp .env.example .env
```

Текущие значения по умолчанию:

```dotenv
POSTGRES_USER=postgres
POSTGRES_PASSWORD=postgres
POSTGRES_DB=check_mate_dev
POSTGRES_PORT=5432
CHECK_MATE_DATABASE_URL=host=localhost port=5432 user=postgres password=postgres dbname=check_mate_dev
```

Если меняешь `POSTGRES_*`, обязательно держи `CHECK_MATE_DATABASE_URL` синхронным.

---

## 3. Поднять PostgreSQL и применить схему

```bash
bash scripts/db_up.sh
bash scripts/db_bootstrap.sh
```

Что делает `db_bootstrap.sh`:
- поднимает контейнер PostgreSQL 16;
- ждёт healthcheck;
- синхронизирует пароль роли PostgreSQL с `.env`;
- применяет все migration SQL;
- применяет `backend/seeds/0001_reference_data.sql`, включая current GG Mystery Battle Royale economics reference tables.

Shortcut через `make`:

```bash
make bootstrap
```

---

## 4. Прогнать backend checks

```bash
bash scripts/backend_test.sh
```

Это вызывает:

```bash
cd backend
cargo test
```

Важно:
- часть integration tests в проекте помечена `#[ignore]` и требует живую локальную БД;
- `cargo test` без дополнительных флагов их не запускает.

Если нужен backend-focused path:

```bash
cd backend
bash scripts/bootstrap_backend_dev.sh
bash scripts/run_backend_checks.sh
```

---

## 5. Импортировать committed fixture-pack

### Важное правило
Сначала импортируется **TS**, потом соответствующий **HH**.

Если импортировать HH раньше TS, `parser_worker import-local` вернёт ошибку: турнир ещё не создан в `core.tournaments`.

Текущая timestamp policy для GG MBR:
- `core.tournaments.started_at` и `core.hands.hand_started_at` остаются `NULL`, пока нет exact timezone source;
- при этом importer уже пишет `started_at_raw`, `started_at_local`, `started_at_tz_provenance` и `hand_started_at_raw`, `hand_started_at_local`, `hand_started_at_tz_provenance`;
- текущее provenance значение для committed GG HH/TS: `gg_text_without_timezone`.

### Один пример

```bash
bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"
```

### Импорт всего committed pack

```bash
bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271767530 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0307 - Mystery Battle Royale 25.txt"

bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271767841 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0312 - Mystery Battle Royale 25.txt"

bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271768265 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0316 - Mystery Battle Royale 25.txt"

bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271768505 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0319 - Mystery Battle Royale 25.txt"

bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271768917 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0323 - Mystery Battle Royale 25.txt"

bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271769484 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0338 - Mystery Battle Royale 25.txt"

bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271769772 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0342 - Mystery Battle Royale 25.txt"

bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"

bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271771269 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0351 - Mystery Battle Royale 25.txt"
```

---

## 6. Подключиться к БД и проверить содержимое

Открыть `psql`:

```bash
bash scripts/db_psql.sh
```

Есть готовые запросы в `docs/sql/quick_queries.sql`.

### Базовые smoke-check запросы

#### Турниры

```sql
SELECT COUNT(*) AS tournaments_count
FROM core.tournaments;
```

На committed pack ожидается: `9`.

#### Руки

```sql
SELECT COUNT(*) AS hands_count
FROM core.hands;
```

На committed pack ожидается: `321`.

#### Parse issues

```sql
SELECT COUNT(*) AS parse_issues_count
FROM core.parse_issues;
```

На committed pack ожидается: `0`.

#### Parse issues по severity

```sql
SELECT severity, code, COUNT(*) AS issue_count
FROM core.parse_issues
GROUP BY severity, code
ORDER BY severity, code;
```

На committed pack ожидается: пустой результат.

#### Source file members / job attempts

```sql
SELECT
    (SELECT COUNT(*) FROM import.source_file_members) AS source_file_members_count,
    (SELECT COUNT(*) FROM import.job_attempts) AS job_attempts_count;
```

На committed pack ожидается:
- `source_file_members_count = 18`;
- `job_attempts_count = 18`.

#### Timestamp provenance

```sql
SELECT
    external_tournament_id,
    started_at,
    started_at_raw,
    started_at_local,
    started_at_tz_provenance
FROM core.tournaments
ORDER BY created_at DESC
LIMIT 5;
```

```sql
SELECT
    external_hand_id,
    hand_started_at,
    hand_started_at_raw,
    hand_started_at_local,
    hand_started_at_tz_provenance
FROM core.hands
ORDER BY created_at DESC
LIMIT 5;
```

На committed pack ожидается:
- canonical UTC fields `started_at` / `hand_started_at` равны `NULL`;
- raw/local/provenance поля заполнены.

#### Syntax catalog

Коммитнутый syntax surface зафиксирован в:
- `docs/COMMITTED_PACK_SYNTAX_CATALOG.md`

#### Invariant mismatches

```sql
SELECT COUNT(*) AS invariant_mismatch_count
FROM derived.hand_state_resolutions
WHERE NOT chip_conservation_ok
   OR NOT pot_conservation_ok
   OR jsonb_array_length(invariant_errors) > 0;
```

На committed pack ожидается: `0`.

#### Eliminations

```sql
SELECT hand_id, eliminated_player_name, resolved_by_pot_no, hero_involved, hero_share_fraction, is_split_ko, split_n, is_sidepot_based, certainty_state
FROM derived.hand_eliminations
ORDER BY created_at DESC
LIMIT 20;
```

Важно:
- при exact winner mapping `hero_involved` / `hero_share_fraction` заполнены как обычно;
- при ambiguous winner mapping busted-seat context сохраняется, но guessed `hand_pot_winners` и guessed winner attribution намеренно не materialize-ятся.

#### FT / boundary stage rows

```sql
SELECT hand_id, played_ft_hand, played_ft_hand_state, entered_boundary_zone, entered_boundary_zone_state, ft_table_size, boundary_ko_state, boundary_ko_ev, boundary_ko_min, boundary_ko_max
FROM derived.mbr_stage_resolution
ORDER BY created_at DESC
LIMIT 20;
```

Важно: `boundary_ko_ev / min / max` сейчас уже materialize'ятся, но только как boundary v1 point estimate для последней `5-max` hand перед первой `9-max` hand. Это foundation-level legacy-compatible слой, а не финальный uncertainty-aware EV resolver.

#### Tournament entry economics

```sql
SELECT t.external_tournament_id, te.finish_place, te.regular_prize_money, te.total_payout_money, te.mystery_money_total
FROM core.tournament_entries AS te
INNER JOIN core.tournaments AS t
    ON t.id = te.tournament_id
ORDER BY te.created_at DESC
LIMIT 20;
```

#### Reference MBR economics tables

```sql
SELECT cfg.buyin_total, prize.finish_place, prize.regular_prize_money
FROM ref.mbr_regular_prizes AS prize
INNER JOIN ref.mbr_buyin_configs AS cfg
    ON cfg.id = prize.buyin_config_id
ORDER BY cfg.buyin_total, prize.finish_place;
```

```sql
SELECT cfg.buyin_total, envelope.sort_order, envelope.payout_money, envelope.frequency_per_100m
FROM ref.mbr_mystery_envelopes AS envelope
INNER JOIN ref.mbr_buyin_configs AS cfg
    ON cfg.id = envelope.buyin_config_id
ORDER BY cfg.buyin_total, envelope.sort_order;
```

#### Street hand strength

```sql
SELECT hand_id, seat_no, street, best_hand_class, pair_strength, has_flush_draw, has_open_ended, has_gutshot, has_air, has_missed_draw_by_river, descriptor_version, certainty_state
FROM derived.street_hand_strength
ORDER BY created_at DESC
LIMIT 20;
```

#### Materialized features

```sql
SELECT feature_key, value
FROM analytics.player_hand_bool_features
ORDER BY created_at DESC
LIMIT 20;
```

---

## 7. Поднять frontend

```bash
npm install
npm run build
npm run dev
```

Открыть адрес, который покажет Vite, обычно:

```text
http://localhost:5173
```

Важно:
- frontend сейчас работает на mock data;
- это UI-prototype, а не реальный кабинет на живом backend.

---

## 8. Что тестировать руками в первую очередь

### Блок А. БД и импорт
1. TS import создаёт турнир и tournament entry.
2. HH import создаёт canonical hand rows.
3. Повторный HH import не плодит duplicate `core.hands` children.
4. `derived.hand_state_resolutions` не содержит invariant mismatches.

### Блок Б. Пот-слой и выбивания
1. `core.hand_pots` заполняется.
2. `core.hand_pot_contributions` заполняется.
3. `core.hand_pot_winners` заполняется.
4. `core.hand_returns` работает на uncalled cases.
5. `derived.hand_eliminations` корректно отражает:
   - `resolved_by_pot_no`
   - `hero_involved`
   - `hero_share_fraction`
   - `split_n`
   - `is_sidepot_based`

### Блок В. MBR stage foundation
1. `played_ft_hand` заполняется на 9-max header руках.
2. `ft_table_size` хранит фактическое число игроков, сидящих в руке.
3. `entered_boundary_zone` помечает boundary-candidate руку.
4. `boundary_ko_*` пока не воспринимать как финальную аналитику.

### Блок Г. Street strength foundation
1. строки пишутся в `derived.street_hand_strength`;
2. Hero получает descriptors по всем достигнутым улицам;
3. showdown-known villains тоже появляются там, где карты известны.

---

## 9. Что пока не нужно ожидать

- production auth;
- coach/student visibility;
- RLS;
- upload через web;
- object storage;
- giant popup;
- полный MBR stat catalog;
- query/filter AST engine;
- корректный boundary KO EV слой;
- final stat materialization поверх tournament economics и Big KO decode.

---

## 10. Полезные команды

### Полный цикл

```bash
cp .env.example .env
bash scripts/db_up.sh
bash scripts/db_bootstrap.sh
bash scripts/backend_test.sh
bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"
npm install
npm run build
npm run dev
```

### Reset БД с томом

```bash
bash scripts/db_down.sh --volumes
bash scripts/db_up.sh
bash scripts/db_bootstrap.sh
```

### Через make

```bash
make bootstrap
make backend-test
make frontend-build
make verify
```

---

## 11. Troubleshooting

### `import-local` ругается, что турнир не найден
Причина: HH импортирован раньше TS.

Решение: сначала импортировать соответствующий TS, потом HH.

### После смены `.env` PostgreSQL не пускает
Решение:

```bash
bash scripts/db_bootstrap.sh
```

Этот скрипт специально синхронизирует пароль роли с текущим `.env`.

### Во frontend нет живых данных
Это ожидаемо. Текущий frontend целиком mock-driven.

### Backend tests падают из-за отсутствия БД
Unit-tests должны идти без БД, но ignored integration tests требуют локальную БД.
Для них сначала надо выполнить bootstrap.
