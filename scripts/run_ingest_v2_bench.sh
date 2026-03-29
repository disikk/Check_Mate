#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

load_project_env

PLAYER_PROFILE_ID="${1:-${INGEST_V2_BENCH_PLAYER_PROFILE_ID:-}}"
if [[ -z "$PLAYER_PROFILE_ID" ]]; then
  printf 'usage: bash scripts/run_ingest_v2_bench.sh <player-profile-id>\n' >&2
  printf 'or set INGEST_V2_BENCH_PLAYER_PROFILE_ID\n' >&2
  exit 1
fi

HH_DIR="${INGEST_V2_BENCH_HH_DIR:-$ROOT_DIR/backend/fixtures/mbr/hh}"
TS_DIR="${INGEST_V2_BENCH_TS_DIR:-$ROOT_DIR/backend/fixtures/mbr/ts}"
HH_GLOB="${INGEST_V2_BENCH_HH_GLOB:-GG20260316-*.txt}"
TS_GLOB="${INGEST_V2_BENCH_TS_GLOB:-GG20260316 - Tournament #*.txt}"
OUTPUT_DIR="${INGEST_V2_BENCH_OUTPUT_DIR:-$ROOT_DIR/backend/target/ingest_v2_bench}"
LIST_FILE="${INGEST_V2_BENCH_LIST_FILE:-$OUTPUT_DIR/happy_path_files.txt}"
JSON_OUT="${INGEST_V2_BENCH_JSON_OUT:-$OUTPUT_DIR/latest_run.json}"
RUNNER_NAME="${INGEST_V2_BENCH_RUNNER_NAME:-parser_worker_ingest_v2_bench}"

mkdir -p "$OUTPUT_DIR"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

hh_map="$tmp_dir/hh.tsv"
ts_map="$tmp_dir/ts.tsv"
pair_map="$tmp_dir/pairs.tsv"

extract_tournament_id() {
  local path="$1"
  sed -n '1{/Tournament #/s/^.*Tournament #\([0-9][0-9]*\).*$/\1/p; q;}' "$path"
}

build_map() {
  local dir="$1"
  local pattern="$2"
  local out="$3"

  find "$dir" -type f -name "$pattern" -print0 \
    | while IFS= read -r -d '' path; do
        local tournament_id
        tournament_id="$(extract_tournament_id "$path")"
        if [[ -z "$tournament_id" ]]; then
          printf 'failed to extract tournament_id from %s\n' "$path" >&2
          exit 1
        fi
        printf '%s\t%s\n' "$tournament_id" "$path"
      done \
    | sort -n > "$out"
}

build_map "$HH_DIR" "$HH_GLOB" "$hh_map"
build_map "$TS_DIR" "$TS_GLOB" "$ts_map"

hh_count="$(wc -l < "$hh_map" | tr -d ' ')"
ts_count="$(wc -l < "$ts_map" | tr -d ' ')"
if [[ "$hh_count" == "0" || "$ts_count" == "0" ]]; then
  printf 'empty benchmark corpus: hh=%s ts=%s\n' "$hh_count" "$ts_count" >&2
  exit 1
fi

join -t "$(printf '\t')" -j 1 "$ts_map" "$hh_map" > "$pair_map"
pair_count="$(wc -l < "$pair_map" | tr -d ' ')"
if [[ "$pair_count" == "0" ]]; then
  printf 'no HH+TS pairs found for the benchmark corpus\n' >&2
  exit 1
fi

if [[ "$pair_count" != "$hh_count" || "$pair_count" != "$ts_count" ]]; then
  printf 'happy-path benchmark corpus must be fully paired: hh=%s ts=%s pairs=%s\n' \
    "$hh_count" "$ts_count" "$pair_count" >&2
  exit 1
fi

: > "$LIST_FILE"
while IFS="$(printf '\t')" read -r _ ts_path hh_path; do
  printf '%s\n%s\n' "$ts_path" "$hh_path" >> "$LIST_FILE"
done < "$pair_map"

file_count="$((pair_count * 2))"
chunk_size="${INGEST_V2_BENCH_CHUNK_SIZE:-$file_count}"

printf 'running ingest v2 happy-path benchmark\n' >&2
printf 'player_profile_id=%s\n' "$PLAYER_PROFILE_ID" >&2
printf 'hh_dir=%s\n' "$HH_DIR" >&2
printf 'ts_dir=%s\n' "$TS_DIR" >&2
printf 'pairs=%s files=%s chunk_size=%s\n' "$pair_count" "$file_count" "$chunk_size" >&2
printf 'list_file=%s\n' "$LIST_FILE" >&2
printf 'json_out=%s\n' "$JSON_OUT" >&2

cd "$ROOT_DIR/backend"
cargo run -p parser_worker --bin bulk_local_import -- \
  --player-profile-id "$PLAYER_PROFILE_ID" \
  --list-file "$LIST_FILE" \
  --chunk-size "$chunk_size" \
  --runner-name "$RUNNER_NAME" \
  | tee "$JSON_OUT"
