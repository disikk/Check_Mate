#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_DATABASE_URL="host=localhost port=5433 user=postgres dbname=check_mate_dev"
CHECK_MATE_DATABASE_URL="${CHECK_MATE_DATABASE_URL:-$DEFAULT_DATABASE_URL}"
export CHECK_MATE_DATABASE_URL

log() {
  printf '[bootstrap] %s\n' "$1"
}

die() {
  printf '[bootstrap] error: %s\n' "$1" >&2
  exit 1
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    die "required command '$1' is not installed"
  fi
}

resolve_psql() {
  local candidate=""
  local candidates=(
    "${PSQL_BIN:-}"
    "$(command -v psql 2>/dev/null || true)"
    "/opt/homebrew/opt/postgresql@16/bin/psql"
    "/opt/homebrew/bin/psql"
    "/Library/PostgreSQL/12/bin/psql"
  )

  for candidate in "${candidates[@]}"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  die "required command 'psql' is not installed or not on PATH"
}

build_maintenance_url() {
  local db_name=""
  local parts=()
  local token=""

  for token in $CHECK_MATE_DATABASE_URL; do
    case "$token" in
      dbname=*)
        db_name="${token#dbname=}"
        ;;
      *)
        parts+=("$token")
        ;;
    esac
  done

  if [[ -z "$db_name" ]]; then
    die "CHECK_MATE_DATABASE_URL must include dbname=<database> in libpq key=value format"
  fi

  printf '%s\n' "$(printf '%s ' "${parts[@]}")dbname=postgres|${db_name}"
}

require_cmd cargo

if [[ "$CHECK_MATE_DATABASE_URL" == *"://"* ]]; then
  die "bootstrap currently supports libpq key=value CHECK_MATE_DATABASE_URL values only"
fi

PSQL_BIN="$(resolve_psql)"
export PSQL_BIN

maintenance_data="$(build_maintenance_url)"
MAINTENANCE_URL="${maintenance_data%%|*}"
TARGET_DB_NAME="${maintenance_data##*|}"

if ! "$PSQL_BIN" "$MAINTENANCE_URL" -v ON_ERROR_STOP=1 -Atqc "SELECT 1" >/dev/null 2>&1; then
  die "PostgreSQL is not reachable via ${MAINTENANCE_URL}"
fi

db_exists="$(
  "$PSQL_BIN" "$MAINTENANCE_URL" -v ON_ERROR_STOP=1 -Atqc \
    "SELECT 1 FROM pg_database WHERE datname = '${TARGET_DB_NAME}'"
)"

if [[ "$db_exists" != "1" ]]; then
  log "creating database ${TARGET_DB_NAME}"
  "$PSQL_BIN" "$MAINTENANCE_URL" -v ON_ERROR_STOP=1 -c "CREATE DATABASE \"${TARGET_DB_NAME}\"" >/dev/null
else
  log "database ${TARGET_DB_NAME} already exists"
fi

shopt -s nullglob
migration_files=("${BACKEND_DIR}"/migrations/*.sql)
shopt -u nullglob

if [[ "${#migration_files[@]}" -eq 0 ]]; then
  die "no migration files found in ${BACKEND_DIR}/migrations"
fi

for migration in "${migration_files[@]}"; do
  log "applying migration $(basename "$migration")"
  "$PSQL_BIN" "$CHECK_MATE_DATABASE_URL" -v ON_ERROR_STOP=1 -f "$migration" >/dev/null
done

seed_file="${BACKEND_DIR}/seeds/0001_reference_data.sql"
if [[ ! -f "$seed_file" ]]; then
  die "reference seed is missing at ${seed_file}"
fi

log "applying seed $(basename "$seed_file")"
"$PSQL_BIN" "$CHECK_MATE_DATABASE_URL" -v ON_ERROR_STOP=1 -f "$seed_file" >/dev/null

cat <<EOF
[bootstrap] backend foundation is ready
[bootstrap] CHECK_MATE_DATABASE_URL=${CHECK_MATE_DATABASE_URL}

Next steps:
  cd "${BACKEND_DIR}"
  cargo test
  cargo test -p parser_worker local_import::tests::import_local_persists_canonical_hand_layer_to_postgres -- --ignored --exact
  cargo test -p parser_worker local_import::tests::import_local_refreshes_analytics_features_and_seed_stats -- --ignored --exact
  bash scripts/run_backend_checks.sh
EOF
