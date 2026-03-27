ALTER TABLE derived.hand_state_resolutions
    ADD COLUMN IF NOT EXISTS settlement_state TEXT NOT NULL DEFAULT 'exact';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'hand_state_resolutions_settlement_state_check'
    ) THEN
        ALTER TABLE derived.hand_state_resolutions
            ADD CONSTRAINT hand_state_resolutions_settlement_state_check
            CHECK (settlement_state IN ('exact', 'estimated', 'uncertain', 'inconsistent'));
    END IF;
END $$;

ALTER TABLE derived.hand_state_resolutions
    ADD COLUMN IF NOT EXISTS settlement JSONB NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE derived.hand_state_resolutions
    ADD COLUMN IF NOT EXISTS invariant_issues JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE derived.hand_state_resolutions
    DROP COLUMN IF EXISTS invariant_errors;

ALTER TABLE derived.hand_state_resolutions
    DROP COLUMN IF EXISTS uncertain_reason_codes;
