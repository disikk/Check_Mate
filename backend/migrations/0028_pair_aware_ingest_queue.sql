ALTER TABLE import.import_jobs
    ADD COLUMN IF NOT EXISTS depends_on_job_id UUID REFERENCES import.import_jobs(id);

CREATE INDEX IF NOT EXISTS idx_import_jobs_claim_dependency
    ON import.import_jobs(status, job_kind, depends_on_job_id, created_at ASC);
