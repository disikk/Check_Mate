ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS pots_participated_by_busted INTEGER[] NOT NULL DEFAULT '{}'::integer[];

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS pots_causing_bust INTEGER[] NOT NULL DEFAULT '{}'::integer[];

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS last_busting_pot_no INTEGER;

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS ko_winner_set TEXT[] NOT NULL DEFAULT ARRAY[]::text[];

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS ko_share_fraction_by_winner JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS elimination_certainty_state TEXT NOT NULL DEFAULT 'exact'
    CHECK (elimination_certainty_state IN ('exact', 'estimated', 'uncertain', 'inconsistent'));

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS ko_certainty_state TEXT NOT NULL DEFAULT 'uncertain'
    CHECK (ko_certainty_state IN ('exact', 'estimated', 'uncertain', 'inconsistent'));

UPDATE derived.hand_eliminations
SET
    pots_participated_by_busted = CASE
        WHEN COALESCE(array_length(resolved_by_pot_nos, 1), 0) > 0
        THEN resolved_by_pot_nos
        WHEN resolved_by_pot_no IS NOT NULL
        THEN ARRAY[resolved_by_pot_no]
        ELSE '{}'::integer[]
    END,
    pots_causing_bust = CASE
        WHEN ko_credit_pot_no IS NOT NULL
        THEN ARRAY[ko_credit_pot_no]
        WHEN resolved_by_pot_no IS NOT NULL
        THEN ARRAY[resolved_by_pot_no]
        ELSE '{}'::integer[]
    END,
    last_busting_pot_no = COALESCE(ko_credit_pot_no, resolved_by_pot_no),
    ko_winner_set = COALESCE(ko_involved_winners, ARRAY[]::text[]),
    elimination_certainty_state = 'exact',
    ko_certainty_state = COALESCE(certainty_state, 'uncertain');
