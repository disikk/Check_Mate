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
   - `cargo test -p tracker_parser_core parses_all_committed_tournament_summary_fixtures`
   - `cargo test -p tracker_parser_core parses_all_committed_hand_history_fixtures_without_unexpected_warnings`
   - `cargo test -p tracker_parser_core keeps_full_pack_invariants_green_for_all_committed_hands`
   - `cargo test -p parser_worker import_local_full_pack_smoke_is_clean_and_idempotent -- --ignored`

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
- PostgreSQL full-pack smoke на committed corpus теперь подтверждает `0` parse issues, `0` invariant mismatches и idempotent re-import.

Из этого следует:
- exact-core для committed GG pack заметно сильнее, чем в исходной оценке ниже;
- но общая продуктовая стадия проекта всё ещё остаётся foundation / narrow alpha, потому что extended corpus, web ingest и stat engine пока не завершены.

---

## 3. Оценка стадии по слоям

| Слой | Стадия | Готовность | Корректность |
|---|---:|---:|---|
| Система целиком | foundation / narrow alpha | 20–25% | частично корректна |
| Репозиторий и воспроизводимость | ранняя | 20–25% | пока слабая |
| PostgreSQL source-of-truth foundation | средняя | 45–50% | хорошая как foundation |
| GG parser на committed pack | хороший узкий alpha | 55–60% | высокая на committed pack |
| GG parser как production parser | ранняя | 30–35% | ещё не доказана |
| Normalizer | узкий alpha | 30–35% | хорош в покрытых кейсах, но не replay-grade |
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
- logic уже опирается на replay-подобное применение action events;
- repeated collect и split/side-pot кейсы учтены;
- есть `certainty_state`.

Но до replay-grade exact core ещё не дотягивает, потому что:
- часть mapping pot winners выводится через inference по aggregated `collected_amounts`;
- на общих кейсах это допустимо как foundation, но не даёт ещё полной уверенности для широкого production корпуса;
- acceptance надо доказывать большим golden pack и расширенным synthetic набором.

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
- `build_mbr_stage_resolutions()` определяет boundary candidate как “последняя 5-max рука перед первой 9-max рукой”;
- `boundary_ko_ev / min / max` в схеме существуют, но importer их не заполняет;
- boundary logic пока не реализует финальную вероятностную модель.

Вывод:
- MBR-specific derived-слой пока нельзя считать завершённым и точным.

#### 4. Tournament economics не доведён

Факты:
- `total_payout_money` импортируется;
- `regular_prize_money` и `mystery_money_total` пока не рассчитываются и не пишутся.

Последствия:
- `winnings_from_itm`, `winnings_from_ko`, `roi_regular`, `roi_bounty`, `big_ko` пока не на чем корректно строить.

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
