-- F2-T2: Stable ordering substrate for hands within a tournament.
-- Replaces fragile string-based ORDER BY hand_started_at_local with deterministic integer order.
-- Computed at import time from chronological sort (timestamp + external_hand_id tiebreak).

ALTER TABLE core.hands
    ADD COLUMN IF NOT EXISTS tournament_hand_order INT;

-- Index for efficient ORDER BY within tournament scope
CREATE INDEX IF NOT EXISTS idx_hands_tournament_order
    ON core.hands(tournament_id, tournament_hand_order)
    WHERE tournament_hand_order IS NOT NULL;

-- Backfill existing hands with order derived from current string-based sort
WITH ordered AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY tournament_id
            ORDER BY hand_started_at_local NULLS LAST,
                     external_hand_id,
                     id
        )::int AS computed_order
    FROM core.hands
)
UPDATE core.hands h
SET tournament_hand_order = ordered.computed_order
FROM ordered
WHERE h.id = ordered.id
  AND h.tournament_hand_order IS NULL;
