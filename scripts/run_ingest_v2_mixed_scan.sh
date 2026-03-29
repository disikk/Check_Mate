#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

load_project_env

OUTPUT_DIR="${INGEST_V2_MIXED_OUTPUT_DIR:-$ROOT_DIR/backend/target/ingest_v2_mixed_scan}"
JSON_OUT="${INGEST_V2_MIXED_JSON_OUT:-$OUTPUT_DIR/latest_report.json}"
LOCAL_ROOT="${1:-${INGEST_V2_MIXED_ROOT:-$ROOT_DIR/backend/fixtures/mbr/quarantine_sample}}"

mkdir -p "$OUTPUT_DIR"

printf 'running ingest v2 mixed scan baseline\n' >&2
printf 'json_out=%s\n' "$JSON_OUT" >&2
printf 'local_root=%s\n' "$LOCAL_ROOT" >&2

cd "$ROOT_DIR/backend"
cargo run -p parser_worker --bin parser_worker -- dir-import --prepare-only "$LOCAL_ROOT" | tee "$JSON_OUT"
