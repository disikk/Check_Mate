CREATE TABLE IF NOT EXISTS derived.mbr_tournament_ft_helper (
    tournament_id UUID NOT NULL REFERENCES core.tournaments(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    reached_ft_exact BOOLEAN NOT NULL DEFAULT FALSE,
    first_ft_hand_id UUID REFERENCES core.hands(id),
    first_ft_hand_started_local TIMESTAMP,
    first_ft_table_size INTEGER,
    ft_started_incomplete BOOLEAN,
    deepest_ft_size_reached INTEGER,
    hero_ft_entry_stack_chips BIGINT,
    hero_ft_entry_stack_bb NUMERIC(18, 6),
    entered_boundary_zone BOOLEAN NOT NULL DEFAULT FALSE,
    boundary_resolution_state TEXT NOT NULL DEFAULT 'uncertain'
        CHECK (boundary_resolution_state IN ('exact', 'estimated', 'uncertain', 'inconsistent')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tournament_id, player_profile_id)
);
