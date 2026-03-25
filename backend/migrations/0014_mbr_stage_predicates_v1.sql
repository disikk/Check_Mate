ALTER TABLE derived.mbr_stage_resolution
    ADD COLUMN IF NOT EXISTS is_ft_hand BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS ft_players_remaining_exact INTEGER
        CHECK (ft_players_remaining_exact IS NULL OR ft_players_remaining_exact BETWEEN 2 AND 9),
    ADD COLUMN IF NOT EXISTS is_stage_2 BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS is_stage_3_4 BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS is_stage_4_5 BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS is_stage_5_6 BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS is_stage_6_9 BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS is_boundary_hand BOOLEAN NOT NULL DEFAULT FALSE;

UPDATE derived.mbr_stage_resolution
SET
    is_ft_hand = played_ft_hand AND played_ft_hand_state = 'exact',
    ft_players_remaining_exact = CASE
        WHEN played_ft_hand IS TRUE AND played_ft_hand_state = 'exact' THEN ft_table_size
        ELSE NULL
    END,
    is_stage_2 = played_ft_hand IS TRUE AND played_ft_hand_state = 'exact' AND ft_table_size = 2,
    is_stage_3_4 = played_ft_hand IS TRUE AND played_ft_hand_state = 'exact' AND ft_table_size IN (3, 4),
    is_stage_4_5 = played_ft_hand IS TRUE AND played_ft_hand_state = 'exact' AND ft_table_size IN (4, 5),
    is_stage_5_6 = played_ft_hand IS TRUE AND played_ft_hand_state = 'exact' AND ft_table_size IN (5, 6),
    is_stage_6_9 = played_ft_hand IS TRUE AND played_ft_hand_state = 'exact' AND ft_table_size BETWEEN 6 AND 9,
    is_boundary_hand = entered_boundary_zone;
