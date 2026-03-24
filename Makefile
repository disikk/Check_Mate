.PHONY: db-up db-down db-bootstrap db-reset db-psql backend-test frontend-install frontend-dev frontend-build bootstrap verify import-ts import-hh

db-up:
	bash scripts/db_up.sh

db-down:
	bash scripts/db_down.sh

db-bootstrap:
	bash scripts/db_bootstrap.sh

bootstrap:
	bash scripts/db_up.sh
	bash scripts/db_bootstrap.sh

db-reset:
	bash scripts/db_down.sh --volumes
	bash scripts/db_up.sh
	bash scripts/db_bootstrap.sh

db-psql:
	bash scripts/db_psql.sh

backend-test:
	bash scripts/backend_test.sh

frontend-install:
	npm install

frontend-dev:
	npm run dev

frontend-build:
	npm run build

verify:
	bash scripts/backend_test.sh
	npm run build

import-ts:
	bash scripts/import_fixture.sh backend/fixtures/mbr/ts/GG20260316\ -\ Tournament\ #271770266\ -\ Mystery\ Battle\ Royale\ 25.txt

import-hh:
	bash scripts/import_fixture.sh backend/fixtures/mbr/hh/GG20260316-0344\ -\ Mystery\ Battle\ Royale\ 25.txt
