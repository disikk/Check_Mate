#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

load_project_env
cd "$ROOT_DIR/backend"
printf '[backend-test] CHECK_MATE_DATABASE_URL=%s\n' "$CHECK_MATE_DATABASE_URL"
cargo test
