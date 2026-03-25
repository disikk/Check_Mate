CREATE TABLE IF NOT EXISTS core.hand_pot_eligibility (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    pot_no INTEGER NOT NULL,
    seat_no INTEGER NOT NULL,
    UNIQUE (hand_id, pot_no, seat_no)
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_hand_pot_eligibility_hand_pot'
    ) THEN
        ALTER TABLE core.hand_pot_eligibility
            ADD CONSTRAINT fk_hand_pot_eligibility_hand_pot
            FOREIGN KEY (hand_id, pot_no)
            REFERENCES core.hand_pots(hand_id, pot_no);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_hand_pot_eligibility_hand_seat'
    ) THEN
        ALTER TABLE core.hand_pot_eligibility
            ADD CONSTRAINT fk_hand_pot_eligibility_hand_seat
            FOREIGN KEY (hand_id, seat_no)
            REFERENCES core.hand_seats(hand_id, seat_no);
    END IF;
END $$;

ALTER TABLE derived.hand_state_resolutions
    ADD COLUMN IF NOT EXISTS uncertain_reason_codes JSONB NOT NULL DEFAULT '[]'::jsonb;
