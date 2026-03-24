# Чёткий план исправления и продолжения разработки

## Принцип

Порядок работ фиксированный:

1. reproducible environment
2. full GG parser coverage
3. replay-grade normalizer
4. street hand strength
5. corrected MBR stage/KO layer
6. перенос MBR Stats
7. stat/filter AST engine
8. web backend
9. frontend integration
10. popup/HUD

Нельзя перепрыгивать через этапы.

---

## Фаза 1. Привести среду к воспроизводимому виду

### Задачи

1. Удалить из репозитория лишние артефакты сборки.
2. Держать golden fixtures внутри проекта или в официальном приватном storage с bootstrap-script.
3. Добиться сценария:
   - clone
   - db up
   - migrations
   - cargo test
   - fixture import
4. Добавить CI-пайплайн, который проверяет:
   - migrations
   - parser tests
   - normalizer tests
   - import-local tests

### Критерий завершения

Новый разработчик без ручной магии поднимает проект с нуля по runbook.

---

## Фаза 2. Закрыть parser coverage для GG MBR

### Задачи

1. Собрать расширенный golden pack реальных GG HH/TS.
2. Зафиксировать каталог всех syntax patterns.
3. На каждый pattern сделать fixture test.
4. Любая неизвестная строка должна либо:
   - быть осознанно whitelisted как info;
   - либо падать в parse issue.
5. Проверить все формы:
   - hidden deal lines;
   - collect lines;
   - summary lines;
   - showdown variants;
   - uncalled returns;
   - split pots;
   - repeated collect.

### Критерий завершения

На golden pack нет непонятных warning-level строк, кроме специально whitelisted.

---

## Фаза 3. Довести normalizer до replay-grade

### Задачи

1. Построить полноценный replay state machine.
2. Проверить exact committed / returns / side pots / winners.
3. Заполнить exact pot layer во всех покрытиях.
4. Расширить synthetic edge-cases:
   - split KO
   - side-pot KO
   - multiple winners
   - uncalled return
   - short stack bust in side pot
   - repeated collect
   - no-show/muck если встретятся
5. Зафиксировать инварианты:
   - chip conservation
   - pot conservation
   - pot-winner coverage

### Критерий завершения

На golden pack и synthetic edge-cases нет несходимостей по стеку и банкам.

---

## Фаза 4. Реализовать street hand strength

### Задачи

1. Сделать evaluator для Hero на flop / turn / river.
2. Считать:
   - best_hand_class
   - pair_strength
   - flush draw
   - backdoor flush draw
   - open-ended
   - gutshot
   - double gutshot
   - pair+draw
   - overcards
   - air
   - missed_draw_by_river
3. Писать результаты в `derived.street_hand_strength`.
4. Сделать fixture tests под эти признаки.

### Критерий завершения

Можно выражать фильтры типа:
- чек две улицы с air
- river jam с missed draw
- flop continue с pair+draw

---

## Фаза 5. Исправить MBR stage / KO модель

### Задачи

1. Оставить `played_ft_hand` как exact-факт.
2. Внедрить final boundary-KO EV формулу.
3. Разделить:
   - `ko_exact`
   - `ko_boundary_ev`
   - `ko_total_adj`
4. Хранить:
   - `boundary_ko_min`
   - `boundary_ko_ev`
   - `boundary_ko_max`
   - `boundary_ko_method`
   - `boundary_ko_certainty`
5. Никаких int-округлений в базовых KO-агрегатах.
6. Для eliminations хранить и использовать:
   - `hero_involved`
   - `hero_share_fraction`
   - `split_n`
   - `is_sidepot_based`

### Критерий завершения

Boundary-логика формализована и не смешивается с exact FT logic.

---

## Фаза 6. Перенести каталог MBR Stats

### Задачи

1. Инвентаризировать все старые статы.
2. Каждый стат классифицировать:
   - exact
   - estimated
   - expression
   - fun
3. Перенести статы на новый source-of-truth слой.
4. Явно разделить exact и estimated версии там, где это нужно.
5. Убрать legacy assumptions старого `MBR Stats`.

### Критерий завершения

Все статы из каталога доступны из нового ядра и имеют корректную классификацию.

---

## Фаза 7. Построить AST/filter/stat engine

### Задачи

1. Ввести feature registry.
2. Ввести stat registry.
3. Реализовать AST для фильтров.
4. Реализовать AST для выражений/stat formulas.
5. Реализовать planner/executor.
6. Отдельно определить policy materialization hot-features.

### Критерий завершения

Новые статы и popup-блоки добавляются через каталоги и AST, а не hand-coded классами.

---

## Фаза 8. Сделать web backend для школы

### Задачи

1. Auth:
   - invite flow
   - password setup
   - session strategy
2. Role model:
   - student
   - coach
   - org_admin
   - super_admin
3. Visibility model:
   - student sees own data
   - coach sees own groups/org scope
   - super_admin sees all
4. Upload API
5. Import job API
6. Stats/report API
7. Parse issues API

### Критерий завершения

Ученики и тренеры реально работают в одной системе с разделённым доступом.

---

## Фаза 9. Подключить frontend к живому API

### Задачи

1. Убрать mock data.
2. Подключить auth.
3. Подключить real upload flow.
4. Подключить dashboard/stat views.
5. Сделать coach aggregate pages.
6. Сделать parse issue drilldown.

### Критерий завершения

Frontend больше не зависит от моков и работает на живой БД.

---

## Фаза 10. Реализовать giant popup / HUD layer

### Задачи

1. Собрать popup stat catalog.
2. Реализовать display groups.
3. Реализовать drilldown filters.
4. Сделать expression stats.
5. Сделать performance tuning по реальным query patterns.

### Критерий завершения

Большая часть поп-апа строится через единый stat/query engine.
