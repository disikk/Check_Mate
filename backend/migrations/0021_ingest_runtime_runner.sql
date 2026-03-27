CREATE TABLE IF NOT EXISTS import.ingest_bundles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    created_by_user_id UUID NOT NULL REFERENCES auth.users(id),
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'finalizing', 'succeeded', 'partial_success', 'failed')),
    error_code TEXT,
    error_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS import.ingest_bundle_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bundle_id UUID NOT NULL REFERENCES import.ingest_bundles(id) ON DELETE CASCADE,
    source_file_id UUID NOT NULL REFERENCES import.source_files(id),
    file_order_index INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (bundle_id, source_file_id),
    UNIQUE (bundle_id, file_order_index)
);

ALTER TABLE import.import_jobs
    ADD COLUMN IF NOT EXISTS bundle_id UUID REFERENCES import.ingest_bundles(id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS bundle_file_id UUID REFERENCES import.ingest_bundle_files(id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS job_kind TEXT NOT NULL DEFAULT 'legacy_import',
    ADD COLUMN IF NOT EXISTS claimed_by TEXT,
    ADD COLUMN IF NOT EXISTS claimed_at TIMESTAMPTZ;

ALTER TABLE import.import_jobs
    ALTER COLUMN source_file_id DROP NOT NULL;

UPDATE import.import_jobs
SET status = CASE status
    WHEN 'done' THEN 'succeeded'
    WHEN 'failed' THEN 'failed_terminal'
    ELSE status
END;

UPDATE import.job_attempts
SET status = CASE status
    WHEN 'done' THEN 'succeeded'
    WHEN 'failed' THEN 'failed_terminal'
    ELSE status
END;

ALTER TABLE import.import_jobs
    DROP CONSTRAINT IF EXISTS import_jobs_status_check,
    DROP CONSTRAINT IF EXISTS import_jobs_stage_check,
    DROP CONSTRAINT IF EXISTS import_jobs_job_kind_check,
    DROP CONSTRAINT IF EXISTS import_jobs_kind_reference_check;

ALTER TABLE import.job_attempts
    DROP CONSTRAINT IF EXISTS job_attempts_status_check,
    DROP CONSTRAINT IF EXISTS job_attempts_stage_check;

ALTER TABLE import.import_jobs
    ADD CONSTRAINT import_jobs_status_check CHECK (
        status IN (
            'queued',
            'running',
            'done',
            'failed',
            'succeeded',
            'failed_retriable',
            'failed_terminal',
            'cancelled'
        )
    ),
    ADD CONSTRAINT import_jobs_stage_check CHECK (
        stage IN (
            'queued',
            'register',
            'split',
            'parse',
            'normalize',
            'derive',
            'persist',
            'materialize_refresh',
            'done',
            'failed'
        )
    ),
    ADD CONSTRAINT import_jobs_job_kind_check CHECK (
        job_kind IN ('legacy_import', 'file_ingest', 'bundle_finalize')
    ),
    ADD CONSTRAINT import_jobs_kind_reference_check CHECK (
        (job_kind = 'legacy_import' AND source_file_id IS NOT NULL)
        OR (job_kind = 'file_ingest' AND bundle_id IS NOT NULL AND bundle_file_id IS NOT NULL)
        OR (job_kind = 'bundle_finalize' AND bundle_id IS NOT NULL AND bundle_file_id IS NULL)
    );

ALTER TABLE import.job_attempts
    ADD CONSTRAINT job_attempts_status_check CHECK (
        status IN (
            'running',
            'done',
            'failed',
            'succeeded',
            'failed_retriable',
            'failed_terminal',
            'cancelled'
        )
    ),
    ADD CONSTRAINT job_attempts_stage_check CHECK (
        stage IN (
            'queued',
            'register',
            'split',
            'parse',
            'normalize',
            'derive',
            'persist',
            'materialize_refresh',
            'done',
            'failed'
        )
    );

CREATE INDEX IF NOT EXISTS idx_ingest_bundle_files_bundle
    ON import.ingest_bundle_files(bundle_id, file_order_index);

CREATE INDEX IF NOT EXISTS idx_import_jobs_bundle_status
    ON import.import_jobs(bundle_id, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_import_jobs_claim
    ON import.import_jobs(status, job_kind, created_at ASC);

CREATE UNIQUE INDEX IF NOT EXISTS uniq_import_jobs_bundle_finalize
    ON import.import_jobs(bundle_id)
    WHERE job_kind = 'bundle_finalize';

CREATE UNIQUE INDEX IF NOT EXISTS uniq_import_jobs_bundle_file_ingest
    ON import.import_jobs(bundle_file_id)
    WHERE job_kind = 'file_ingest';
