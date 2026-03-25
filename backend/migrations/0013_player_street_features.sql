CREATE TABLE IF NOT EXISTS analytics.player_street_bool_features (
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    seat_no INTEGER NOT NULL,
    street TEXT NOT NULL CHECK (street IN ('flop', 'turn', 'river')),
    feature_key TEXT NOT NULL,
    feature_version TEXT NOT NULL,
    value BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (
        organization_id,
        player_profile_id,
        hand_id,
        seat_no,
        street,
        feature_key,
        feature_version
    )
);

CREATE TABLE IF NOT EXISTS analytics.player_street_num_features (
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    seat_no INTEGER NOT NULL,
    street TEXT NOT NULL CHECK (street IN ('flop', 'turn', 'river')),
    feature_key TEXT NOT NULL,
    feature_version TEXT NOT NULL,
    value NUMERIC(18, 6) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (
        organization_id,
        player_profile_id,
        hand_id,
        seat_no,
        street,
        feature_key,
        feature_version
    )
);

CREATE TABLE IF NOT EXISTS analytics.player_street_enum_features (
    organization_id UUID NOT NULL REFERENCES org.organizations(id),
    player_profile_id UUID NOT NULL REFERENCES core.player_profiles(id),
    hand_id UUID NOT NULL REFERENCES core.hands(id),
    seat_no INTEGER NOT NULL,
    street TEXT NOT NULL CHECK (street IN ('flop', 'turn', 'river')),
    feature_key TEXT NOT NULL,
    feature_version TEXT NOT NULL,
    value TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (
        organization_id,
        player_profile_id,
        hand_id,
        seat_no,
        street,
        feature_key,
        feature_version
    )
);

CREATE INDEX IF NOT EXISTS idx_street_bool_features_lookup
    ON analytics.player_street_bool_features(player_profile_id, feature_key, street, hand_id, seat_no);

CREATE INDEX IF NOT EXISTS idx_street_num_features_lookup
    ON analytics.player_street_num_features(player_profile_id, feature_key, street, hand_id, seat_no);

CREATE INDEX IF NOT EXISTS idx_street_enum_features_lookup
    ON analytics.player_street_enum_features(player_profile_id, feature_key, street, hand_id, seat_no);
