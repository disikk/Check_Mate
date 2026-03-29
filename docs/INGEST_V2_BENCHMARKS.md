# Ingest V2 Benchmarks

Этот документ фиксирует baseline для фазы 0 плана `ingest_v2_rearchitecture`.

## Цели фазы 0

- отделить happy-path benchmark от грязного mixed corpus;
- получить воспроизводимый baseline до перехода на новый `dir-import` контракт;
- сохранить сравнимость с текущим runtime, пока pair-first prepare-layer ещё не внедрён.

## Сценарий 1: happy-path paired corpus

Для baseline используется committed GG MBR pack:

- HH: `backend/fixtures/mbr/hh/GG20260316-*.txt`
- TS: `backend/fixtures/mbr/ts/GG20260316 - Tournament #*.txt`

Harness:

```bash
bash scripts/run_ingest_v2_bench.sh <player-profile-id>
```

Что делает скрипт:

1. собирает committed HH/TS happy-path corpus;
2. валидирует, что корпус полностью спарен по `tournament_id`;
3. строит временный list-file в порядке `TS -> HH` для каждой пары;
4. запускает текущий `bulk_local_import`;
5. сохраняет JSON baseline в `backend/target/ingest_v2_bench/latest_run.json`.

Что считаем baseline-метриками на текущем контракте:

- `file_jobs`
- `finalize_jobs`
- `hands_persisted`
- `runner_elapsed_ms`
- `hands_per_minute`
- `stage_profile`

Важно:

- это ещё не честный `dir-import` e2e benchmark;
- текущий harness измеряет baseline на старом file-first контракте, чтобы дальше можно было сравнить ускорения по фазам 1-5;
- `player_profile_id` должен существовать в локальной БД и относиться к room `gg`.

## Сценарий 2: mixed corpus scan baseline

Пока pair-first prepare-layer не готов, dirty baseline строится через существующий `wide_corpus_triage`.

Harness:

```bash
bash scripts/run_ingest_v2_mixed_scan.sh
bash scripts/run_ingest_v2_mixed_scan.sh /absolute/path/to/local/mixed-root
```

Что делает скрипт:

1. прогоняет committed quarantine sample как минимальный воспроизводимый dirty corpus baseline;
2. опционально добавляет локальный bulk root;
3. сохраняет JSON report в `backend/target/ingest_v2_mixed_scan/latest_report.json`.

Текущий mixed baseline отвечает на вопросы:

- сколько файлов попадает в triage;
- какие parser issue family встречаются;
- как выглядит dirty corpus до pair-first reject pipeline.

Пока это ещё не финальный reject-report из Phase 1. После внедрения общего prepare-layer этот harness должен быть переведён на scan/classify/pair/reject output.

## Ожидаемая эволюция

- Фаза 0: baseline через `bulk_local_import` и `wide_corpus_triage`.
- Фаза 1: заменить dirty baseline на pair-aware prepare report.
- Фаза 3+: добавить `workers_used` и сравнение `1 worker` vs `N workers`.
- Фаза 5: заменить runner-only метрику на `prep_elapsed_ms`, `runner_elapsed_ms` и `e2e_elapsed_ms`.
