# Normalized Hand Golden Regression Design

Дата: 2026-03-26
Задача: `P0-05` из `docs/par_nor.md`

## Проблема

Сейчас `tracker_parser_core` защищен набором точечных assertion-тестов на parser / normalizer / positions / side pots. Это хорошо ловит локальные инварианты, но плохо показывает полный shape-diff нормализованной руки после изменений exact-core.

Из-за этого возможны две неприятные ситуации:

- изменение структуры `NormalizedHand` или ее сериализованного содержимого проходит незаметно, если не задевает существующие точечные assertion'ы;
- осознанное изменение exact-core surface трудно review-ить целиком, потому что нет сохраненного эталонного JSON-diff по committed corpus.

## Цель

Ввести golden snapshot regression для `NormalizedHand`, чтобы любое изменение full serialized output по committed HH pack было явным, diff-friendly и осознанно подтверждаемым.

## Принятые решения

### 1. Coverage scope

Golden regression в первой итерации покрывает весь committed HH pack из `backend/fixtures/mbr/hh`, включая `GG20260325-phase0-exact-core-edge-matrix.txt`.

### 2. Snapshot width

Храним полный serialized JSON normalized output без curated projection и без урезания полей. Цель этого среза — ловить именно полный drift exact-core surface.

### 3. Golden granularity

Один golden JSON хранится на каждый исходный fixture-файл HH.

Это значит:

- diff остается локальным к конкретному source fixture;
- тестовый код не разрастается до сотен отдельных golden-файлов по одной руке;
- внутри одного файла можно детерминированно сравнивать весь набор рук данного fixture.

### 4. Golden structure

Каждый golden JSON содержит:

- `fixture_file`;
- `hand_count`;
- `hands`;

где `hands` — это детерминированно отсортированный map `external_hand_id -> serialized NormalizedHand`.

### 5. Update policy

Golden-файлы обновляются только при явном `UPDATE_GOLDENS=1`.

Обычный тестовый прогон:

- ничего не перезаписывает;
- падает при mismatch;
- дает явную подсказку, как осознанно обновить эталоны.

### 6. First-run / drift policy

Если golden отсутствует:

- обычный прогон падает с понятным сообщением;
- прогон с `UPDATE_GOLDENS=1` создает файл.

Если fixture удален, renamed или изменился состав рук:

- тест падает явно через mismatch `fixture_file` / `hand_count` / `external_hand_id` map;
- никаких silent auto-recreate без флага не происходит.

## Архитектура

Добавляется новый integration-style test в `backend/crates/tracker_parser_core/tests/normalized_hand_golden.rs`, который:

1. перечисляет committed HH fixtures;
2. для каждого файла делает `split_hand_history -> parse_canonical_hand -> normalize_hand`;
3. сериализует результат в golden-friendly JSON structure;
4. сравнивает с committed JSON из `tests/goldens/`;
5. при `UPDATE_GOLDENS=1` переписывает golden-файл.

## Не делаем в этом срезе

- не добавляем отдельный curated projection layer поверх `NormalizedHand`;
- не заводим golden на каждую отдельную руку;
- не делаем auto-update без флага;
- не меняем exact-core semantics ради “красивого” snapshot'а.

## Acceptance

- есть отдельный golden test для `tracker_parser_core`;
- весь committed HH pack покрыт golden JSON файлами;
- mismatch дает diff-friendly failure;
- intentional change обновляется только через `UPDATE_GOLDENS=1`;
- `hand_normalization` и `phase0_exact_core_corpus` остаются complementary tests, а не заменяются golden-механизмом.
