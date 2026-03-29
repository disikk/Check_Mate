#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

load_project_env

OUTPUT_DIR="${INGEST_V2_MIXED_OUTPUT_DIR:-$ROOT_DIR/backend/target/ingest_v2_mixed_scan}"
JSON_OUT="${INGEST_V2_MIXED_JSON_OUT:-$OUTPUT_DIR/latest_report.json}"
LOCAL_ROOT="${1:-${INGEST_V2_MIXED_ROOT:-}}"

mkdir -p "$OUTPUT_DIR"

export WIDE_CORPUS_JSON_OUT="$JSON_OUT"

printf 'running ingest v2 mixed scan baseline\n' >&2
printf 'json_out=%s\n' "$JSON_OUT" >&2
if [[ -n "$LOCAL_ROOT" ]]; then
  printf 'local_root=%s\n' "$LOCAL_ROOT" >&2
  bash "$ROOT_DIR/backend/scripts/run_wide_corpus_triage.sh" "$LOCAL_ROOT"
else
  printf 'local_root=<committed quarantine sample only>\n' >&2
  bash "$ROOT_DIR/backend/scripts/run_wide_corpus_triage.sh"
fi
