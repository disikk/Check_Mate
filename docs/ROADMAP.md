# Чёткий план исправления и продолжения разработки

## Базовый принцип

Порядок фиксированный. Нельзя перепрыгивать через этапы.

1. Репозиторий и воспроизводимость.
2. Схема БД v2.
3. Реальный ingest pipeline.
4. Полное покрытие GG parser.
5. Replay-grade normalizer.
6. MBR stage/economics layer.
7. Street hand strength v2.
8. Feature/stat engine.
9. Перенос каталога MBR Stats.
10. Web backend для школы.
11. Frontend integration.
12. Popup/HUD и performance layer.

Acceptance criteria по фазам лежат в `docs/QUALITY_GATES.md`.

Статус на `2026-03-24`:
- Phase 1 schema hardening закрыта через `backend/migrations/0004_exact_core_schema_v2.sql`;
- committed-pack slice для parser coverage и replay-grade exact core закрыт;
- stage/economics foundation уже реализован;
- следующий логический шаг после текущего exact-core hardening: **Фаза 6 / street hand strength v2**.

---

## Фаза 0. Привести репозиторий в воспроизводимое состояние

### Цель
Сделать так, чтобы любой разработчик и любой агент могли одинаково поднять и проверить проект.

### Сделать
1. Удалить из репозитория и будущих архиваций:
   - `dist/`
   - `backend/target/`
   - `__MACOSX/`
   - `.DS_Store`
2. Зафиксировать root `.gitignore`.
3. Держать `.env.example` в корне.
4. Зафиксировать committed fixtures как канонический minimal pack.
5. Привести все README/runbook-файлы к одному фактическому состоянию проекта.
6. Поднять CI минимум на:
   - migrations;
   - parser tests;
   - normalizer tests;
   - frontend build.

### Выходные артефакты
- чистый репозиторий;
- `.env.example`;
- root `.gitignore`;
- CI pipeline.

### Gate
См. `G0` в `docs/QUALITY_GATES.md`.

---

## Фаза 1. Усилить схему БД до v2

### Цель
Довести foundation schema до уровня, на котором реально можно строить систему для школы и stat engine.

### Сделать

### 1.1 Добавить недостающие сущности
1. `core.player_aliases`
2. `import.source_file_members`
3. `import.job_attempts`
4. `ref.mbr_buyin_configs`
5. `ref.mbr_regular_prizes`
6. `ref.mbr_mystery_envelopes`
7. `analytics.feature_catalog`
8. `analytics.stat_catalog`
9. `analytics.stat_dependencies`
10. `analytics.materialization_policies`

### 1.2 Добавить временные поля
Для турниров и рук хранить минимум:
- raw time string;
- parsed local time;
- utc time;
- timezone provenance.

### 1.3 Усилить целостность
Добавить composite FK/unique constraints для:
- `(hand_id, seat_no)` child tables;
- `(hand_id, pot_no)` pot child tables;
- source file dedupe;
- archive-member dedupe.

### 1.4 Согласовать reference model
Убрать концептуальную рассинхронизацию, где часть room/model живёт как text, а часть как reference tables.

### Выходные артефакты
- миграция `0004_exact_core_schema_v2.sql`;
- обновлённый seed reference data;
- схема, достаточная для stat engine и web foundation.

### Gate
См. `G1`.

### Статус
Закрыто в текущем snapshot.

---

## Фаза 2. Построить реальный ingest pipeline

### Цель
Перестать опираться на dev-only `import-local` как на основной путь загрузки.

### Сделать
1. API endpoint для загрузки ZIP/TXT.
2. Сохранение файла в object storage.
3. Создание `source_file` + `import_job`.
4. Worker contract для разбора job.
5. ZIP splitting → `source_file_members`.
6. Dedup policy на source files и members.
7. Retry model через `job_attempts`.
8. Progress/state model для UI.

### Выходные артефакты
- API contract;
- parser worker contract;
- object storage contract;
- import state machine.

### Gate
См. `G2` и `G8` частично.

---

## Фаза 3. Закрыть parser coverage для GG MBR

### Цель
Сделать parser форматно-полным для реального GG MBR корпуса.

### Сделать
1. Собрать expanded golden pack.
2. Завести syntax catalog.
3. На каждый syntax pattern сделать fixture test.
4. Поддержать и классифицировать:
   - hidden dealt lines;
   - showdown variants;
   - collect variants;
   - summary seat-result lines;
   - repeated collect;
   - no-show / muck / partial reveal, если встретятся;
   - future edge variants из реального корпуса.
5. Ввести нормальный severity model для parse issues:
   - info;
   - warning;
   - error.
6. Не позволять новой неизвестной строке исчезать без следа.

### Важное требование
Parser не считает статы. Parser только строит каноническую модель.

### Gate
См. `G2`.

### Статус
Закрыто для committed `9 HH + 9 TS` GG pack:
- syntax catalog зафиксирован в `docs/COMMITTED_PACK_SYNTAX_CATALOG.md`;
- parser-worker пишет structured parse issue severity;
- committed pack держит `warning-level parse issues = 0`.

---

## Фаза 4. Довести normalizer до replay-grade exact core

### Цель
Сделать ядро, на котором можно без стыда строить popup, filters и stats.

### Сделать
1. Полный replay state machine.
2. Exact pot tree:
   - main pot;
   - side pots;
   - returns;
   - winners;
   - winner shares.
3. Persist exact pot layer во всех покрытых кейсах.
4. Ввести расширенный synthetic edge-pack:
   - split KO;
   - side-pot KO;
   - repeated collect;
   - multiple winners;
   - uncalled return;
   - short stack side-pot bust;
   - summary/showdown oddities.
5. Зафиксировать invariants как жёсткое правило.
6. Отдельно документировать uncertainty model для ambiguous mappings.

### Gate
См. `G3`.

### Статус
Закрыто для current exact-core scope:
- replay ledger остаётся source-of-truth для pots/contributions;
- ambiguous winner mappings больше не materialize-ят guessed `hand_pot_winners`;
- unsatisfied mappings уходят в `invariant_errors`;
- committed pack и synthetic edge-pack держат invariants зелёными.

---

## Фаза 5. Исправить MBR-specific слой

### Цель
Сделать корректный MBR stage/KO/economics слой поверх exact core.

### Сделать

### 5.1 Stage model
Разделить и хранить отдельно:
- `played_ft_hand` — exact;
- `entered_boundary_zone` — estimated/uncertain;
- `ft_table_size` — exact observed seated players on 9-max hand;
- `boundary_ko_min`;
- `boundary_ko_ev`;
- `boundary_ko_max`;
- `boundary_ko_method`;
- `boundary_ko_certainty`.

### 5.2 Tournament economics
Импортировать и рассчитывать:
- `regular_prize_money`;
- `mystery_money_total`;
- `total_payout_money`;
- `buyin` components from reference tables.

### 5.3 Big KO
Убрать greedy decomposition.
Сделать posterior / DP decoder по:
- official envelope distribution;
- total mystery money;
- split factors;
- KO event count.

### Gate
См. `G5`.

### Статус
Foundation-layer уже реализован в текущем snapshot; phase не является следующим блокером.

---

## Фаза 6. Довести street hand strength до v2

### Цель
Сделать hand-strength слой пригодным для будущих фильтров и статов.

### Сделать
1. Проверить и дополнить current descriptor set.
2. Завершить:
   - `is_nut_hand`;
   - `is_nut_draw`;
   - missed-draw semantics;
   - pair+draw / combo-draw / overcards / air rules.
3. Явно зафиксировать, какие descriptors exact для Hero, а какие только для showdown-known opponents.
4. Добавить fixtures под реальные фильтры будущего popup.

### Gate
См. `G4`.

---

## Фаза 7. Построить feature/stat engine

### Цель
Перейти от hand-coded статов к нормальному движку признаков, фильтров и формул.

### Сделать
1. `feature_registry`
2. `stat_registry`
3. AST filters
4. AST expressions
5. planner
6. executor
7. exact/estimated/fun policy
8. selective materialization hot-features

### Принцип
Source of truth — canonical + normalized + derived data.
Materialized features — только ускорение, не основной источник истины.

### Gate
См. `G6`.

---

## Фаза 8. Перенести весь каталог MBR Stats

### Цель
Перевести все 31 legacy stat-модуля на новое ядро.

### Сделать
1. Для каждого stat из inventory зафиксировать:
   - final formula;
   - dependencies;
   - exactness class;
   - blockers;
   - acceptance fixture.
2. Переносить статы не Python-классами, а через registry/AST.
3. Для каждого stat сделать одну из категорий:
   - `exact`;
   - `estimated`;
   - `expression`;
   - `fun`.
4. Для спорных stat-ов хранить раздельно exact и estimated компоненты.

### Gate
См. `G7`.

---

## Фаза 9. Сделать web backend для школы

### Цель
Построить реальную систему доступа и работы с данными.

### Сделать
1. auth-flow;
2. invite flow;
3. sessions;
4. role model;
5. RLS policy design;
6. upload/import API;
7. stats/report API;
8. parse issues API;
9. coach aggregate API.

### Gate
См. `G8`.

---

## Фаза 10. Подключить frontend к живому backend

### Цель
Убрать mocks из основных пользовательских сценариев.

### Сделать
1. убрать mock upload;
2. убрать mock stats;
3. подключить auth;
4. подключить progress/import lifecycle;
5. сделать student views;
6. сделать coach views;
7. сделать parse issue drilldown.

### Gate
См. `G9`.

---

## Фаза 11. Реализовать большой popup / HUD

### Цель
Построить popup-слой поверх stat/query engine, а не костылями.

### Сделать
1. popup stat catalog;
2. display groups;
3. drilldown filters;
4. expression stats;
5. performance tuning;
6. selective precomputation where profiling proves it is needed.

### Gate
См. `G10`.

---

## Строгий запрет на неправильный порядок

Нельзя:
- переносить popup раньше stat engine;
- переносить массу legacy stat-ов раньше corrected MBR layer;
- строить UI на живые данные раньше backend visibility model;
- считать проект “уже почти продуктом” по одному clean fixture-pack.
