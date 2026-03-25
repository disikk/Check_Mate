CREATE TABLE IF NOT EXISTS core.player_aliases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    room TEXT NOT NULL,
    alias TEXT NOT NULL,
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    source TEXT NOT NULL DEFAULT 'manual',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (player_profile_id, room, alias)
);

CREATE TABLE IF NOT EXISTS import.source_file_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_file_id UUID NOT NULL REFERENCES import.source_files(id),
    member_index INTEGER NOT NULL,
    member_path TEXT NOT NULL,
    member_kind TEXT NOT NULL CHECK (member_kind IN ('hh', 'ts', 'archive')),
    sha256 CHAR(64) NOT NULL,
    byte_size BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (source_file_id, member_index),
    UNIQUE (source_file_id, sha256)
);

CREATE TABLE IF NOT EXISTS import.job_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    import_job_id UUID NOT NULL REFERENCES import.import_jobs(id) ON DELETE CASCADE,
    attempt_no INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'done', 'failed', 'cancelled')),
    stage TEXT NOT NULL CHECK (stage IN ('queued', 'split', 'parse', 'normalize', 'derive', 'persist', 'done', 'failed')),
    error_code TEXT,
    error_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (import_job_id, attempt_no)
);

CREATE TABLE IF NOT EXISTS analytics.feature_catalog (
    feature_key TEXT NOT NULL,
    feature_version TEXT NOT NULL,
    table_family TEXT NOT NULL CHECK (table_family IN ('bool', 'num', 'enum')),
    value_kind TEXT NOT NULL CHECK (value_kind IN ('bool', 'double', 'enum')),
    exactness_class TEXT NOT NULL CHECK (exactness_class IN ('exact', 'estimated', 'uncertain', 'fun')),
    description TEXT NOT NULL DEFAULT '',
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (feature_key, feature_version)
);

CREATE TABLE IF NOT EXISTS analytics.stat_catalog (
    stat_key TEXT PRIMARY KEY,
    stat_family TEXT NOT NULL,
    exactness_class TEXT NOT NULL CHECK (exactness_class IN ('exact', 'estimated', 'expression', 'fun')),
    output_kind TEXT NOT NULL CHECK (output_kind IN ('double', 'integer', 'percent', 'enum')),
    description TEXT NOT NULL DEFAULT '',
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS analytics.stat_dependencies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    stat_key TEXT NOT NULL REFERENCES analytics.stat_catalog(stat_key) ON DELETE CASCADE,
    dependency_kind TEXT NOT NULL CHECK (dependency_kind IN ('feature', 'summary_field', 'coverage_metric')),
    dependency_key TEXT NOT NULL,
    dependency_version TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (stat_key, dependency_kind, dependency_key, dependency_version)
);

CREATE TABLE IF NOT EXISTS analytics.materialization_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_kind TEXT NOT NULL CHECK (target_kind IN ('feature', 'stat')),
    target_key TEXT NOT NULL,
    target_version TEXT NOT NULL DEFAULT '',
    policy_code TEXT NOT NULL,
    refresh_mode TEXT NOT NULL CHECK (refresh_mode IN ('full_refresh', 'incremental')),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (target_kind, target_key, target_version, policy_code)
);

ALTER TABLE core.tournaments
    ADD COLUMN IF NOT EXISTS started_at_raw TEXT,
    ADD COLUMN IF NOT EXISTS started_at_local TIMESTAMP,
    ADD COLUMN IF NOT EXISTS started_at_tz_provenance TEXT;

ALTER TABLE core.hands
    ADD COLUMN IF NOT EXISTS hand_started_at_raw TEXT,
    ADD COLUMN IF NOT EXISTS hand_started_at_local TIMESTAMP,
    ADD COLUMN IF NOT EXISTS hand_started_at_tz_provenance TEXT;

DROP TABLE IF EXISTS tmp_fragment_dedupe_map;
DROP TABLE IF EXISTS tmp_source_file_dedupe_map;

CREATE TEMP TABLE tmp_source_file_dedupe_map AS
WITH ranked AS (
    SELECT
        id,
        player_profile_id,
        room,
        file_kind,
        sha256,
        ROW_NUMBER() OVER (
            PARTITION BY player_profile_id, room, file_kind, sha256
            ORDER BY created_at DESC, id DESC
        ) AS rn
    FROM import.source_files
),
canonical AS (
    SELECT
        player_profile_id,
        room,
        file_kind,
        sha256,
        id AS canonical_id
    FROM ranked
    WHERE rn = 1
)
SELECT
    ranked.id AS duplicate_id,
    canonical.canonical_id
FROM ranked
INNER JOIN canonical
    ON canonical.player_profile_id = ranked.player_profile_id
   AND canonical.room = ranked.room
   AND canonical.file_kind = ranked.file_kind
   AND canonical.sha256 = ranked.sha256
WHERE ranked.rn > 1;

CREATE TEMP TABLE tmp_fragment_dedupe_map AS
SELECT
    duplicate_fragment.id AS duplicate_fragment_id,
    canonical_fragment.id AS canonical_fragment_id
FROM tmp_source_file_dedupe_map AS map
INNER JOIN import.file_fragments AS duplicate_fragment
    ON duplicate_fragment.source_file_id = map.duplicate_id
INNER JOIN import.file_fragments AS canonical_fragment
    ON canonical_fragment.source_file_id = map.canonical_id
   AND canonical_fragment.kind = duplicate_fragment.kind
   AND COALESCE(canonical_fragment.external_hand_id, '') = COALESCE(duplicate_fragment.external_hand_id, '')
   AND canonical_fragment.sha256 = duplicate_fragment.sha256;

UPDATE core.tournaments AS tournaments
SET source_summary_file_id = map.canonical_id
FROM tmp_source_file_dedupe_map AS map
WHERE tournaments.source_summary_file_id = map.duplicate_id;

UPDATE core.hands AS hands
SET source_file_id = map.canonical_id
FROM tmp_source_file_dedupe_map AS map
WHERE hands.source_file_id = map.duplicate_id;

UPDATE import.import_jobs AS jobs
SET source_file_id = map.canonical_id
FROM tmp_source_file_dedupe_map AS map
WHERE jobs.source_file_id = map.duplicate_id;

UPDATE core.parse_issues AS issues
SET source_file_id = map.canonical_id
FROM tmp_source_file_dedupe_map AS map
WHERE issues.source_file_id = map.duplicate_id;

UPDATE core.hands AS hands
SET raw_fragment_id = fragment_map.canonical_fragment_id
FROM tmp_fragment_dedupe_map AS fragment_map
WHERE hands.raw_fragment_id = fragment_map.duplicate_fragment_id;

UPDATE core.parse_issues AS issues
SET fragment_id = fragment_map.canonical_fragment_id
FROM tmp_fragment_dedupe_map AS fragment_map
WHERE issues.fragment_id = fragment_map.duplicate_fragment_id;

DELETE FROM import.file_fragments AS fragments
USING tmp_source_file_dedupe_map AS map
WHERE fragments.source_file_id = map.duplicate_id;

DELETE FROM import.import_jobs AS jobs
USING tmp_source_file_dedupe_map AS map
WHERE jobs.source_file_id = map.duplicate_id;

DELETE FROM import.source_files AS files
USING tmp_source_file_dedupe_map AS map
WHERE files.id = map.duplicate_id;

CREATE UNIQUE INDEX IF NOT EXISTS idx_player_aliases_primary_per_profile
    ON core.player_aliases(player_profile_id)
    WHERE is_primary;

CREATE UNIQUE INDEX IF NOT EXISTS idx_player_aliases_lookup
    ON core.player_aliases(organization_id, room, alias);

CREATE UNIQUE INDEX IF NOT EXISTS idx_source_files_player_room_kind_sha
    ON import.source_files(player_profile_id, room, file_kind, sha256);

CREATE UNIQUE INDEX IF NOT EXISTS idx_file_fragments_source_sha
    ON import.file_fragments(source_file_id, sha256);

CREATE INDEX IF NOT EXISTS idx_source_file_members_lookup
    ON import.source_file_members(source_file_id, member_index);

CREATE INDEX IF NOT EXISTS idx_job_attempts_import_job
    ON import.job_attempts(import_job_id, attempt_no);

CREATE INDEX IF NOT EXISTS idx_feature_catalog_lookup
    ON analytics.feature_catalog(feature_version, table_family, feature_key);

CREATE INDEX IF NOT EXISTS idx_stat_dependencies_lookup
    ON analytics.stat_dependencies(stat_key, dependency_kind, dependency_key);

CREATE INDEX IF NOT EXISTS idx_materialization_policies_lookup
    ON analytics.materialization_policies(target_kind, target_key, target_version);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_hand_hole_cards_hand_seat'
    ) THEN
        ALTER TABLE core.hand_hole_cards
            ADD CONSTRAINT fk_hand_hole_cards_hand_seat
            FOREIGN KEY (hand_id, seat_no)
            REFERENCES core.hand_seats(hand_id, seat_no);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_hand_showdowns_hand_seat'
    ) THEN
        ALTER TABLE core.hand_showdowns
            ADD CONSTRAINT fk_hand_showdowns_hand_seat
            FOREIGN KEY (hand_id, seat_no)
            REFERENCES core.hand_seats(hand_id, seat_no);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_hand_returns_hand_seat'
    ) THEN
        ALTER TABLE core.hand_returns
            ADD CONSTRAINT fk_hand_returns_hand_seat
            FOREIGN KEY (hand_id, seat_no)
            REFERENCES core.hand_seats(hand_id, seat_no);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_hand_pot_contributions_hand_pot'
    ) THEN
        ALTER TABLE core.hand_pot_contributions
            ADD CONSTRAINT fk_hand_pot_contributions_hand_pot
            FOREIGN KEY (hand_id, pot_no)
            REFERENCES core.hand_pots(hand_id, pot_no);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_hand_pot_contributions_hand_seat'
    ) THEN
        ALTER TABLE core.hand_pot_contributions
            ADD CONSTRAINT fk_hand_pot_contributions_hand_seat
            FOREIGN KEY (hand_id, seat_no)
            REFERENCES core.hand_seats(hand_id, seat_no);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_hand_pot_winners_hand_pot'
    ) THEN
        ALTER TABLE core.hand_pot_winners
            ADD CONSTRAINT fk_hand_pot_winners_hand_pot
            FOREIGN KEY (hand_id, pot_no)
            REFERENCES core.hand_pots(hand_id, pot_no);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_hand_pot_winners_hand_seat'
    ) THEN
        ALTER TABLE core.hand_pot_winners
            ADD CONSTRAINT fk_hand_pot_winners_hand_seat
            FOREIGN KEY (hand_id, seat_no)
            REFERENCES core.hand_seats(hand_id, seat_no);
    END IF;
END $$;

DROP TABLE IF EXISTS tmp_fragment_dedupe_map;
DROP TABLE IF EXISTS tmp_source_file_dedupe_map;
