CREATE TABLE IF NOT EXISTS core.hand_summary_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id) ON DELETE CASCADE,
    seat_no INTEGER NOT NULL,
    player_name TEXT NOT NULL,
    position_marker TEXT CHECK (position_marker IN ('button', 'small blind', 'big blind')),
    outcome_kind TEXT NOT NULL CHECK (outcome_kind IN ('folded', 'showed_won', 'showed_lost', 'lost', 'mucked', 'won', 'collected')),
    folded_street TEXT CHECK (folded_street IN ('preflop', 'flop', 'turn', 'river')),
    shown_cards TEXT[],
    won_amount BIGINT,
    hand_class TEXT,
    raw_line TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, seat_no),
    FOREIGN KEY (hand_id, seat_no) REFERENCES core.hand_seats(hand_id, seat_no) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_hand_summary_results_hand_seat
    ON core.hand_summary_results(hand_id, seat_no);
