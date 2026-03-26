DO $migration$
BEGIN
    IF to_regclass('core.hand_positions_v1') IS NOT NULL
       AND EXISTS (
           SELECT 1
           FROM information_schema.columns
           WHERE table_schema = 'core'
             AND table_name = 'hand_positions'
             AND column_name = 'position_index'
       )
    THEN
        EXECUTE 'DROP TABLE core.hand_positions_v1';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'core'
          AND table_name = 'hand_positions'
          AND column_name = 'position_code'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'core'
          AND table_name = 'hand_positions'
          AND column_name = 'position_index'
    ) THEN
        EXECUTE 'ALTER TABLE core.hand_positions RENAME TO hand_positions_v1';

        EXECUTE $sql$
            CREATE TABLE core.hand_positions (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                hand_id UUID NOT NULL REFERENCES core.hands(id) ON DELETE CASCADE,
                seat_no INTEGER NOT NULL,
                position_index INTEGER NOT NULL CHECK (position_index BETWEEN 1 AND 10),
                position_label TEXT NOT NULL CHECK (
                    position_label IN (
                        'BTN',
                        'SB',
                        'BB',
                        'UTG',
                        'UTG+1',
                        'UTG+2',
                        'MP',
                        'MP+1',
                        'LJ',
                        'HJ',
                        'CO'
                    )
                ),
                preflop_act_order_index INTEGER NOT NULL,
                postflop_act_order_index INTEGER NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (hand_id, seat_no),
                FOREIGN KEY (hand_id, seat_no) REFERENCES core.hand_seats(hand_id, seat_no) ON DELETE CASCADE
            )
        $sql$;

        EXECUTE $sql$
            WITH active_counts AS (
                SELECT hand_id, COUNT(*)::INTEGER AS active_count
                FROM core.hand_positions_v1
                GROUP BY hand_id
            )
            INSERT INTO core.hand_positions (
                id,
                hand_id,
                seat_no,
                position_index,
                position_label,
                preflop_act_order_index,
                postflop_act_order_index,
                created_at
            )
            SELECT
                hp.id,
                hp.hand_id,
                hp.seat_no,
                CASE ac.active_count
                    WHEN 2 THEN CASE hp.position_code
                        WHEN 'BTN' THEN 1
                        WHEN 'BB' THEN 2
                    END
                    WHEN 3 THEN CASE hp.position_code
                        WHEN 'BTN' THEN 1
                        WHEN 'SB' THEN 2
                        WHEN 'BB' THEN 3
                    END
                    WHEN 4 THEN CASE hp.position_code
                        WHEN 'BTN' THEN 1
                        WHEN 'SB' THEN 2
                        WHEN 'BB' THEN 3
                        WHEN 'CO' THEN 4
                    END
                    WHEN 5 THEN CASE hp.position_code
                        WHEN 'BTN' THEN 1
                        WHEN 'SB' THEN 2
                        WHEN 'BB' THEN 3
                        WHEN 'HJ' THEN 4
                        WHEN 'CO' THEN 5
                    END
                    WHEN 6 THEN CASE hp.position_code
                        WHEN 'BTN' THEN 1
                        WHEN 'SB' THEN 2
                        WHEN 'BB' THEN 3
                        WHEN 'LJ' THEN 4
                        WHEN 'HJ' THEN 5
                        WHEN 'CO' THEN 6
                    END
                    WHEN 7 THEN CASE hp.position_code
                        WHEN 'BTN' THEN 1
                        WHEN 'SB' THEN 2
                        WHEN 'BB' THEN 3
                        WHEN 'MP' THEN 4
                        WHEN 'LJ' THEN 5
                        WHEN 'HJ' THEN 6
                        WHEN 'CO' THEN 7
                    END
                    WHEN 8 THEN CASE hp.position_code
                        WHEN 'BTN' THEN 1
                        WHEN 'SB' THEN 2
                        WHEN 'BB' THEN 3
                        WHEN 'UTG+1' THEN 4
                        WHEN 'MP' THEN 5
                        WHEN 'LJ' THEN 6
                        WHEN 'HJ' THEN 7
                        WHEN 'CO' THEN 8
                    END
                    WHEN 9 THEN CASE hp.position_code
                        WHEN 'BTN' THEN 1
                        WHEN 'SB' THEN 2
                        WHEN 'BB' THEN 3
                        WHEN 'UTG' THEN 4
                        WHEN 'UTG+1' THEN 5
                        WHEN 'MP' THEN 6
                        WHEN 'LJ' THEN 7
                        WHEN 'HJ' THEN 8
                        WHEN 'CO' THEN 9
                    END
                END AS position_index,
                hp.position_code AS position_label,
                hp.preflop_act_order_index,
                hp.postflop_act_order_index,
                hp.created_at
            FROM core.hand_positions_v1 hp
            INNER JOIN active_counts ac
                ON ac.hand_id = hp.hand_id
        $sql$;

        EXECUTE 'DROP TABLE core.hand_positions_v1';
    END IF;
END
$migration$;

CREATE INDEX IF NOT EXISTS idx_hand_positions_hand_seat
    ON core.hand_positions(hand_id, seat_no);
