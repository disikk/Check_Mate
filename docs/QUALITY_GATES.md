# Quality gates

Этот документ нужен как жёсткий набор условий, после которых можно переходить к следующей фазе. Нельзя считать этап завершённым без прохождения его gate.

## G0. Репозиторий и воспроизводимость

Обязательно:
- в репозитории нет `dist/`, `backend/target/`, `__MACOSX/`, `.DS_Store`;
- есть `.env.example`;
- есть root `.gitignore`;
- clean clone + runbook поднимают проект без ручной магии;
- committed fixtures доступны сразу после clone.

Pass condition:
- новый разработчик за один проход выполняет bootstrap без ручных исправлений.

## G1. Схема БД v2

Обязательно:
- добавлены таблицы/каталоги для alias, archive members, ref economics, feature/stat catalog;
- добавлены raw/local/utc time поля;
- введены dedupe/uniqueness constraints;
- добавлены composite FK для seat/pot child tables.

Pass condition:
- миграции применяются на пустую БД и на обновление существующей dev БД без ручной правки.

Текущий статус:
- PASS на `2026-03-24`.

## G2. Parser coverage для GG MBR

Обязательно:
- зафиксирован syntax catalog;
- на каждый реальный syntax pattern есть fixture test;
- summary seat-result lines, collect variants и hidden-info variants классифицированы;
- unexpected line не пропадает молча;
- parse issues имеют structured severity model.

Pass condition:
- на committed pack `warning-level parse issues = 0`;
- на extended pack все неожиданные строки либо покрыты, либо явно whitelisted.

Текущий статус:
- PASS для committed `9 HH + 9 TS` GG pack на `2026-03-24`.

## G3. Replay-grade normalizer

Обязательно:
- есть полноценный replay state machine;
- exact pot tree строится детерминированно;
- persisted `hand_pots`, `hand_pot_contributions`, `hand_pot_winners`, `hand_returns` согласованы;
- elimination attribution проходит через pot resolution;
- invariants обязательны;
- ambiguous winner mappings не materialize-ят guessed exact winners.

Pass condition:
- на committed pack и synthetic edge-pack `chip_conservation_ok = true`, `pot_conservation_ok = true`, `invariant_errors = []`.

Текущий статус:
- PASS для current committed-pack + synthetic edge-pack scope на `2026-03-24`.

## G4. Street hand strength v2

Обязательно:
- Hero descriptors корректны по flop/turn/river;
- есть `air`, overcards, draw classes, pair strength;
- реализованы `is_nut_hand` и `is_nut_draw` либо явно отложены с documented reason.

Pass condition:
- hand-strength fixtures покрывают все ключевые future filter cases.

## G5. MBR stage / boundary / economics

Обязательно:
- `played_ft_hand` остаётся exact-фактом;
- boundary candidate/EV logic реализована отдельно;
- `boundary_ko_min / ev / max` реально пишутся;
- `regular_prize_money` и `mystery_money_total` реально пишутся;
- big-KO greedy больше не используется.

Pass condition:
- stage/economics слой даёт данные, достаточные для переноса MBR Stats без legacy эвристик старого приложения.

Текущий статус:
- foundation PASS на `2026-03-24`, но full stat-layer handoff всё ещё зависит от следующих фаз.

## G6. Feature/stat engine v1

Обязательно:
- feature registry;
- stat registry;
- AST filters;
- AST formulas;
- planner/executor;
- policy exact/estimated/fun.

Pass condition:
- новый stat добавляется через каталог, а не через новый hand-coded класс.

## G7. Перенос MBR Stats

Обязательно:
- все 31 legacy module перенесены;
- каждый stat имеет классификацию `exact / estimated / expression / fun`;
- legacy assumptions не протаскиваются молча.

Pass condition:
- весь каталог MBR Stats доступен из нового ядра и имеет documented formula + dependencies.

## G8. Web product foundation

Обязательно:
- auth;
- roles;
- visibility model;
- upload/import job API;
- parse issues API;
- stats/report API.

Pass condition:
- ученик видит только своё, тренер видит свою группу/организацию, супер-админ видит всё.

## G9. Frontend integration

Обязательно:
- нет зависимости от mock data для core user flows;
- upload, progress, stats, parse issues работают на живом API.

Pass condition:
- frontend можно использовать для реального smoke test учеником и тренером.

## G10. Popup/HUD

Обязательно:
- popup catalog;
- drilldown filters;
- expression stats;
- performance tuning по реальным query patterns.

Pass condition:
- значимая часть pop-up собирается из единого stat/query engine, а не из отдельных кастомных endpoint-ов.
