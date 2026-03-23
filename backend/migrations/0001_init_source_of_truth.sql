CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE SCHEMA IF NOT EXISTS auth;
CREATE SCHEMA IF NOT EXISTS org;
CREATE SCHEMA IF NOT EXISTS import;
CREATE SCHEMA IF NOT EXISTS core;
CREATE SCHEMA IF NOT EXISTS derived;
CREATE SCHEMA IF NOT EXISTS analytics;

CREATE TABLE IF NOT EXISTS auth.users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    auth_provider TEXT NOT NULL DEFAULT 'local',
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('invited', 'active', 'disabled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (
        (auth_provider = 'local' AND password_hash IS NOT NULL)
        OR auth_provider <> 'local'
    )
);

CREATE TABLE IF NOT EXISTS org.organizations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS org.organization_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    user_id UUID NOT NULL REFERENCES auth.users(id),
    role TEXT NOT NULL CHECK (role IN ('student', 'coach', 'org_admin', 'super_admin')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, user_id)
);

CREATE TABLE IF NOT EXISTS org.study_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    name TEXT NOT NULL,
    created_by_user_id UUID NOT NULL REFERENCES auth.users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS org.study_group_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    group_id UUID NOT NULL REFERENCES org.study_groups(id),
    user_id UUID NOT NULL REFERENCES auth.users(id),
    membership_role TEXT NOT NULL CHECK (membership_role IN ('member', 'coach')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (group_id, user_id)
);

CREATE TABLE IF NOT EXISTS core.rooms (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS core.formats (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id UUID NOT NULL REFERENCES core.rooms(id),
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    max_players INTEGER,
    UNIQUE (room_id, code)
);

CREATE TABLE IF NOT EXISTS core.player_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    owner_user_id UUID NOT NULL REFERENCES auth.users(id),
    room TEXT NOT NULL,
    network TEXT NOT NULL,
    screen_name TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (room, screen_name, organization_id)
);

CREATE TABLE IF NOT EXISTS import.source_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    uploaded_by_user_id UUID NOT NULL REFERENCES auth.users(id),
    owner_user_id UUID NOT NULL REFERENCES auth.users(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    room TEXT NOT NULL,
    file_kind TEXT NOT NULL CHECK (file_kind IN ('hh', 'ts', 'archive')),
    sha256 CHAR(64) NOT NULL,
    original_filename TEXT NOT NULL,
    byte_size BIGINT NOT NULL,
    storage_uri TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS import.import_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    source_file_id UUID NOT NULL REFERENCES import.source_files(id),
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'done', 'failed', 'cancelled')),
    stage TEXT NOT NULL CHECK (stage IN ('queued', 'split', 'parse', 'normalize', 'derive', 'persist', 'done', 'failed')),
    error_code TEXT,
    error_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS import.file_fragments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_file_id UUID NOT NULL REFERENCES import.source_files(id),
    fragment_index INTEGER NOT NULL,
    external_hand_id TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('hand', 'summary')),
    raw_text TEXT NOT NULL,
    sha256 CHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (source_file_id, fragment_index)
);

CREATE TABLE IF NOT EXISTS core.tournaments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    room_id UUID NOT NULL REFERENCES core.rooms(id),
    format_id UUID NOT NULL REFERENCES core.formats(id),
    external_tournament_id TEXT NOT NULL,
    buyin_total NUMERIC(12, 2) NOT NULL,
    buyin_prize_component NUMERIC(12, 2) NOT NULL,
    buyin_bounty_component NUMERIC(12, 2) NOT NULL,
    fee_component NUMERIC(12, 2) NOT NULL,
    currency TEXT NOT NULL,
    max_players INTEGER NOT NULL,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    source_summary_file_id UUID REFERENCES import.source_files(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (player_profile_id, room_id, external_tournament_id)
);

CREATE TABLE IF NOT EXISTS core.tournament_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID NOT NULL REFERENCES core.tournaments(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    finish_place INTEGER,
    regular_prize_money NUMERIC(12, 2),
    total_payout_money NUMERIC(12, 2),
    mystery_money_total NUMERIC(12, 2),
    is_winner BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tournament_id, player_profile_id)
);

CREATE TABLE IF NOT EXISTS core.hands (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    tournament_id UUID NOT NULL REFERENCES core.tournaments(id),
    source_file_id UUID NOT NULL REFERENCES import.source_files(id),
    external_hand_id TEXT NOT NULL,
    hand_started_at TIMESTAMPTZ,
    table_name TEXT NOT NULL,
    table_max_seats INTEGER NOT NULL,
    dealer_seat_no INTEGER,
    small_blind BIGINT NOT NULL,
    big_blind BIGINT NOT NULL,
    ante BIGINT NOT NULL DEFAULT 0,
    currency TEXT NOT NULL,
    raw_fragment_id UUID REFERENCES import.file_fragments(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (player_profile_id, external_hand_id)
);

CREATE TABLE IF NOT EXISTS core.hand_seats (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    seat_no INTEGER NOT NULL,
    player_name TEXT NOT NULL,
    player_profile_id UUID REFERENCES core.player_profiles(id),
    starting_stack BIGINT NOT NULL,
    is_hero BOOLEAN NOT NULL DEFAULT FALSE,
    is_button BOOLEAN NOT NULL DEFAULT FALSE,
    is_sitting_out BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, seat_no)
);

CREATE TABLE IF NOT EXISTS core.hand_hole_cards (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    seat_no INTEGER NOT NULL,
    card1 TEXT,
    card2 TEXT,
    known_to_hero BOOLEAN NOT NULL DEFAULT FALSE,
    known_at_showdown BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, seat_no)
);

CREATE TABLE IF NOT EXISTS core.hand_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    sequence_no INTEGER NOT NULL,
    street TEXT NOT NULL CHECK (street IN ('preflop', 'flop', 'turn', 'river', 'showdown', 'summary')),
    seat_no INTEGER,
    action_type TEXT NOT NULL,
    raw_amount BIGINT,
    to_amount BIGINT,
    is_all_in BOOLEAN NOT NULL DEFAULT FALSE,
    references_previous_bet BOOLEAN NOT NULL DEFAULT FALSE,
    raw_line TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, sequence_no)
);

CREATE TABLE IF NOT EXISTS core.hand_boards (
    hand_id UUID PRIMARY KEY REFERENCES core.hands(id),
    flop1 TEXT,
    flop2 TEXT,
    flop3 TEXT,
    turn TEXT,
    river TEXT
);

CREATE TABLE IF NOT EXISTS core.hand_pots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    pot_no INTEGER NOT NULL,
    pot_type TEXT NOT NULL CHECK (pot_type IN ('main', 'side')),
    amount BIGINT NOT NULL,
    UNIQUE (hand_id, pot_no)
);

CREATE TABLE IF NOT EXISTS core.hand_pot_contributions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    pot_no INTEGER NOT NULL,
    seat_no INTEGER NOT NULL,
    amount BIGINT NOT NULL,
    UNIQUE (hand_id, pot_no, seat_no)
);

CREATE TABLE IF NOT EXISTS core.hand_pot_winners (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    pot_no INTEGER NOT NULL,
    seat_no INTEGER NOT NULL,
    share_amount BIGINT NOT NULL,
    UNIQUE (hand_id, pot_no, seat_no)
);

CREATE TABLE IF NOT EXISTS core.hand_returns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    seat_no INTEGER NOT NULL,
    amount BIGINT NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('uncalled', 'adjustment', 'refund')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS core.hand_showdowns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    seat_no INTEGER NOT NULL,
    shown_cards TEXT[],
    best5_cards TEXT[],
    hand_rank_class TEXT,
    hand_rank_value BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, seat_no)
);

CREATE TABLE IF NOT EXISTS core.parse_issues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_file_id UUID NOT NULL REFERENCES import.source_files(id),
    fragment_id UUID REFERENCES import.file_fragments(id),
    hand_id UUID REFERENCES core.hands(id),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'error')),
    code TEXT NOT NULL,
    message TEXT NOT NULL,
    line_no INTEGER,
    raw_line TEXT,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS derived.hand_state_resolutions (
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    resolution_version TEXT NOT NULL,
    chip_conservation_ok BOOLEAN NOT NULL DEFAULT FALSE,
    pot_conservation_ok BOOLEAN NOT NULL DEFAULT FALSE,
    rake_amount BIGINT NOT NULL DEFAULT 0,
    final_stacks JSONB NOT NULL DEFAULT '{}'::jsonb,
    invariant_errors JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (hand_id, resolution_version)
);

CREATE TABLE IF NOT EXISTS derived.hand_eliminations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    eliminated_seat_no INTEGER NOT NULL,
    eliminated_player_name TEXT NOT NULL,
    resolved_by_pot_no INTEGER,
    ko_involved_winner_count INTEGER NOT NULL DEFAULT 0,
    hero_involved BOOLEAN NOT NULL DEFAULT FALSE,
    hero_share_fraction NUMERIC(12, 6),
    is_split_ko BOOLEAN NOT NULL DEFAULT FALSE,
    is_sidepot_based BOOLEAN NOT NULL DEFAULT FALSE,
    certainty_state TEXT NOT NULL DEFAULT 'exact' CHECK (certainty_state IN ('exact', 'estimated', 'uncertain', 'inconsistent')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, eliminated_seat_no)
);

CREATE TABLE IF NOT EXISTS derived.mbr_stage_resolution (
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    played_ft_hand BOOLEAN NOT NULL DEFAULT FALSE,
    played_ft_hand_state TEXT NOT NULL DEFAULT 'exact' CHECK (played_ft_hand_state IN ('exact', 'estimated', 'uncertain', 'inconsistent')),
    entered_boundary_zone BOOLEAN NOT NULL DEFAULT FALSE,
    entered_boundary_zone_state TEXT NOT NULL DEFAULT 'estimated' CHECK (entered_boundary_zone_state IN ('exact', 'estimated', 'uncertain', 'inconsistent')),
    ft_table_size INTEGER,
    boundary_ko_ev NUMERIC(12, 6),
    boundary_ko_min NUMERIC(12, 6),
    boundary_ko_max NUMERIC(12, 6),
    boundary_ko_method TEXT,
    boundary_ko_certainty TEXT,
    boundary_ko_state TEXT NOT NULL DEFAULT 'estimated' CHECK (boundary_ko_state IN ('exact', 'estimated', 'uncertain', 'inconsistent')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (hand_id, player_profile_id)
);

CREATE TABLE IF NOT EXISTS derived.street_hand_strength (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    seat_no INTEGER NOT NULL,
    street TEXT NOT NULL CHECK (street IN ('flop', 'turn', 'river')),
    best_hand_class TEXT,
    best_hand_rank_value BIGINT,
    pair_strength TEXT,
    is_nut_hand BOOLEAN,
    is_nut_draw BOOLEAN,
    has_flush_draw BOOLEAN,
    has_backdoor_flush_draw BOOLEAN,
    has_open_ended BOOLEAN,
    has_gutshot BOOLEAN,
    has_double_gutshot BOOLEAN,
    has_pair_plus_draw BOOLEAN,
    has_overcards BOOLEAN,
    has_air BOOLEAN,
    has_missed_draw_by_river BOOLEAN,
    descriptor_version TEXT NOT NULL,
    certainty_state TEXT NOT NULL DEFAULT 'exact' CHECK (certainty_state IN ('exact', 'estimated', 'uncertain', 'inconsistent')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hand_id, seat_no, street, descriptor_version)
);

CREATE TABLE IF NOT EXISTS analytics.player_hand_bool_features (
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    feature_key TEXT NOT NULL,
    feature_version TEXT NOT NULL,
    value BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, player_profile_id, hand_id, feature_key, feature_version)
);

CREATE TABLE IF NOT EXISTS analytics.player_hand_num_features (
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    feature_key TEXT NOT NULL,
    feature_version TEXT NOT NULL,
    value NUMERIC(18, 6) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, player_profile_id, hand_id, feature_key, feature_version)
);

CREATE TABLE IF NOT EXISTS analytics.player_hand_enum_features (
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    feature_key TEXT NOT NULL,
    feature_version TEXT NOT NULL,
    value TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, player_profile_id, hand_id, feature_key, feature_version)
);

CREATE INDEX IF NOT EXISTS idx_org_organization_memberships_user ON org.organization_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_org_study_groups_org ON org.study_groups(organization_id);
CREATE INDEX IF NOT EXISTS idx_player_profiles_owner ON core.player_profiles(owner_user_id, organization_id);
CREATE INDEX IF NOT EXISTS idx_source_files_sha256 ON import.source_files(sha256);
CREATE INDEX IF NOT EXISTS idx_source_files_org_created ON import.source_files(organization_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_import_jobs_source_file ON import.import_jobs(source_file_id);
CREATE INDEX IF NOT EXISTS idx_import_jobs_stage ON import.import_jobs(status, stage);
CREATE INDEX IF NOT EXISTS idx_file_fragments_source_fragment ON import.file_fragments(source_file_id, fragment_index);
CREATE INDEX IF NOT EXISTS idx_tournaments_player_started ON core.tournaments(player_profile_id, started_at);
CREATE INDEX IF NOT EXISTS idx_tournament_entries_tournament ON core.tournament_entries(tournament_id);
CREATE INDEX IF NOT EXISTS idx_hands_player_started ON core.hands(player_profile_id, hand_started_at);
CREATE INDEX IF NOT EXISTS idx_hands_tournament_started ON core.hands(tournament_id, hand_started_at);
CREATE INDEX IF NOT EXISTS idx_hands_external_hand_id ON core.hands(external_hand_id);
CREATE INDEX IF NOT EXISTS idx_hand_actions_hand_sequence ON core.hand_actions(hand_id, sequence_no);
CREATE INDEX IF NOT EXISTS idx_hand_seats_hand_seat ON core.hand_seats(hand_id, seat_no);
CREATE INDEX IF NOT EXISTS idx_parse_issues_source_severity ON core.parse_issues(source_file_id, severity);
CREATE INDEX IF NOT EXISTS idx_hand_eliminations_hand ON derived.hand_eliminations(hand_id);
CREATE INDEX IF NOT EXISTS idx_mbr_stage_resolution_player_ft ON derived.mbr_stage_resolution(player_profile_id, played_ft_hand);
CREATE INDEX IF NOT EXISTS idx_street_hand_strength_lookup ON derived.street_hand_strength(hand_id, seat_no, street);
CREATE INDEX IF NOT EXISTS idx_bool_features_lookup ON analytics.player_hand_bool_features(player_profile_id, feature_key, hand_id);
CREATE INDEX IF NOT EXISTS idx_num_features_lookup ON analytics.player_hand_num_features(player_profile_id, feature_key, hand_id);
CREATE INDEX IF NOT EXISTS idx_enum_features_lookup ON analytics.player_hand_enum_features(player_profile_id, feature_key, hand_id);
