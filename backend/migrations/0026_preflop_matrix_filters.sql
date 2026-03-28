CREATE TABLE IF NOT EXISTS derived.preflop_starting_hands (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    seat_no INTEGER NOT NULL,
    starter_hand_class TEXT NOT NULL,
    certainty_state TEXT NOT NULL DEFAULT 'exact' CHECK (certainty_state IN ('exact', 'estimated', 'uncertain', 'inconsistent')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, seat_no)
);

CREATE INDEX IF NOT EXISTS idx_preflop_starting_hands_lookup
    ON derived.preflop_starting_hands(hand_id, seat_no);

ALTER TABLE analytics.player_street_bool_features
    DROP CONSTRAINT IF EXISTS player_street_bool_features_street_check;
ALTER TABLE analytics.player_street_bool_features
    ADD CONSTRAINT player_street_bool_features_street_check
    CHECK (street IN ('preflop', 'flop', 'turn', 'river'));

ALTER TABLE analytics.player_street_num_features
    DROP CONSTRAINT IF EXISTS player_street_num_features_street_check;
ALTER TABLE analytics.player_street_num_features
    ADD CONSTRAINT player_street_num_features_street_check
    CHECK (street IN ('preflop', 'flop', 'turn', 'river'));

ALTER TABLE analytics.player_street_enum_features
    DROP CONSTRAINT IF EXISTS player_street_enum_features_street_check;
ALTER TABLE analytics.player_street_enum_features
    ADD CONSTRAINT player_street_enum_features_street_check
    CHECK (street IN ('preflop', 'flop', 'turn', 'river'));
