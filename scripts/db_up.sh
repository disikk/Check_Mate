#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Shared env/default loading keeps all root scripts on one DB contract.
source "$SCRIPT_DIR/common.sh"

load_project_env
cd "$ROOT_DIR"

docker compose up -d postgres
printf '[db-up] postgres started on localhost:%s\n' "$POSTGRES_PORT"
