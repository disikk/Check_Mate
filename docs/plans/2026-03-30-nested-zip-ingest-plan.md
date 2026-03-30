# Nested ZIP Ingest Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Научить общий ingest-контур рекурсивно раскрывать ZIP и nested ZIP, импортировать валидные HH/TS leaf-файлы и отбрасывать невалидные источники без падения всего импорта.

**Architecture:** Решение строится без миграций схемы: typed archive locator живёт в коде, а в существующий `member_path` сериализуется стабильной строкой. `tracker_ingest_prepare` отвечает за рекурсивный scan и pair-first reject/candidate surface, а `parser_worker` использует тот же locator для runtime-чтения archive members.

**Tech Stack:** Rust 2024, `zip`, `tracker_ingest_prepare`, `parser_worker`, `tracker_ingest_runtime`.

---

### Task 1: Зафиксировать red-тесты на nested ZIP prepare path

**Files:**
- Modify: `backend/crates/tracker_ingest_prepare/tests/prepare_report.rs`

**Step 1: Write the failing test**

- Добавить тест, где top-level ZIP содержит inner ZIP с валидной `TS+HH` парой, и `prepare_path()` должен вернуть `paired_tournaments.len() == 1`.
- Добавить тест, где inner ZIP содержит невалидный leaf-файл, и `prepare_path()` должен вернуть `unsupported_source`, не падая scan.

**Step 2: Run test to verify it fails**

Run: `cargo test -p tracker_ingest_prepare --test prepare_report nested_zip -- --nocapture`

Expected: FAIL, потому что текущий scan раскрывает только один ZIP-уровень.

**Step 3: Write minimal implementation**

- Пока не писать production code; сначала зафиксировать оба failing tests.

**Step 4: Run test to verify it fails consistently**

Run: `cargo test -p tracker_ingest_prepare --test prepare_report nested_zip -- --nocapture`

Expected: FAIL по ожидаемой причине рекурсивного ZIP traversal.

### Task 2: Зафиксировать red-тест на runtime чтение nested archive member

**Files:**
- Modify: `backend/crates/parser_worker/src/local_import.rs`

**Step 1: Write the failing test**

- Добавить тест, где `load_ingest_job_input()` читает leaf-файл из `outer.zip -> inner.zip -> member.hh`.

**Step 2: Run test to verify it fails**

Run: `cargo test -p parser_worker load_ingest_job_input_reads_nested_archive_member_text -- --nocapture`

Expected: FAIL, потому что runtime делает только один `archive.by_name(member_path)`.

### Task 3: Реализовать shared archive locator и recursive prepare scan

**Files:**
- Modify: `backend/crates/tracker_ingest_prepare/src/lib.rs`
- Modify: `backend/crates/tracker_ingest_prepare/src/archive.rs`
- Modify: `backend/crates/tracker_ingest_prepare/src/scan.rs`

**Step 1: Add typed locator helpers**

- Добавить codec `segments <-> member_path string`.
- Экспортировать decode helper для runtime use в `parser_worker`.

**Step 2: Implement recursive ZIP traversal**

- Научить `list_archive_members()` обходить direct и nested ZIP members.
- Для nested ZIP использовать in-memory bytes/reader без временной распаковки на диск.
- Битые inner ZIP превращать в reject surface, а не в hard error scan.

**Step 3: Keep pair/hash behavior compatible**

- `PreparedFileRef.member_path` остаётся `Option<String>`.
- `ensure_sha256()` и hashing должны уметь читать leaf по locator chain.

**Step 4: Run prepare tests**

Run: `cargo test -p tracker_ingest_prepare --test prepare_report -- --nocapture`

Expected: PASS.

### Task 4: Реализовать recursive runtime archive reading

**Files:**
- Modify: `backend/crates/parser_worker/src/local_import.rs`

**Step 1: Add nested member reader**

- Научить `read_archive_member_bytes()` и `read_archive_member_text()` проходить по decoded locator chain.
- Оставить existing storage/runtime DB contract без миграций.

**Step 2: Add invalid leaf guard**

- Если runtime-прочитанный leaf содержит `\0`, завершать file job controlled failure, не давая источнику упасть в PostgreSQL.

**Step 3: Run focused runtime tests**

Run: `cargo test -p parser_worker load_ingest_job_input_reads_ -- --nocapture`

Expected: PASS.

### Task 5: Синхронизировать документацию и верифицировать на реальном корпусе

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/ingest_v2_rearchitecture_progress.md`
- Modify: `docs/plans/2026-03-30-nested-zip-ingest-progress.md`

**Step 1: Update architecture docs**

- Зафиксировать, что ingest теперь рекурсивно раскрывает nested ZIP через shared archive locator.

**Step 2: Run verification**

Run:
- `cargo test -p tracker_ingest_prepare --test prepare_report -- --nocapture`
- `cargo test -p parser_worker load_ingest_job_input_reads_ -- --nocapture`
- `bash backend/scripts/run_backend_checks.sh`

Expected: PASS.

**Step 3: Run real import benchmark**

Run:
- `/usr/bin/time -p ./target/release/parser_worker dir-import --player-profile-id <fresh_uuid> --workers 8 '/Users/cyberjam/!Poker/HHs/KONFA/Aleshka/15k hands'`

Expected:
- импорт завершается без `invalid byte sequence for encoding "UTF8": 0x00`;
- собираются wall-clock time, `hands_per_minute_e2e`, `hands/sec`.
