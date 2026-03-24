#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

load_project_env
cd "$ROOT_DIR"

docker compose up -d postgres >/dev/null
printf '[db-bootstrap] waiting for postgres healthcheck...\n'
for _ in {1..30}; do
  if docker compose exec -T postgres pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

docker compose exec -T postgres pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null

# Keep reused Docker volumes aligned with the current .env auth contract.
printf '[db-bootstrap] syncing role password from .env\n'
docker compose exec -T postgres psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
  --set=role_password="$POSTGRES_PASSWORD" >/dev/null <<SQL
ALTER ROLE "$POSTGRES_USER" WITH PASSWORD :'role_password';
SQL

for file in backend/migrations/*.sql; do
  printf '[db-bootstrap] applying %s\n' "$(basename "$file")"
  docker compose exec -T postgres psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB" -f "/workspace/$file" >/dev/null
done

printf '[db-bootstrap] applying %s\n' "$(basename backend/seeds/0001_reference_data.sql)"
docker compose exec -T postgres psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB" -f /workspace/backend/seeds/0001_reference_data.sql >/dev/null

printf '[db-bootstrap] done\n'
printf '[db-bootstrap] CHECK_MATE_DATABASE_URL="%s"\n' "$CHECK_MATE_DATABASE_URL"
