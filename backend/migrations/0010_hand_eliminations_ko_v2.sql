ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS resolved_by_pot_nos INTEGER[] NOT NULL DEFAULT '{}'::integer[];

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS ko_involved_winners TEXT[] NOT NULL DEFAULT ARRAY[]::text[];

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS hero_ko_share_total NUMERIC(12, 6);

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS joint_ko BOOLEAN NOT NULL DEFAULT FALSE;
