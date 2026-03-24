# Подробная инструкция по запуску и работе

## Что это за архив

Это текущий foundation snapshot нового проекта. Его задача сейчас:

1. поднять PostgreSQL source-of-truth;
2. прогонять parser/normalizer tests;
3. импортировать GG MBR HH/TS в БД;
4. смотреть, какие canonical/derived данные уже реально считаются.

Это ещё не финальный web-продукт.

Канонический first-run path для этого snapshot идёт через root-level `docker-compose.yml`, `scripts/` и `Makefile`. `backend/scripts/` нужны для backend-specific bootstrap/checks, но не заменяют основной root onboarding.

---

## Требования

### Обязательные

- Docker Desktop или другой compose-capable runtime с командами `docker` и `docker compose`
- Rust stable toolchain
- Node.js 22+
- npm 10+

### Проверка

```bash
docker --version
docker compose version
cargo --version
rustc --version
node --version
npm --version
```

---

## Шаг 1. Распаковать архив

```bash
unzip check_mate_foundation_2026_03_24.zip
cd check_mate_foundation_2026_03_24
```

---

## Шаг 2. Поднять PostgreSQL

```bash
cp .env.example .env
bash scripts/db_up.sh
bash scripts/db_bootstrap.sh
```

Что это делает:

- запускает PostgreSQL 16 в Docker;
- открывает его на `localhost:5432`;
- выравнивает пароль роли PostgreSQL под текущий `.env`, даже если Docker volume уже существовал;
- применяет все SQL migration файлы;
- применяет seed reference data.

Эквивалентный shortcut:

```bash
make bootstrap
```

---

## Шаг 3. Прогнать backend tests

```bash
bash scripts/backend_test.sh
```

Это базовая проверка, что:

- parser core собирается;
- tests на фикстурах проходят;
- foundation не сломан.

---

## Шаг 4. Импортировать тестовые данные

### Важно

Сначала импортируется **Tournament Summary**, потом **Hand History** этого же турнира.

### Пример

```bash
bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"
```

Если импортировать HH до TS, worker вернёт ошибку, потому что турнир ещё не создан в `core.tournaments`.

---

## Шаг 5. Проверить, что записалось в БД

Открыть psql:

```bash
bash scripts/db_psql.sh
```

Дальше можно запускать запросы из `docs/sql/quick_queries.sql`.

### Быстрые проверки

#### Турниры

```sql
SELECT external_tournament_id, buyin_total, max_players
FROM core.tournaments
ORDER BY created_at DESC;
```

#### Записанные руки

```sql
SELECT external_hand_id, table_name, table_max_seats, small_blind, big_blind, ante
FROM core.hands
ORDER BY created_at DESC
LIMIT 20;
```

#### Exact eliminations

```sql
SELECT eliminated_player_name, resolved_by_pot_no, hero_involved, hero_share_fraction, is_split_ko, split_n, is_sidepot_based, certainty_state
FROM derived.hand_eliminations
ORDER BY created_at DESC;
```

#### FT / boundary layer

```sql
SELECT played_ft_hand, played_ft_hand_state, entered_boundary_zone, entered_boundary_zone_state, ft_table_size
FROM derived.mbr_stage_resolution
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

## Шаг 6. Поднять frontend

```bash
npm install
npm run build
npm run dev
```

Открыть адрес Vite, обычно:

```text
http://localhost:5173
```

### Важно

Frontend сейчас работает на mock data. Он нужен как UI foundation, а не как рабочий кабинет на живых данных.

---

## Полезные сценарии

### Полный локальный цикл

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

### Остановить PostgreSQL

```bash
bash scripts/db_down.sh
```

### Снести PostgreSQL volume и начать с нуля

```bash
bash scripts/db_down.sh --volumes
bash scripts/db_up.sh
bash scripts/db_bootstrap.sh
```

---

## Что тестировать руками в первую очередь

1. Что TS импорт создаёт турнир и tournament entry.
2. Что HH импорт создаёт canonical hand rows.
3. Что `core.hand_pots`, `core.hand_pot_contributions`, `core.hand_pot_winners` заполняются.
4. Что `derived.hand_eliminations` заполняется корректно на split/side-pot кейсах.
5. Что `derived.mbr_stage_resolution.played_ft_hand` и `ft_table_size` заполняются.
6. Что `analytics.player_hand_*_features` обновляются после импорта.

---

## Что пока не нужно ожидать

- реального логина;
- кабинета тренера;
- живой синхронизации frontend ↔ backend;
- giant popup;
- полного каталога MBR Stats;
- полноценных фильтров по силе руки.

---

## Где смотреть дальше

- общий статус: `docs/STATUS_ASSESSMENT.md`
- план работ: `docs/ROADMAP.md`
- backend notes: `backend/README.md`
