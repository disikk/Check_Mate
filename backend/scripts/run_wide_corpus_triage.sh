#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
backend_dir="${repo_root}/backend"

local_root="${1:-${WIDE_CORPUS_LOCAL_ROOT:-}}"
json_out="${WIDE_CORPUS_JSON_OUT:-${backend_dir}/target/wide_corpus_triage/latest_report.json}"

args=("--json-out" "${json_out}")
if [[ -n "${local_root}" ]]; then
  args+=("--local-root" "${local_root}")
fi

echo "running wide_corpus_triage"
echo "repo_root=${repo_root}"
echo "json_out=${json_out}"
if [[ -n "${local_root}" ]]; then
  echo "local_root=${local_root}"
else
  echo "local_root=<auto>"
fi

cd "${backend_dir}"
cargo run -p tracker_parser_core --bin wide_corpus_triage -- "${args[@]}"
