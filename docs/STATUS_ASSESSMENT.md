# Статус разработки и оценка корректности

## Короткий вердикт

Текущее состояние проекта — **foundation / narrow alpha**.

Это уже не набор идей и не просто макеты. База, parser-core, нормализатор и derived-слой уже существуют и частично работают. Но это всё ещё **не полноценный трекер** и **не готовая система для школы**.

## Оценка стадии

### Система целиком

- готовность: **20–25%**
- стадия: **foundation + узкий альфа-срез GG MBR импорта**

### PostgreSQL source-of-truth

- готовность: **45–50%**
- оценка: **хорошо**

Плюсы:
- правильное разделение на схемы;
- хороший фундамент под multi-tenant модель;
- есть canonical / derived / analytics слои.

Минусы:
- ещё нет полноценного API-контура;
- не хватает некоторых catalog/reference сущностей для будущего stat engine;
- RLS ещё не включён.

### GG MBR parser

- готовность: **40–45%**
- корректность: **частично корректен, coverage ещё не полный**

Что хорошо:
- parser уже покрывает базовые HH/TS сценарии;
- есть fixture-based tests;
- summary fields и ряд ранее проблемных строк уже поддержаны.

Что не доказано:
- полное покрытие формата GG MBR на широком реальном паке;
- все редкие варианты summary/showdown/collect.

### Exact normalizer

- готовность: **30–35%**
- корректность: **хорошая в покрытых тестами кейсах, но ещё не replay-grade полностью**

Что уже хорошо:
- committed totals;
- final pots;
- pot contributions;
- pot winners;
- returns;
- split KO;
- side-pot KO;
- invariants.

Что ещё не завершено:
- полноценный replay engine для всего формата;
- полный exact hand strength layer;
- полная уверенность на большом реальном корпусе.

### MBR stage / KO derived-слой

- готовность: **20–25%**
- корректность: **частично корректен**

Что уже хорошо:
- `played_ft_hand` хранится отдельно как exact-факт;
- `hero_involved` в eliminations опирается на pot winners, а не на шумную эвристику;
- split / side-pot признаки пишутся.

Что пока плохо:
- `build_mbr_stage_resolutions()` сейчас всё ещё слишком упрощён;
- boundary-candidate определяется грубо как "последняя 5-max рука перед первой 9-max";
- boundary KO EV-алгоритм ещё не внедрён.

### Runtime-слой статов

- готовность: **10–15%**
- корректность: **только seed-safe subset**

Что есть:
- feature materialization;
- несколько seed-safe runtime признаков;
- простые агрегаты.

Чего нет:
- перенос всего каталога MBR Stats;
- точное разделение all exact/estimated/fun stat groups;
- AST/filter engine;
- popup/stat registry.

### Frontend

- готовность: **10%**
- статус: **mock/prototype only**

Что есть:
- базовый layout;
- student dashboard skeleton;
- upload page mock;
- FT analytics mock.

Чего нет:
- auth;
- реальный API;
- real upload/import lifecycle;
- coach view;
- aggregated groups;
- drilldown и popup.

## Итоговая оценка корректности

### Что уже можно считать хорошим фундаментом

- SQL architecture
- import source-of-truth idea
- parser/normalizer split
- exact elimination concept
- feature materialization idea

### Что пока нельзя считать окончательно корректным

- весь GG parser на полном реальном формате
- весь normalizer на полном наборе edge-cases
- MBR boundary-stage logic
- весь каталог MBR Stats
- любой popup/HUD/stat engine выше seed-safe слоя

## Главный вывод

Сейчас проект готов не к релизу, а к **интенсивной инженерной фазе**:

1. сначала exact core;
2. затем corrected MBR stats;
3. затем query/stat engine;
4. затем web/system layer.

Попытка ускорить UI или popup раньше exact core почти гарантированно создаст слой костылей.
