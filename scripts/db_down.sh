#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

cd "$ROOT_DIR"
if [[ "${1:-}" == "--volumes" ]]; then
  docker compose down -v
  printf '[db-down] postgres stopped and volumes removed\n'
else
  docker compose down
  printf '[db-down] postgres stopped\n'
fi
