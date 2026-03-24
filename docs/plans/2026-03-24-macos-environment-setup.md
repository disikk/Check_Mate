# macOS Environment Setup Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Проблема:** На этом Mac окружение проекта `Check_Mate` настроено частично: фронтенд-зависимости уже присутствуют, но для backend-части отсутствуют `cargo`, `rustc`, `psql` и локальный PostgreSQL runtime. Из-за этого нельзя полноценно запускать `cargo test`, выполнять `parser_worker import-local` и воспроизводить backend smoke-сценарии.

**Цель:** Установить и настроить на macOS все системные зависимости, необходимые для локальной работы frontend и backend частей `Check_Mate`, включая Rust toolchain, PostgreSQL и рабочую dev-базу `check_mate_dev`.

**Architecture:** Настройка делается поверх текущего репозитория без изменения продуктовой архитектуры. Системные зависимости ставятся через Homebrew и rustup, PostgreSQL поднимается как локальный сервис, после чего проект проверяется штатными командами frontend и backend.

**Tech Stack:** macOS, Homebrew, Node.js/npm, React/Vite, Rust 2024 edition, PostgreSQL, SQL migrations/seeds.

---

## Зависимости между фазами

- Фаза 1 обязательна перед любой установкой: сначала нужно подтвердить, что уже стоит, и что именно отсутствует.
- Фаза 2 зависит от Фазы 1: установка должна покрыть только реально недостающие системные пакеты.
- Фаза 3 зависит от Фазы 2: инициализация БД невозможна без установленного и запущенного PostgreSQL.
- Фаза 4 зависит от Фаз 2 и 3: финальная проверка проходит только после готового Rust toolchain и доступной dev-базы.

### Фаза 1: Аудит текущего окружения

**Зависит от:** нет

**Файлы:**
- Modify: `docs/progress/2026-03-24-macos-environment-setup-progress.md`
- Read: `CLAUDE.md`
- Read: `package.json`
- Read: `backend/README.md`
- Read: `backend/Cargo.toml`
- Read: `backend/crates/parser_worker/Cargo.toml`
- Read: `backend/crates/tracker_parser_core/Cargo.toml`

**Шаги:**
1. Проверить наличие `brew`, `node`, `npm`, `cargo`, `rustc`, `psql`, `postgres`, `docker`.
2. Сопоставить найденные инструменты с фактическими требованиями репозитория.
3. Зафиксировать результаты аудита в progress-файле.

### Фаза 2: Установка системных зависимостей

**Зависит от:** Фаза 1

**Файлы:**
- Modify: `docs/progress/2026-03-24-macos-environment-setup-progress.md`

**Шаги:**
1. Установить Rust toolchain через `brew install rustup`, затем выполнить `rustup-init -y` и активировать stable toolchain.
2. Установить PostgreSQL через Homebrew.
3. При необходимости убедиться, что `node`/`npm` версия достаточна для `Vite 6` и `React 19`.
4. Зафиксировать установленные версии в progress-файле.

### Фаза 3: Настройка локального PostgreSQL для проекта

**Зависит от:** Фаза 2

**Файлы:**
- Modify: `docs/progress/2026-03-24-macos-environment-setup-progress.md`
- Read: `backend/migrations/0001_init_source_of_truth.sql`
- Read: `backend/seeds/0001_reference_data.sql`
- Read: `docs/architecture/2026-03-23-mbr-handoff.md`

**Шаги:**
1. Запустить PostgreSQL как локальный сервис через `brew services start`.
2. Создать dev-базу `check_mate_dev`.
3. Задать совместимые с проектом параметры доступа для `CHECK_MATE_DATABASE_URL`.
4. Применить migration `backend/migrations/0001_init_source_of_truth.sql`.
5. Применить seed `backend/seeds/0001_reference_data.sql`.
6. Зафиксировать состояние БД и способ подключения в progress-файле.

### Фаза 4: Финальная верификация окружения

**Зависит от:** Фаза 2, Фаза 3

**Файлы:**
- Modify: `docs/progress/2026-03-24-macos-environment-setup-progress.md`

**Шаги:**
1. Проверить frontend-команду `npm run build`.
2. Проверить backend-команду `cargo test`.
3. Проверить доступность локального PostgreSQL через `psql` и переменную `CHECK_MATE_DATABASE_URL`.
4. Если fixture-driven smoke import невозможен из-за отсутствия нужных файлов, явно зафиксировать это ограничение.
5. Записать итоговый статус по каждой подсистеме в progress-файл.
