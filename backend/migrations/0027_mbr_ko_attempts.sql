CREATE TABLE IF NOT EXISTS derived.hand_ko_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    hero_seat_no INTEGER NOT NULL,
    target_seat_no INTEGER NOT NULL,
    target_player_name TEXT NOT NULL,
    attempt_kind TEXT NOT NULL CHECK (attempt_kind IN ('hero_push', 'hero_response', 'forced_auto_all_in')),
    street TEXT NOT NULL CHECK (street IN ('preflop', 'flop', 'turn', 'river', 'showdown', 'summary')),
    source_sequence_no INTEGER NOT NULL,
    is_forced_all_in BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, player_profile_id, hero_seat_no, target_seat_no)
);

CREATE INDEX IF NOT EXISTS idx_hand_ko_attempts_player_hand
    ON derived.hand_ko_attempts(player_profile_id, hand_id);

CREATE INDEX IF NOT EXISTS idx_hand_ko_attempts_hand_player
    ON derived.hand_ko_attempts(hand_id, player_profile_id);

CREATE TABLE IF NOT EXISTS derived.hand_ko_opportunities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    hero_seat_no INTEGER NOT NULL,
    target_seat_no INTEGER NOT NULL,
    target_player_name TEXT NOT NULL,
    opportunity_kind TEXT NOT NULL CHECK (opportunity_kind IN ('all_in', 'forced_auto_all_in')),
    street TEXT NOT NULL CHECK (street IN ('preflop', 'flop', 'turn', 'river', 'showdown', 'summary')),
    source_sequence_no INTEGER NOT NULL,
    is_forced_all_in BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, player_profile_id, hero_seat_no, target_seat_no)
);

CREATE INDEX IF NOT EXISTS idx_hand_ko_opportunities_player_hand
    ON derived.hand_ko_opportunities(player_profile_id, hand_id);

CREATE INDEX IF NOT EXISTS idx_hand_ko_opportunities_hand_player
    ON derived.hand_ko_opportunities(hand_id, player_profile_id);
