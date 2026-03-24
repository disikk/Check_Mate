#!/usr/bin/env bash
set -euo pipefail
if [[ $# -lt 1 ]]; then
  printf 'usage: bash scripts/import_fixture.sh <path-to-hh-or-ts>\n' >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

load_project_env
FILE_PATH="$1"
cd "$ROOT_DIR/backend"
cargo run -p parser_worker -- import-local "$ROOT_DIR/$FILE_PATH"
