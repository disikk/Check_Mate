#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

load_project_env
cd "$ROOT_DIR"
docker compose exec postgres psql -U "$POSTGRES_USER" -d "$POSTGRES_DB"
