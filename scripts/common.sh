#!/usr/bin/env bash

COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$COMMON_DIR/.." && pwd)"

load_project_env() {
  local env_file="$ROOT_DIR/.env"

  if [[ -f "$env_file" ]]; then
    while IFS='=' read -r key value; do
      [[ -z "$key" ]] && continue
      [[ "$key" == \#* ]] && continue

      value="${value%$'\r'}"
      if [[ "$value" == \"*\" && "$value" == *\" ]]; then
        value="${value:1:-1}"
      fi

      if [[ -n "${!key+x}" ]]; then
        continue
      fi

      export "$key=$value"
    done < "$env_file"
  fi

  export POSTGRES_USER="${POSTGRES_USER:-postgres}"
  export POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-postgres}"
  export POSTGRES_DB="${POSTGRES_DB:-check_mate_dev}"
  export POSTGRES_PORT="${POSTGRES_PORT:-5432}"
  export CHECK_MATE_DATABASE_URL="${CHECK_MATE_DATABASE_URL:-host=localhost port=${POSTGRES_PORT} user=${POSTGRES_USER} password=${POSTGRES_PASSWORD} dbname=${POSTGRES_DB}}"
}
