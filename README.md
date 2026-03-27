# Check Mate — foundation snapshot

Это текущий инженерный срез нового проекта для покерной школы.

Цель проекта:
- единая система для учеников, тренеров и групп;
- загрузка GG MBR HH/TS;
- канонический parser + normalizer + derived-слой;
- перенос исправленного каталога MBR Stats;
- дальнейшее построение большого pop-up и произвольных фильтров/статов.

Сейчас это **не production-продукт** и **не полноценный трекер**. Это foundation-срез, который уже можно поднять локально, импортировать committed GG fixtures и проверить, что именно пишется в БД.

## Что проверено по этому архиву

Лично перепроверено по содержимому архива:
- в `backend/migrations/0001_init_source_of_truth.sql` описаны **6 схем** и **31 таблица**;
- в проекте есть **3 Rust-crate**: `tracker_parser_core`, `parser_worker`, `mbr_stats_runtime`;
- в `docs/stat_catalog/mbr_stats_inventory.yml` заинвентаризировано **31 legacy-стат-модуль** из старого MBR Stats;
- committed fixture-pack содержит **9 TS**, **9 HH** и **321 руку**;
- синтаксическое покрытие committed GG pack у текущего parser-слоя чистое: на этом паке нет неожиданных unparsed-line событий;
- frontend production build проходит под Node 22 (`npm run build`).

Что **не** было перепроверено в этом sandbox:
- полный bootstrap из чистого Docker volume;
- root-level onboarding целиком с нуля на другой машине.

Что **дополнительно перепроверено локально**:
- `cargo test -p tracker_parser_core`;
- `cargo test -p mbr_stats_runtime`;
- `cargo test -p parser_worker`;
- `cargo test -p parser_worker -- --ignored`.

Итоговая оценка backend-runtime теперь опирается не только на code review и committed fixtures, но и на свежий локальный crate/test verification pass.

## Текущая оценка стадии

- система целиком: **20–25%**;
- схема БД и source-of-truth foundation: **45–50%**;
- GG MBR parser на committed pack: **хороший узкий alpha**;
- GG MBR parser как общий production parser: **ещё далеко не готов**;
- normalizer: **частично корректен, но ещё не replay-grade**;
- MBR stage/KO derived-слой: **foundation-only**;
- runtime stat-layer: **очень ранний seed-safe срез**;
- web/API слой для школы: **ещё не реализован**;
- frontend: **mock UI prototype**.

Подробности лежат в:
- `docs/STATUS_ASSESSMENT.md`
- `docs/ROADMAP.md`
- `docs/RUNBOOK.md`
- `docs/QUALITY_GATES.md`

## Что уже есть

### Backend foundation
Есть SQL-ядро с контурами:
- `auth`
- `org`
- `import`
- `core`
- `derived`
- `analytics`

Есть foundation под:
- организации;
- пользователей;
- membership/roles foundation;
- study groups;
- player profiles;
- source files и import jobs;
- canonical hands;
- normalized pots / winners / returns;
- eliminations;
- street hand strength foundation;
- первый materialized feature-layer.

### Parser + normalizer
Есть:
- определение типа файла `HH` / `TS`;
- parser GG MBR HH и TS на committed fixture-pack;
- split HH на руки;
- canonical hand model;
- normalizer с pot / contribution / winner / return / elimination layer;
- persisted `certainty_state`;
- persisted `split_n`, `hero_share_fraction`, `is_sidepot_based`.

### Runtime foundation
Есть:
- `mbr_stats_runtime`;
- feature registry первого поколения;
- materializer первого поколения;
- seed-safe агрегаты первого поколения.

### Frontend
Есть Vite/React foundation:
- dashboard;
- upload page prototype;
- FT analytics prototype;
- placeholders под будущие разделы.

## Чего пока нет

- real API server;
- auth-flow и сессии;
- object storage;
- queue/background workers;
- полноценный upload pipeline;
- RLS и реальная multi-tenant безопасность;
- full GG parser coverage на широком реальном корпусе;
- replay-grade exact normalizer;
- final boundary KO EV resolver beyond boundary v1 point estimate;
- stat-layer materialization on top of already implemented `regular_prize_money` vs `mystery_money_total` decomposition;
- перенос всех 31 MBR stat-модулей;
- AST/stat/filter engine;
- giant popup/HUD;
- frontend ↔ backend интеграция.

## Важные замечания по корректности

1. `played_ft_hand` сейчас уже отделён правильно как exact-факт по 9-max header.
2. `hero_involved` в `derived.hand_eliminations` трактуется правильно: Hero реально получил долю KO через pot winner mapping.
3. `boundary_ko_ev / min / max` уже пишутся для boundary v1 candidate-hand как legacy-compatible point estimate.
4. `regular_prize_money` и `mystery_money_total` уже декомпозируются через seeded `ref.mbr_*` tables для current listed GG Royal buy-ins.
5. `hand_started_at` и `tournament.started_at` для GG теперь зависят от пользовательской IANA timezone: без неё canonical UTC честно остаётся `NULL`.
6. current `import-local` требует явный `--player-profile-id`; production-path больше не поднимает `Hero` / dev-org автоматически.

## Быстрый старт

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

Подробная и исправленная инструкция — в `docs/RUNBOOK.md`.

## Что делать дальше

Правильный порядок такой:
1. привести проект к полностью воспроизводимому виду;
2. закрыть GG parser coverage;
3. довести normalizer до replay-grade exact core;
4. реализовать корректный MBR stage/KO/economics слой;
5. перенести каталог MBR Stats;
6. сделать stat/filter engine;
7. только потом строить web product и большой popup.
