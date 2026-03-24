# Check Mate — foundation snapshot

Это архив текущего состояния нового проекта для покерной школы:

- единая БД под учеников, тренеров и группы;
- новый GG MBR parser/normalizer;
- перенос MBR Stats на новую source-of-truth модель;
- фундамент для будущего большого pop-up и произвольных фильтров/статов.

Важно: это **не готовый production-продукт**, а **рабочая foundation-версия**, которую уже можно поднимать локально, прогонять фикстуры, импортировать реальные GG MBR HH/TS и смотреть, что реально уже пишет в БД.

Канонический onboarding path теперь идёт через root-level `docker-compose.yml`, `scripts/` и `Makefile`. Локальные backend-only helper-скрипты в `backend/scripts/` остаются как secondary verification layer, а не как first-run setup.

## Текущее состояние

Оценка готовности на сейчас:

- система целиком: **20–25%**
- PostgreSQL source-of-truth и multi-tenant foundation: **45–50%**
- GG MBR parser: **40–45%**
- exact normalizer: **30–35%**
- MBR stage/KO derived-слой: **20–25%**
- runtime-слой статов: **10–15%**
- web/frontend кабинеты: **10%**

Подробная оценка и план лежат в:

- `docs/STATUS_ASSESSMENT.md`
- `docs/ROADMAP.md`
- `docs/RUNBOOK.md`

## Что уже работает

### Backend foundation

Есть SQL-схема с отдельными контурами:

- `auth`
- `org`
- `import`
- `core`
- `derived`
- `analytics`

Есть foundation под:

- организации;
- пользователей;
- роли;
- группы;
- player profiles;
- source files;
- import jobs;
- canonical hands;
- normalized pots / returns / winners;
- eliminations;
- first feature materialization.

### GG MBR parser

Сейчас parser умеет:

- различать HH и TS;
- разбирать tournament summary;
- резать HH на отдельные руки;
- парсить seats;
- hero cards;
- actions;
- board;
- showdown cards;
- collect lines;
- total pot / rake / summary board;
- parse warnings.

### Normalizer

Сейчас normalizer умеет:

- считать committed totals;
- строить final pots;
- строить pot contributions;
- строить pot winners;
- обрабатывать uncalled returns;
- считать exact bust по end-of-hand stack;
- считать split KO и side-pot KO в покрытых кейсах;
- считать инварианты chip/pot conservation.

### Derived / analytics

Сейчас уже пишутся:

- `derived.hand_state_resolutions`
- `derived.hand_eliminations`
- `derived.mbr_stage_resolution`
- `analytics.player_hand_*_features`

### Frontend

Есть Vite/React foundation с прототипом кабинета:

- dashboard;
- upload page;
- FT analytics mock pages;
- placeholders под errors/settings.

Сейчас это **UI-прототип**, не полноценный рабочий кабинет.

## Что пока не готово

- нет real API server;
- нет auth-flow;
- нет background queue;
- нет object storage integration;
- нет full GG format coverage;
- нет полного replay-grade pot engine;
- нет street hand strength layer;
- нет полноценного stat/query AST engine;
- не перенесён весь каталог MBR Stats;
- giant popup уровня Hand2Note ещё не реализован.

## Быстрый старт

### 1. Что установить

Минимум:

- Docker Desktop или другой compose-capable runtime с командами `docker` и `docker compose`
- Rust stable toolchain (`cargo`, `rustc`)
- Node.js 22+
- npm 10+

### 2. Поднять PostgreSQL

```bash
cp .env.example .env
bash scripts/db_up.sh
bash scripts/db_bootstrap.sh
```

`db_bootstrap.sh` можно безопасно запускать повторно: он синхронизирует пароль роли PostgreSQL с текущим `.env` даже на уже существующем Docker volume.

Альтернатива через `Makefile`:

```bash
make bootstrap
```

### 3. Прогнать backend tests

```bash
bash scripts/backend_test.sh
```

Или:

```bash
make backend-test
```

### 4. Импортировать sample TS и HH

Сначала **обязательно TS**, потом HH:

```bash
bash scripts/import_fixture.sh "backend/fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
bash scripts/import_fixture.sh "backend/fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"
```

### 5. Посмотреть данные в БД

```bash
bash scripts/db_psql.sh
```

Полезные запросы есть в `docs/sql/quick_queries.sql`.

### 6. Поднять frontend

```bash
npm install
npm run build
npm run dev
```

После этого открой адрес, который покажет Vite, обычно `http://localhost:5173`.

Если нужен только smoke-check без запуска dev-сервера:

```bash
make verify
```

## Самые важные ограничения на сейчас

1. `played_ft_hand` считается как exact-факт по 9-max table header.
2. boundary-zone логика пока только foundation-level, без финального EV-алгоритма.
3. all MBR stats ещё не перенесены.
4. frontend пока не подключён к реальному backend API.
5. street hand strength в БД пока только в схеме, без полной реализации.

## Что тестировать в первую очередь

1. parser tests и import-local workflow;
2. запись canonical hand layer в БД;
3. запись normalized pot layer в БД;
4. запись eliminations;
5. запись `played_ft_hand` и `ft_table_size`;
6. seed runtime materialization.

## Следующие крупные этапы

1. закрыть GG parser coverage;
2. довести normalizer до replay-grade;
3. добавить street hand strength;
4. перенести исправленные MBR stats;
5. реализовать stat/filter engine;
6. сделать реальный web backend;
7. подключить frontend к живому API.
