ALTER TABLE derived.mbr_stage_resolution
    ADD COLUMN IF NOT EXISTS boundary_resolution_state TEXT NOT NULL DEFAULT 'uncertain'
        CHECK (boundary_resolution_state IN ('exact', 'estimated', 'uncertain', 'inconsistent')),
    ADD COLUMN IF NOT EXISTS boundary_candidate_count INTEGER NOT NULL DEFAULT 0
        CHECK (boundary_candidate_count >= 0),
    ADD COLUMN IF NOT EXISTS boundary_resolution_method TEXT,
    ADD COLUMN IF NOT EXISTS boundary_confidence_class TEXT;
