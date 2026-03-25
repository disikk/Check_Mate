#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_DATABASE_URL="host=localhost port=5433 user=postgres password=postgres dbname=check_mate_dev"
CHECK_MATE_DATABASE_URL="${CHECK_MATE_DATABASE_URL:-$DEFAULT_DATABASE_URL}"
export CHECK_MATE_DATABASE_URL

log() {
  printf '[checks] %s\n' "$1"
}

die() {
  printf '[checks] error: %s\n' "$1" >&2
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

require_rust_component() {
  local component="$1"
  if ! rustup component list --installed | grep -q "^${component}"; then
    die "Rust component '${component}' is required. Install it with: rustup component add ${component}"
  fi
}

run() {
  log "running: $*"
  "$@"
}

require_cmd cargo
require_cmd rustup
PSQL_BIN="$(resolve_psql)"
export PSQL_BIN
require_rust_component rustfmt
require_rust_component clippy

if ! "$PSQL_BIN" "$CHECK_MATE_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc "SELECT 1" >/dev/null 2>&1; then
  die "PostgreSQL is not reachable via CHECK_MATE_DATABASE_URL. Run backend/scripts/bootstrap_backend_dev.sh first."
fi

cd "$BACKEND_DIR"

run cargo fmt --check
run cargo clippy --workspace --all-targets -- -D warnings
run cargo test
log "running exact-core proof suite"
run cargo test -p tracker_parser_core --test fixture_parsing -- --nocapture
run cargo test -p tracker_parser_core --test positions -- --nocapture
run cargo test -p tracker_parser_core --test hand_normalization -- --nocapture
run cargo test -p tracker_parser_core --test phase0_exact_core_corpus -- --nocapture
run cargo test -p parser_worker local_import::tests::import_local_full_pack_smoke_is_clean -- --ignored --exact
run cargo test -p parser_worker local_import::tests::import_local_persists_tournament_summary_tail_conflicts_as_parse_issues -- --ignored --exact
run cargo test -p parser_worker local_import::tests::import_local_persists_cm06_joint_ko_fields_to_postgres -- --ignored --exact
run cargo test -p parser_worker local_import::tests::import_local_persists_canonical_hand_layer_to_postgres -- --ignored --exact
run cargo test -p parser_worker local_import::tests::import_local_refreshes_analytics_features_and_seed_stats -- --ignored --exact
run cargo test -p parser_worker local_import::tests::import_local_keeps_early_ft_ko_seed_stats_exact_without_proxy_hand_features -- --ignored --exact
run cargo test -p parser_worker local_import::tests::import_local_exposes_exact_core_descriptors_to_runtime_filters -- --ignored --exact
