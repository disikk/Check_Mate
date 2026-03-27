# Wide Corpus Triage

Дата: 2026-03-27
Задача: `P2-01`

## Что это

`wide_corpus_triage` — отдельный offline pipeline для широкого реального HH/TS корпуса. Он не импортирует данные в БД и не лезет в `parser_worker import-local`.

Его задача:

- мерить parser coverage числом;
- отделять known allowed issues от неожиданных;
- собирать family-based syntax surface report.

## Layout корпуса

### Committed sample

Минимальный воспроизводимый sample лежит в репозитории:

- `backend/fixtures/mbr/quarantine_sample/hh`
- `backend/fixtures/mbr/quarantine_sample/ts`

Он должен оставаться самодостаточным: triage можно запустить даже без локального bulk corpus.

### Local bulk corpus

Основной реальный quarantine corpus лежит вне git по умолчанию здесь:

- `backend/.local/wide_corpus_quarantine/hh`
- `backend/.local/wide_corpus_quarantine/ts`

Эта директория intentionally ignored через `.gitignore`.

Можно передать и другой локальный путь.

## Команды

### Минимальный запуск на committed sample

```bash
bash backend/scripts/run_wide_corpus_triage.sh
```

### Запуск с локальным bulk corpus

```bash
bash backend/scripts/run_wide_corpus_triage.sh /absolute/path/to/wide_corpus_quarantine
```

### Запуск через env

```bash
WIDE_CORPUS_LOCAL_ROOT=/absolute/path/to/wide_corpus_quarantine \
WIDE_CORPUS_JSON_OUT=/absolute/path/to/report.json \
bash backend/scripts/run_wide_corpus_triage.sh
```

## Что лежит в отчете

JSON-report по умолчанию пишется в:

- `backend/target/wide_corpus_triage/latest_report.json`

Ключевые поля:

- `source_files_total` / `source_files_parsed_ok` / `source_files_failed`
- `hh_files_total` / `ts_files_total`
- `hands_total`
- `hands_normalized_exact`
- `hands_normalized_uncertain`
- `hands_normalized_inconsistent`
- `allowed_issue_count`
- `unexpected_issue_count`
- `hands_with_unexpected_parse_issues`
- `issue_counts_by_code`
- `syntax_families`

## Как читать метрики

### File-level

- `source_files_failed > 0` означает, что источник как контейнер уже плохо пригоден для triage и требует первичного разбора причины.

### Hand-level

- `hands_normalized_uncertain` — exact-core не может честно доказать settlement.
- `hands_normalized_inconsistent` — факты руки конфликтуют друг с другом.

### Issue-level

- `allowed_issue_count` — уже известные и осознанно допускаемые typed surfaces.
- `unexpected_issue_count` — новый или нежелательный issue code, которого нет в allowlist.
- `hands_with_unexpected_parse_issues` — число HH рук, где встретился хотя бы один unexpected issue.

## Syntax family contract

Каждая запись `syntax_families[]` хранит:

- `family_key`
- `surface_kind`
- `issue_code` или `parse_failure_kind`
- `hit_count`
- несколько `example_lines`

Family key intentionally coarse-grained. Например:

- `hh_show_line::partial_reveal_show_line`
- `ts_tail::ts_tail_finish_place_mismatch`
- `ts_file::parse_tournament_summary_failed`

Это сделано специально, чтобы triage копил паттерны по семействам, а не по каждой строке.

## Как добавлять новый корпус

1. Положить новые HH/TS в локальный quarantine root.
2. Запустить `run_wide_corpus_triage.sh`.
3. Посмотреть `unexpected_issue_count` и `syntax_families`.
4. Для каждого нового family решить:
   - это допустимая explicit surface;
   - это parser gap;
   - это bad source data / file failure.
5. Обновить `docs/COMMITTED_PACK_SYNTAX_CATALOG.md`, если появился новый признанный family.
6. Только после review расширять allowlist в `tracker_parser_core::wide_corpus_triage`.

## Чего этот pipeline не делает

- не импортирует в PostgreSQL;
- не обновляет catalog автоматически;
- не является CI gate для большого локального корпуса;
- не заменяет committed golden/proof tests exact-core.
