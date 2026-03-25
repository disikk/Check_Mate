ALTER TABLE core.hand_actions
    ADD COLUMN IF NOT EXISTS all_in_reason TEXT
        CHECK (all_in_reason IN (
            'voluntary',
            'call_exhausted',
            'raise_exhausted',
            'blind_exhausted',
            'ante_exhausted'
        ));

ALTER TABLE core.hand_actions
    ADD COLUMN IF NOT EXISTS forced_all_in_preflop BOOLEAN NOT NULL DEFAULT FALSE;
