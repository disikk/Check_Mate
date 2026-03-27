INSERT INTO import.source_file_members (
    source_file_id,
    member_index,
    member_path,
    member_kind,
    sha256,
    byte_size
)
SELECT
    files.id,
    0,
    files.original_filename,
    files.file_kind,
    files.sha256,
    files.byte_size
FROM import.source_files AS files
WHERE NOT EXISTS (
    SELECT 1
    FROM import.source_file_members AS members
    WHERE members.source_file_id = files.id
      AND members.member_index = 0
);

ALTER TABLE import.ingest_bundle_files
    ADD COLUMN IF NOT EXISTS source_file_member_id UUID REFERENCES import.source_file_members(id);

UPDATE import.ingest_bundle_files AS bundle_files
SET source_file_member_id = members.id
FROM import.source_file_members AS members
WHERE members.source_file_id = bundle_files.source_file_id
  AND members.member_index = 0
  AND bundle_files.source_file_member_id IS NULL;

ALTER TABLE import.ingest_bundle_files
    ALTER COLUMN source_file_member_id SET NOT NULL;

ALTER TABLE import.ingest_bundle_files
    DROP CONSTRAINT IF EXISTS ingest_bundle_files_bundle_id_source_file_id_key;

CREATE UNIQUE INDEX IF NOT EXISTS uniq_ingest_bundle_files_bundle_member
    ON import.ingest_bundle_files(bundle_id, source_file_member_id);

ALTER TABLE import.import_jobs
    ADD COLUMN IF NOT EXISTS source_file_member_id UUID REFERENCES import.source_file_members(id);

UPDATE import.import_jobs AS jobs
SET source_file_member_id = bundle_files.source_file_member_id
FROM import.ingest_bundle_files AS bundle_files
WHERE jobs.bundle_file_id = bundle_files.id
  AND jobs.source_file_member_id IS NULL;

UPDATE import.import_jobs AS jobs
SET source_file_member_id = members.id
FROM import.source_file_members AS members
WHERE jobs.source_file_id = members.source_file_id
  AND members.member_index = 0
  AND jobs.source_file_member_id IS NULL;

ALTER TABLE import.import_jobs
    DROP CONSTRAINT IF EXISTS import_jobs_kind_reference_check;

ALTER TABLE import.import_jobs
    ADD CONSTRAINT import_jobs_kind_reference_check CHECK (
        (job_kind = 'legacy_import' AND source_file_id IS NOT NULL)
        OR (
            job_kind = 'file_ingest'
            AND bundle_id IS NOT NULL
            AND bundle_file_id IS NOT NULL
            AND source_file_id IS NOT NULL
            AND source_file_member_id IS NOT NULL
        )
        OR (
            job_kind = 'bundle_finalize'
            AND bundle_id IS NOT NULL
            AND bundle_file_id IS NULL
        )
    );

ALTER TABLE import.file_fragments
    ADD COLUMN IF NOT EXISTS source_file_member_id UUID REFERENCES import.source_file_members(id);

UPDATE import.file_fragments AS fragments
SET source_file_member_id = members.id
FROM import.source_file_members AS members
WHERE members.source_file_id = fragments.source_file_id
  AND members.member_index = 0
  AND fragments.source_file_member_id IS NULL;

ALTER TABLE import.file_fragments
    ALTER COLUMN source_file_member_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_file_fragments_member_lookup
    ON import.file_fragments(source_file_member_id, fragment_index);

CREATE TABLE IF NOT EXISTS import.ingest_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sequence_no BIGSERIAL NOT NULL UNIQUE,
    bundle_id UUID NOT NULL REFERENCES import.ingest_bundles(id) ON DELETE CASCADE,
    bundle_file_id UUID REFERENCES import.ingest_bundle_files(id) ON DELETE CASCADE,
    event_kind TEXT NOT NULL CHECK (
        event_kind IN (
            'bundle_updated',
            'file_updated',
            'diagnostic_logged',
            'bundle_terminal'
        )
    ),
    message TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ingest_events_bundle_sequence
    ON import.ingest_events(bundle_id, sequence_no);

CREATE INDEX IF NOT EXISTS idx_ingest_events_bundle_file_sequence
    ON import.ingest_events(bundle_file_id, sequence_no)
    WHERE bundle_file_id IS NOT NULL;
