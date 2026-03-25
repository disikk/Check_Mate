# Статус разработки и оценка корректности

## 1. Что было перепроверено по этому архиву

Лично перепроверено:

1. SQL-схема, миграции, seed, Rust-crates, runtime layer и frontend foundation.
2. Committed GG fixture-pack:
   - `9` tournament summary файлов;
   - `9` hand history файлов;
   - `321` рука суммарно.
3. Синтаксическое покрытие committed GG fixture-pack:
   - на этом паке текущие parser-паттерны закрывают все строки без неожиданных `unparsed_line`.
4. Инвентаризация legacy каталога:
   - в `docs/stat_catalog/mbr_stats_inventory.yml` реально описано `31` stat-модуль.
5. Frontend build:
   - `npm run build` проходит на Node 22.
6. Runtime-перепроверка после exact-core hardening:
   - `bash backend/scripts/run_backend_checks.sh`
   - `cargo test -p tracker_parser_core parses_all_committed_tournament_summary_fixtures`
   - `cargo test -p tracker_parser_core parses_all_committed_hand_history_fixtures_without_unexpected_warnings`
   - `cargo test -p tracker_parser_core --test positions -- --nocapture`
   - `cargo test -p tracker_parser_core --test phase0_exact_core_corpus -- --nocapture`
   - `cargo test -p tracker_parser_core keeps_full_pack_invariants_green_for_all_committed_hands`
   - `cargo test -p parser_worker local_import::tests::import_local_full_pack_smoke_is_clean -- --ignored --exact`
   - `cargo test -p parser_worker local_import::tests::import_local_persists_cm06_joint_ko_fields_to_postgres -- --ignored --exact`

Что **всё ещё не доказано** runtime-исполнением:
- extended real GG corpus за пределами committed `9 HH + 9 TS` пакета;
- web/API ingest path;
- production-like многопользовательский runtime.

---

## 2. Короткий вердикт

Проект находится на стадии **foundation / narrow alpha**.

Это уже не набор идей и не пустой макет. Но это всё ещё:
- не полноценный трекер;
- не production backend;
- не web-система для школы;
- не готовая реализация полного MBR Stats каталога.

Главное: вектор архитектуры правильный. Главный риск сейчас — начать ускорять UI, popup и новые статы раньше, чем будет доведено exact data core.

### Update 2026-03-24

После exact-core hardening закрыты несколько прежних P0/P1 хвостов:
- schema v2 hardening больше не долг: добавлены `core.player_aliases`, `import.source_file_members`, `import.job_attempts`, analytics catalogs и composite FK contract;
- GG timestamps больше не “просто теряются”: canonical UTC поля всё ещё `NULL`, но raw/local/provenance contract уже persisted и зафиксирован;
- parser committed-pack coverage теперь подкреплён syntax catalog (`docs/COMMITTED_PACK_SYNTAX_CATALOG.md`) и structured severity model для `core.parse_issues`;
- normalizer больше не materialize-ит guessed `hand_pot_winners` для ambiguous mappings: такие кейсы остаются `uncertain`, exact-only downstream facts их игнорируют;
- PostgreSQL full-pack smoke на committed corpus теперь подтверждает `0` unexpected parse issues, `0` invariant mismatches и idempotent re-import.

Из этого следует:
- exact-core для committed GG pack заметно сильнее, чем в исходной оценке ниже;
- но общая продуктовая стадия проекта всё ещё остаётся foundation / narrow alpha, потому что extended corpus, web ingest и stat engine пока не завершены.

### Update 2026-03-25

Phase 0 exact-core hardening из аудита теперь закрывает не только committed-pack parser slice, но и весь current exact-core contract:
- summary seat-result lines больше не теряются;
- position engine, legality layer и forced-all-in / sit-out surface доказаны отдельными focused tests;
- deterministic pot settlement больше не сводится к reverse guess по aggregated collect totals;
- KO attribution v2 теперь может описывать multi-pot joint bust и это проверяется как в core, так и через persisted PostgreSQL round-trip;
- synthetic edge matrix теперь committed как отдельный HH fixture и проверяет reason-coded explicit warnings / uncertainty contract на dead blind, no-show, heads-up order, short all-in, side-pot, odd-chip и joint-KO кейсах;
- backend gate теперь явно запускает exact-core proof suite, а не полагается только на общий `cargo test`.
- финальная serial-проверка `bash backend/scripts/run_backend_checks.sh` прошла целиком, включая committed-pack proof suite и ignored PostgreSQL smoke-тесты.

Из этого следует:
- для текущего committed `9 HH + 9 TS` pack + synthetic matrix exact-core уже replay-grade в пределах зафиксированного scope;
- главный незакрытый риск смещается с “алгоритм пока слишком эвристический” на “недостаточно широк общий реальный corpus за пределами committed pack”.

---

## 3. Оценка стадии по слоям

| Слой | Стадия | Готовность | Корректность |
|---|---:|---:|---|
| Система целиком | foundation / narrow alpha | 20–25% | частично корректна |
| Репозиторий и воспроизводимость | ранняя | 20–25% | пока слабая |
| PostgreSQL source-of-truth foundation | средняя | 45–50% | хорошая как foundation |
| GG parser на committed pack | хороший узкий alpha | 55–60% | высокая на committed pack |
| GG parser как production parser | ранняя | 30–35% | ещё не доказана |
| Normalizer | хороший узкий alpha | 45–50% | replay-grade в текущем exact-core scope, но ещё не доказан на широком реальном corpus |
| MBR stage / KO derived-слой | foundation-only | 15–20% | пока частично корректен |
| Street hand strength | foundation | 35–40% | частично корректен |
| Runtime stat-layer | seed-safe slice | 10–15% | только для очень узкого набора |
| Web/API слой | отсутствует как продуктовый | 5–10% | не готов |
| Frontend | mock prototype | 10% | UI только для прототипирования |

Важно: parser на committed pack и parser как общий реальный parser — это **две разные оценки**.

---

## 4. Что уже сделано хорошо

### 4.1 Архитектурные решения

1. Правильно принято решение строить новый source-of-truth контур вместо дальнейшего наращивания старого `MBR Stats`.
2. Правильно разделены уровни:
   - canonical parse;
   - normalization;
   - derived facts;
   - analytics/runtime.
3. Правильно заложен multi-tenant фундамент:
   - `auth.users`;
   - `org.organizations`;
   - memberships;
   - study groups;
   - player profiles.
4. Правильно выбран отдельный Rust-контур `tracker_parser_core + parser_worker`.

### 4.2 Что выглядит корректным уже сейчас

1. `played_ft_hand` отделён как exact-факт по `9-max` header.
2. `hero_involved` в `derived.hand_eliminations` трактуется правильно: Hero реально получает долю KO только если он winner релевантного pot.
3. `split_n`, `hero_share_fraction`, `is_split_ko`, `is_sidepot_based` уже есть и это правильно.
4. Normalizer уже умеет:
   - final pots;
   - pot contributions;
   - pot winners;
   - uncalled returns;
   - chip/pot invariants;
   - exact elimination по итоговому стеку.
5. Street hand strength уже не просто “план”, а реально существующий persisted слой.
6. Frontend production build проходит.

---

## 5. Что сейчас частично корректно, но ещё не завершено

### 5.1 Parser

На committed GG pack parser выглядит хорошо:
- `321/321` рук синтаксически укладываются в текущий parser-layer без неожиданных warnings;
- `9/9` TS-файлов соответствуют текущей модели.

Но production-ready вывод делать рано, потому что:
- committed pack маленький и однотипный;
- это один room, один формат, один buy-in, один тип выгрузки;
- не доказано покрытие редких вариантов GG text-format.

### 5.2 Normalizer

Сильные стороны:
- logic опирается на replay-подобное применение action events;
- positions, legality, forced all-in surface и deterministic pot settlement теперь покрыты отдельными proof-tests;
- repeated collect, split/side-pot, odd chip и joint-KO кейсы учтены;
- ambiguous / hidden showdown cases больше не materialize-ят guessed winners, а surface-ятся через `uncertain_reason_codes` и `certainty_state`.

Что ещё не доказано полностью:
- текущий committed pack + synthetic matrix уже достаточны для Phase 0 gate, но production-ready вывод на широком GG corpus пока делать рано;
- acceptance следующей фазы надо доказывать расширенным real corpus за пределами committed `9 HH + 9 TS`.

### 5.3 Street hand strength

Хорошо:
- Hero и showdown-known opponents уже описываются по улицам;
- есть `best_hand_class`, `pair_strength`, draw flags, `has_air`, `has_missed_draw_by_river`.

Не завершено:
- `is_nut_hand` и `is_nut_draw` пока `NULL`;
- coverage ещё не доказан на широком корпусе;
- это пока foundation-level descriptor layer, а не финальный stat/filter substrate.

---

## 6. Критические проблемы и недоделки

### P0 — нужно исправлять в первую очередь

#### 1. Репозиторий и архив пока не в чистом, воспроизводимом состоянии

Факты:
- в архив попали `dist/`, `backend/target/`, `__MACOSX/`, `.DS_Store`;
- `.env.example` отсутствовал;
- `backend/README.md` ссылался на `.github/workflows/backend-foundation.yml`, которого в архиве нет.

Вывод:
- репозиторий пока не проходит гигиенический production/foundation quality gate.

#### 2. Время рук и турниров фактически не сохраняется как рабочее аналитическое поле

Факты:
- `TournamentSummary.started_at` парсится, но в `core.tournaments.started_at` сейчас уходит `NULL`;
- `HandHeader.played_at` парсится, но `core.hands.hand_started_at` тоже остаётся `NULL`.

Последствия:
- session/date filters пока невозможны;
- order-by-time в БД пока нельзя считать надёжным source-of-truth.

#### 3. MBR stage/boundary модель пока placeholder

Факты:
- hard-coded эвристика “последняя 5-max рука перед первой 9-max рукой” уже убрана;
- boundary resolver теперь использует ordered timeline и умеет честно оставлять boundary `uncertain`, если несколько equally-late non-FT candidates одинаково допустимы;
- `derived.mbr_stage_resolution` уже хранит `boundary_resolution_state`, `boundary_candidate_count`, `boundary_resolution_method`, `boundary_confidence_class`;
- `boundary_ko_ev / min / max` больше не должны имитировать exact point estimate при non-exact boundary;
- `derived.mbr_tournament_ft_helper` теперь materialize-ится на tournament grain и честно стабилизирует `reached_ft_exact`, `first_ft_hand_id`, incomplete FT и FT-entry stack semantics без guessed defaults;
- boundary/stage logic всё ещё не реализует формальную predicate-family модель и probabilistic KO-money слой.

Вывод:
- MBR-specific derived-слой стал заметно честнее и безопаснее, но formal stage predicates и финальная probabilistic/economic модель всё ещё остаются незавершёнными.

#### 4. Tournament economics не доведён

Факты:
- `total_payout_money`, `regular_prize_money` и `mystery_money_total` уже materialize-ятся в `core.tournament_entries`;
- TS parser больше не опирается на жёсткие первые 6 строк и поднимает tail `You finished ...` / `You received ...` как structured confirmation layer;
- primary source для finish/payout остаётся `result line`, а tail-конфликты surface-ятся как reason-coded warning parse issues вместо silent reconciliation;
- missing buy-in config и negative mystery remainder по-прежнему считаются explicit import errors, а не guessed fallback.

Последствия:
- tournament-level ITM / payout / mystery-money foundation уже есть, но probabilistic boundary / KO-money model и downstream stat migration всё ещё не завершены.

#### 5. В БД не хватает части целостностных ограничений

Сейчас как foundation это терпимо, но для production ядра не хватает:
- composite FK из `core.hand_hole_cards` в `(hand_id, seat_no)` таблицы мест;
- composite FK из `core.hand_showdowns` в `(hand_id, seat_no)`;
- composite FK из `core.hand_returns` в `(hand_id, seat_no)`;
- composite FK из `core.hand_pot_contributions` и `core.hand_pot_winners` в `(hand_id, pot_no)`;
- нормальной reference-схемы для feature/stat catalog.

#### 6. `import-local` — dev-only importer, а не foundation для школы

Факты:
- жёстко прошиты `Check Mate Dev Org`, `mbr-dev-student@example.com`, `Hero`, `gg`;
- нет object storage;
- нет archive-member model;
- нет queue/job retry policy;
- нет API-контракта.

Вывод:
- current importer полезен для локальной проверки ядра, но его нельзя принимать за основу готовой ingest-системы школы.

### P1 — важные недоделки второго приоритета

#### 7. `source_files` и `import_jobs` пока не дедуплицируются как production ingest

Повторный HH import не плодит duplicate hand-children, это хорошо.
Но новые `source_files`, `import_jobs`, `file_fragments` всё равно создаются. Для настоящей системы нужно:
- дедуп по sha256 и owner/player scope;
- archive-members;
- retry model.

#### 8. Runtime-слой пока слишком узкий

Факты:
- `mbr_stats_runtime` сейчас materialize-ит только `9` hand-features;
- seed stats — очень узкий безопасный срез.

Вывод:
- это foundation-only runtime, не stat engine.

#### 9. `hero_exact_ko_count` пока считает KO-события, а не KO-share

В runtime-слое сейчас exact KO агрегируется как `hero_involved = true`. Это годится как временный event-count, но не как финальная семантика для всех stat-ов.

#### 10. Street strength ещё не стал базой фильтров

Сейчас слой есть, но ещё нет:
- feature registry для него;
- AST filter integration;
- stats поверх hand-strength features.

### P2 — ожидаемые продуктовые пробелы

- нет auth-flow;
- нет RLS;
- нет coach/student/super-admin runtime visibility;
- frontend целиком сидит на моках;
- нет report API;
- нет popup/HUD.

---

## 7. Оценка корректности по компонентам

### БД

**Оценка:** хорошая foundation, но ещё не production schema.

Что хорошо:
- правильное разбиение на схемы;
- хорошая стартовая модель для школы;
- уже есть canonical/derived/analytics separation.

Что ещё обязательно добавить:
- alias/history model;
- archive members;
- reference tables для MBR economics;
- feature/stat catalogs;
- composite FK и dedupe policy.

### Parser

**Оценка:** высокая корректность на committed pack, средняя уверенность вне него.

### Normalizer

**Оценка:** хороший узкий alpha. На покрытых кейсах выглядит сильно лучше старых приложений, но до exact-core для будущего pop-up ещё не дотянут.

### MBR stage / KO

**Оценка:** foundation-only. Правильная семантическая развязка сделана, но алгоритмический слой ещё не завершён.

### Runtime stats

**Оценка:** только seed-safe foundation, а не движок статов.

### Frontend

**Оценка:** UI prototype, не продуктовый слой.

---

## 8. Итоговый вывод

Сейчас проект правильно воспринимать так:

- **архитектурно**: направление верное;
- **инженерно**: foundation уже есть;
- **алгоритмически**: exact-core ещё не доведён;
- **продуктово**: до системы для школы ещё далеко.

Самый опасный путь сейчас — переносить popup и массу статов поверх ещё незавершённого exact-core.

Правильный путь:
1. воспроизводимость и schema hardening;
2. parser coverage;
3. replay-grade normalizer;
4. corrected MBR stage/economics;
5. перенос 31 MBR stat-модуля;
6. stat/filter engine;
7. web system.
