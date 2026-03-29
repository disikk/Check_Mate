# Ingest V2 Benchmarks

Этот документ фиксирует baseline для фазы 0 плана `ingest_v2_rearchitecture`.

## Цели фазы 0

- отделить happy-path benchmark от грязного mixed corpus;
- получить воспроизводимый baseline уже на pair-first `dir-import` контракте;
- сохранить сравнимость по legacy `stage_profile`, но уже показывать честные `prepare / runner / e2e` метрики рядом.

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
3. собирает временную pair-first директорию из валидных `TS + HH` пар;
4. запускает `parser_worker dir-import`;
5. сохраняет JSON baseline в `backend/target/ingest_v2_bench/latest_run.json`.

Что считаем baseline-метриками на текущем контракте:

- `prepare_report`
- `rejected_by_reason`
- `workers_used`
- `file_jobs`
- `finalize_jobs`
- `hands_persisted`
- `prep_elapsed_ms`
- `runner_elapsed_ms`
- `e2e_elapsed_ms`
- `hands_per_minute`
- `hands_per_minute_runner`
- `hands_per_minute_e2e`
- `e2e_profile.prepare`
- `e2e_profile.runtime`
- `stage_profile`

Важно:

- это уже честный pair-first e2e benchmark для `dir-import`, а legacy `stage_profile` остаётся только для обратной совместимости со старыми замерами;
- `e2e_profile.runtime` теперь честно разделяет HH/runtime на `derive_hand_local_ms`, `derive_tournament_ms` и `persist_db_ms`, чтобы было видно, где CPU, а где реальный DB hot path;
- `player_profile_id` должен существовать в локальной БД и относиться к room `gg`.

## Сценарий 2: mixed corpus scan baseline

Dirty baseline теперь строится уже через shared prepare-layer, а не через отдельный triage helper.

Harness:

```bash
bash scripts/run_ingest_v2_mixed_scan.sh
bash scripts/run_ingest_v2_mixed_scan.sh /absolute/path/to/local/mixed-root
```

Что делает скрипт:

1. запускает `parser_worker dir-import --prepare-only`;
2. прогоняет committed quarantine sample как минимальный воспроизводимый dirty corpus baseline;
3. позволяет подставить локальный mixed root;
4. сохраняет JSON report в `backend/target/ingest_v2_mixed_scan/latest_report.json`.

Текущий mixed baseline отвечает на вопросы:

- сколько файлов попадает в triage;
- сколько валидных HH+TS пар найдено;
- какие reject-причины выдаёт prepare-layer до runtime enqueue.

## Ожидаемая эволюция

- Фаза 0: happy-path baseline через pair-first `dir-import`.
- Фаза 1: dirty baseline через pair-aware prepare report.
- Фаза 3+: добавить `workers_used` и сравнение `1 worker` vs `N workers`.
- Фаза 5: честный `prep_elapsed_ms / runner_elapsed_ms / e2e_elapsed_ms` и nested runtime profile уже подняты; следующий слой теперь зависит от результатов реального corpus profiling.

## Реальный corpus profiling: `MIHA`

### Prepare-only snapshot

Артефакт:

- `backend/target/miha_prepare_report.json`

Что показал prepare-only прогон по `/Users/cyberjam/!Poker/HHs/MIHA`:

- `scanned_files = 2812`
- `paired_tournaments = 1399`
- `rejected_tournaments = 9`
- `rejected_by_reason`:
  - `unsupported_source = 3`
  - `missing_hh = 6`
- prepare timings:
  - `scan_ms = 489`
  - `pair_ms = 3391`
  - `hash_ms = 2669`

Практический вывод:

- prepare-layer уже достаточно дешёвый даже на реальной большой директории;
- главная цена живёт не в scan/pair/hash, а дальше в runtime execution.

### Sample-50 scaling после runtime fix

Артефакты:

- baseline before fix: `backend/target/miha_sample50_w8.json`
- after fix: `backend/target/miha_sample50_w8_after_fix.json`

Результат `50` валидных пар на `8 workers`:

- до фикса:
  - `runner_elapsed_ms = 139315`
  - `hands_per_minute_runner = 554.28`
- после фикса:
  - `runner_elapsed_ms = 33752`
  - `hands_per_minute_runner = 2287.86`

Итог:

- `4.13x` speedup на том же sample после удаления bundle-row mutation из hot path `claim_next_job`.

### Full `MIHA` partial live run после runtime fix

Полный импорт был намеренно остановлен после того, как картина bottleneck-ов стала достаточно ясной.

Профиль свежего run:

- target profile: `7d99654d-635e-4cbb-af24-2538a8a38db2`
- bundle: `524a6bcd-8187-48ab-a59a-4ca899d9ed00`
- stop point:
  - `elapsed_seconds = 1450`
  - `file_succeeded = 1693`
  - `file_queued = 1105`
  - `file_failed = 0`
  - `hands_persisted = 22142`
  - `tournaments_persisted = 850`

Live throughput at this stop point:

- `hands_per_minute_live ~= 916`
- `tournaments_per_minute_live ~= 35.17`
- `file_jobs_per_minute_live ~= 70.06`

### Диагностические выводы

1. Подтверждённый фикс:
   - multi-worker stall действительно сидел в `claim_next_job`, где каждый worker мутировал `import.ingest_bundles`; после перевода claim-time event payload на read-only snapshot параллелизм ожил.

2. Что уже быстро:
   - prepare-layer (`scan/pair/hash`) на реальном corpus дешёвый;
   - dependency-aware queue и multi-worker drain больше не стоят колом;
   - sample paired corpus после фикса уже уверенно пробивает `2000+ hands/min`.

3. Что всё ещё дорого:
   - HH runtime остаётся главным тяжёлым слоем;
   - на полном `MIHA` sustained throughput держится около `900+ hands/min`, заметно ниже малого sample.

4. Следующий вероятный overhead:
   - это inference из кода и live DB traces: `file_updated/bundle_updated` сейчас всё ещё строятся через полный bundle snapshot (`load_bundle_snapshot_*`), который тянет весь список bundle files, per-file diagnostics и activity log;
   - на больших bundle это даёт масштабирование хуже линейного и выглядит как следующий сильный кандидат на оптимизацию после снятого bundle-row lock.

5. Следующая инженерная волна:
   - облегчить event payload builders, чтобы `file_updated` читал только один file snapshot, а `bundle_updated` использовал агрегаты вместо полного `load_bundle_snapshot`;
   - затем снова повторить full `MIHA` profiling;
   - отдельно оценить, нужно ли выносить CPU-heavy HH compute из долгой открытой транзакции до чистого DB persist.
