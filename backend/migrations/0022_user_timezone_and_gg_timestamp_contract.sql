ALTER TABLE auth.users
    ADD COLUMN IF NOT EXISTS timezone_name TEXT;

ALTER TABLE auth.users
    DROP CONSTRAINT IF EXISTS auth_users_timezone_name_not_blank;

ALTER TABLE auth.users
    ADD CONSTRAINT auth_users_timezone_name_not_blank CHECK (
        timezone_name IS NULL OR btrim(timezone_name) <> ''
    );

ALTER TABLE import.ingest_bundle_files
    DROP COLUMN IF EXISTS source_file_member_id;

ALTER TABLE import.file_fragments
    DROP COLUMN IF EXISTS source_file_member_id;

CREATE UNIQUE INDEX IF NOT EXISTS idx_source_file_members_source_member_index_unique
    ON import.source_file_members(source_file_id, member_index);

CREATE UNIQUE INDEX IF NOT EXISTS idx_file_fragments_source_fragment_unique
    ON import.file_fragments(source_file_id, fragment_index);

UPDATE core.tournaments
SET started_at = NULL,
    started_at_tz_provenance = 'gg_user_timezone_missing'
WHERE started_at_raw IS NOT NULL
  AND started_at_tz_provenance IS DISTINCT FROM 'gg_user_timezone_missing';

UPDATE core.hands
SET hand_started_at = NULL,
    hand_started_at_tz_provenance = 'gg_user_timezone_missing'
WHERE hand_started_at_raw IS NOT NULL
  AND hand_started_at_tz_provenance IS DISTINCT FROM 'gg_user_timezone_missing';

ALTER TABLE core.tournaments
    DROP CONSTRAINT IF EXISTS core_tournaments_started_at_tz_provenance_check;

ALTER TABLE core.tournaments
    ADD CONSTRAINT core_tournaments_started_at_tz_provenance_check CHECK (
        started_at_tz_provenance IS NULL
        OR started_at_tz_provenance IN (
            'gg_user_timezone',
            'gg_user_timezone_missing'
        )
    );

ALTER TABLE core.hands
    DROP CONSTRAINT IF EXISTS core_hands_hand_started_at_tz_provenance_check;

ALTER TABLE core.hands
    ADD CONSTRAINT core_hands_hand_started_at_tz_provenance_check CHECK (
        hand_started_at_tz_provenance IS NULL
        OR hand_started_at_tz_provenance IN (
            'gg_user_timezone',
            'gg_user_timezone_missing'
        )
    );
