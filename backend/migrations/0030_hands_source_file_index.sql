-- Index on core.hands.source_file_id for scoped finalize tournament lookup.
-- Without this index, the materialize_refresh query in bundle finalize
-- does a seq scan on core.hands joined with import.import_jobs, which
-- becomes extremely slow on large imports (220K+ hands).
CREATE INDEX IF NOT EXISTS idx_hands_source_file
    ON core.hands (source_file_id);
