BEGIN;

-- Street-strength is a fully derived layer, so we clear legacy rows before
-- replacing the persisted contract in place.
DELETE FROM derived.street_hand_strength;

ALTER TABLE derived.street_hand_strength
    DROP CONSTRAINT IF EXISTS street_hand_strength_hand_id_seat_no_street_descriptor_version_key;

ALTER TABLE derived.street_hand_strength
    DROP COLUMN IF EXISTS pair_strength,
    DROP COLUMN IF EXISTS has_flush_draw,
    DROP COLUMN IF EXISTS has_backdoor_flush_draw,
    DROP COLUMN IF EXISTS has_open_ended,
    DROP COLUMN IF EXISTS has_gutshot,
    DROP COLUMN IF EXISTS has_double_gutshot,
    DROP COLUMN IF EXISTS has_pair_plus_draw,
    DROP COLUMN IF EXISTS has_overcards,
    DROP COLUMN IF EXISTS has_missed_draw_by_river,
    DROP COLUMN IF EXISTS descriptor_version;

ALTER TABLE derived.street_hand_strength
    ADD COLUMN IF NOT EXISTS made_hand_category TEXT,
    ADD COLUMN IF NOT EXISTS draw_category TEXT,
    ADD COLUMN IF NOT EXISTS overcards_count INTEGER,
    ADD COLUMN IF NOT EXISTS missed_flush_draw BOOLEAN,
    ADD COLUMN IF NOT EXISTS missed_straight_draw BOOLEAN;

ALTER TABLE derived.street_hand_strength
    ALTER COLUMN best_hand_class SET NOT NULL,
    ALTER COLUMN best_hand_rank_value SET NOT NULL,
    ALTER COLUMN made_hand_category SET NOT NULL,
    ALTER COLUMN draw_category SET NOT NULL,
    ALTER COLUMN overcards_count SET NOT NULL,
    ALTER COLUMN has_air SET NOT NULL,
    ALTER COLUMN missed_flush_draw SET NOT NULL,
    ALTER COLUMN missed_straight_draw SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'street_hand_strength_hand_id_seat_no_street_key'
    ) THEN
        ALTER TABLE derived.street_hand_strength
            ADD CONSTRAINT street_hand_strength_hand_id_seat_no_street_key
            UNIQUE (hand_id, seat_no, street);
    END IF;
END $$;

COMMIT;
