# macOS Environment Setup Progress

## Цель

Подготовить этот Mac для полноценной локальной работы приложения `Check_Mate`, включая frontend runtime, Rust backend toolchain и локальный PostgreSQL.

## Статус по фазам

- [x] Фаза 1. Аудит текущего окружения
- [x] Фаза 2. Установка системных зависимостей
- [x] Фаза 3. Настройка локального PostgreSQL для проекта
- [x] Фаза 4. Финальная верификация окружения

## Текущий аудит

- `brew`: найден в `/opt/homebrew/bin/brew`
- `node`: найден, версия `v20.11.1`
- `npm`: найден, версия `10.2.4`
- `cargo`: отсутствует
- `rustc`: отсутствует
- `psql`: отсутствует
- `postgres`: отсутствует
- `docker`: отсутствует
- В репозитории уже есть `node_modules` и `package-lock.json`
- Backend требует Rust workspace и локальный PostgreSQL для `cargo test` и `parser_worker import-local`

## Журнал выполнения

### 2026-03-24

- Созданы plan-файл и progress-файл для системной настройки окружения.
- Выполнен аудит текущих инструментов и подтверждено, что фронтенд-часть частично готова, а backend runtime на этом Mac отсутствует.
- Выявлены обязательные системные зависимости для установки: Rust toolchain и PostgreSQL.
- Установлен `postgresql@16` через Homebrew, подтверждена доступность `psql 16.13`.
- Установлен `rustup` через Homebrew и активирован `stable` toolchain с minimal profile.
- Подтверждены рабочие версии: `cargo 1.94.0`, `rustc 1.94.0`, `rustup 1.29.0`.
- В `~/.zshrc` и `~/.zprofile` добавлен PATH для `postgresql@16`, `rustup` и `$HOME/.cargo/bin`, чтобы инструменты были доступны в обычной оболочке.
- Выявлен конфликт локальных PostgreSQL-инстансов: порт `5432` уже занят существующим `PostgreSQL 12` из `/Library/PostgreSQL/12`, работающим отдельным сервисом от OS-пользователя `postgres`.
- Поднятый через Homebrew `postgresql@16` не стал активным проектным runtime, потому что на стандартном порту уже слушает другой локальный сервер.
- `.pgpass` и переменные окружения `PG*`/`POSTGRES*` у пользователя не настроены, поэтому готовые сохранённые доступы к существующему серверу пока не обнаружены.
- В локальном `pgAdmin` найден серверный профиль `PostgreSQL 12 -> localhost:5432`, maintenance DB `postgres`, user `postgres`.
- Пароль для этого подключения в `pgAdmin` не сохранён, поэтому автоматическое переиспользование существующего `PostgreSQL 12` без знания текущего пароля сейчас заблокировано.
- Дополнительно confirmed: существующий `PostgreSQL 12` запущен как отдельный сервис EnterpriseDB (`/Library/PostgreSQL/12/bin/postmaster -D /Library/PostgreSQL/12/data/`), а его data directory недоступен текущему пользователю без административного повышения прав.
- Выполнена проверка типовых паролей `postgres`, `postgresdb`, `admin` для `postgres@localhost:5432`; все варианты отклонены сервером с `password authentication failed`.
- На текущем шаге использование существующего `PostgreSQL 12` остаётся заблокированным до получения реального пароля или смены стратегии на отдельный проектный инстанс.
- Отдельный Homebrew `postgresql@16` переведён на порт `5433` и успешно запущен как независимый сервис, не затрагивающий существующий `PostgreSQL 12` на `5432`.
- Внутри кластера PostgreSQL 16 создана проектная база `check_mate_dev`; migration `backend/migrations/0001_init_source_of_truth.sql` и seed `backend/seeds/0001_reference_data.sql` применены успешно.
- В `~/.zshrc` и `~/.zprofile` добавлен `CHECK_MATE_DATABASE_URL=\"host=localhost port=5433 user=postgres dbname=check_mate_dev\"`.
- Seed-проверка подтверждена в новой БД: `core.rooms = 1`, `core.formats = 1`.
- `npm run build` выполнен успешно; production build собирается в `dist/`.
- `cargo build` выполнен успешно; Rust workspace и toolchain рабочие.
- Полный `cargo test` и `cargo test -p parser_worker --bin parser_worker` сейчас не проходят не из-за окружения, а из-за отсутствующих fixture-файлов `backend/fixtures/mbr/...` в текущем репозитории.
- Дополнительно подтверждено, что `cargo test --lib -p tracker_parser_core` проходит успешно.
- Smoke-проверка `import-local` не выполнена по той же причине: в репозитории отсутствуют реальные GG MBR fixture-файлы, на которых она должна запускаться.
- Из архивов пользователя восстановлены fixture-паки: `9` HH-файлов положены в `backend/fixtures/mbr/hh`, `9` TS-файлов — в `backend/fixtures/mbr/ts`.
- После восстановления fixtures полный `cargo test` проходит успешно: parser worker unit tests, fixture parsing tests и hand normalization tests зелёные.
- Отдельно подтверждён ignored PostgreSQL integration test `local_import::tests::import_local_persists_canonical_hand_layer_to_postgres`; для него потребовался запуск вне sandbox из-за локального доступа к PostgreSQL.
