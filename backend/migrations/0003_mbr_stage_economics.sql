CREATE SCHEMA IF NOT EXISTS ref;

CREATE TABLE IF NOT EXISTS ref.mbr_buyin_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id UUID NOT NULL REFERENCES core.rooms(id),
    format_id UUID NOT NULL REFERENCES core.formats(id),
    buyin_total NUMERIC(12, 2) NOT NULL,
    currency TEXT NOT NULL,
    max_players INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (room_id, format_id, buyin_total, currency, max_players)
);

CREATE TABLE IF NOT EXISTS ref.mbr_regular_prizes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    buyin_config_id UUID NOT NULL REFERENCES ref.mbr_buyin_configs(id) ON DELETE CASCADE,
    finish_place INTEGER NOT NULL,
    regular_prize_money NUMERIC(12, 2) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (buyin_config_id, finish_place)
);

CREATE TABLE IF NOT EXISTS ref.mbr_mystery_envelopes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    buyin_config_id UUID NOT NULL REFERENCES ref.mbr_buyin_configs(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL,
    payout_money NUMERIC(12, 2) NOT NULL,
    frequency_per_100m BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (buyin_config_id, sort_order),
    UNIQUE (buyin_config_id, payout_money, frequency_per_100m)
);

CREATE INDEX IF NOT EXISTS idx_mbr_buyin_configs_lookup
    ON ref.mbr_buyin_configs(room_id, format_id, buyin_total, currency, max_players);

CREATE INDEX IF NOT EXISTS idx_mbr_regular_prizes_config
    ON ref.mbr_regular_prizes(buyin_config_id, finish_place);

CREATE INDEX IF NOT EXISTS idx_mbr_mystery_envelopes_config
    ON ref.mbr_mystery_envelopes(buyin_config_id, sort_order);
