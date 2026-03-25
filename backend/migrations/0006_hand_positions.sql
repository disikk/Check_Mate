CREATE TABLE IF NOT EXISTS core.hand_positions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id) ON DELETE CASCADE,
    seat_no INTEGER NOT NULL,
    position_code TEXT NOT NULL CHECK (position_code IN ('BTN', 'SB', 'BB', 'UTG', 'UTG+1', 'MP', 'LJ', 'HJ', 'CO')),
    preflop_act_order_index INTEGER NOT NULL,
    postflop_act_order_index INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, seat_no),
    FOREIGN KEY (hand_id, seat_no) REFERENCES core.hand_seats(hand_id, seat_no) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_hand_positions_hand_seat
    ON core.hand_positions(hand_id, seat_no);
