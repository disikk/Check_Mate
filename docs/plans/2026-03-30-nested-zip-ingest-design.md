# Nested ZIP Ingest Design

## Problem

Текущий ingest-контур умеет работать только с одним уровнем ZIP. Если внутри архива лежат HH/TS файлы, они импортируются, но если внутри архива лежит другой ZIP, он не раскрывается. Из-за этого валидные HH/TS во вложенных архивах теряются, а часть невалидных бинарных источников может доходить до поздних стадий импорта и ломать прогон.

## Goal

Сделать ZIP полноценным рекурсивным источником данных во всём общем ingest-контуре: `prepare`, runtime и downstream CLI/web upload должны одинаково находить валидные HH/TS во вложенных ZIP, а невалидные leaf-файлы и битые вложенные архивы должны отбрасываться без падения всего импорта.

## Chosen approach

- Храним nested member locator в существующем `member_path` без изменений схемы БД.
- В коде работаем с typed locator как с цепочкой сегментов `Vec<String>`, а в `member_path` сериализуем его в стабильный строковый формат.
- `tracker_ingest_prepare` рекурсивно обходит ZIP и nested ZIP, но в кандидаты выдаёт только leaf-файлы.
- `parser_worker` читает архивный источник через тот же locator и умеет дойти до leaf-файла через несколько ZIP-уровней.
- Битые inner ZIP и невалидные leaf-файлы превращаются в `unsupported_source`, а не в abort всего scan/import.

## Locator contract

- В памяти locator хранится как ordered chain of archive members.
- Сериализация в `member_path`: сегменты соединяются через `!/`.
- Для однозначности внутри каждого сегмента экранируются `%` и `!`.
- Примеры:
  - direct member: `nested/one.hh.txt`
  - nested member: `packs/day1.zip!/tables/one.hh.txt`

## Validation policy

- Если leaf не является HH/TS, он отбрасывается как `unsupported_source`.
- Если вложенный ZIP повреждён или не читается как ZIP, он тоже отбрасывается как `unsupported_source`.
- Если runtime при чтении leaf получает текст с `\0`, такой источник считается невалидным и завершается как terminal file failure, не ломая весь bundle.

## Test strategy

- `tracker_ingest_prepare`: nested ZIP с валидной `TS+HH` парой.
- `tracker_ingest_prepare`: nested ZIP с невалидным leaf и/или битым inner ZIP, scan не падает.
- `parser_worker`: чтение nested archive member через `load_ingest_job_input()`.
- После green: focused tests, затем полный `bash backend/scripts/run_backend_checks.sh`, затем реальный импорт `KONFA/Aleshka/15k hands`.
