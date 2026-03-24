INSERT INTO core.rooms (code, name)
VALUES ('gg', 'GG Poker')
ON CONFLICT (code) DO UPDATE
SET name = EXCLUDED.name;

INSERT INTO core.formats (room_id, code, name, max_players)
SELECT r.id, 'mbr', 'Mystery Battle Royale', 18
FROM core.rooms AS r
WHERE r.code = 'gg'
ON CONFLICT (room_id, code) DO UPDATE
SET
    name = EXCLUDED.name,
    max_players = EXCLUDED.max_players;

-- Current GG Mystery Battle Royale reference economics, aligned to the
-- official GGPoker public payout tables as captured on 2026-03-24.
WITH gg_mbr AS (
    SELECT r.id AS room_id, f.id AS format_id
    FROM core.rooms AS r
    INNER JOIN core.formats AS f
        ON f.room_id = r.id
    WHERE r.code = 'gg'
      AND f.code = 'mbr'
),
buyin_configs(buyin_total, currency, max_players) AS (
    VALUES
        (0.25::numeric(12, 2), 'USD', 18),
        (1.00::numeric(12, 2), 'USD', 18),
        (3.00::numeric(12, 2), 'USD', 18),
        (10.00::numeric(12, 2), 'USD', 18),
        (25.00::numeric(12, 2), 'USD', 18)
)
INSERT INTO ref.mbr_buyin_configs (room_id, format_id, buyin_total, currency, max_players)
SELECT gg_mbr.room_id, gg_mbr.format_id, buyin_configs.buyin_total, buyin_configs.currency, buyin_configs.max_players
FROM gg_mbr
CROSS JOIN buyin_configs
ON CONFLICT (room_id, format_id, buyin_total, currency, max_players) DO NOTHING;

WITH config_map AS (
    SELECT cfg.id, cfg.buyin_total
    FROM ref.mbr_buyin_configs AS cfg
    INNER JOIN core.rooms AS r
        ON r.id = cfg.room_id
    INNER JOIN core.formats AS f
        ON f.id = cfg.format_id
    WHERE r.code = 'gg'
      AND f.code = 'mbr'
      AND cfg.currency = 'USD'
      AND cfg.max_players = 18
),
regular_prizes(buyin_total, finish_place, regular_prize_money) AS (
    VALUES
        (0.25::numeric(12, 2), 1, 1.00::numeric(12, 2)),
        (0.25::numeric(12, 2), 2, 0.75::numeric(12, 2)),
        (0.25::numeric(12, 2), 3, 0.50::numeric(12, 2)),
        (1.00::numeric(12, 2), 1, 4.00::numeric(12, 2)),
        (1.00::numeric(12, 2), 2, 3.00::numeric(12, 2)),
        (1.00::numeric(12, 2), 3, 2.00::numeric(12, 2)),
        (3.00::numeric(12, 2), 1, 12.00::numeric(12, 2)),
        (3.00::numeric(12, 2), 2, 9.00::numeric(12, 2)),
        (3.00::numeric(12, 2), 3, 6.00::numeric(12, 2)),
        (10.00::numeric(12, 2), 1, 40.00::numeric(12, 2)),
        (10.00::numeric(12, 2), 2, 30.00::numeric(12, 2)),
        (10.00::numeric(12, 2), 3, 20.00::numeric(12, 2)),
        (25.00::numeric(12, 2), 1, 100.00::numeric(12, 2)),
        (25.00::numeric(12, 2), 2, 75.00::numeric(12, 2)),
        (25.00::numeric(12, 2), 3, 50.00::numeric(12, 2))
)
INSERT INTO ref.mbr_regular_prizes (buyin_config_id, finish_place, regular_prize_money)
SELECT config_map.id, regular_prizes.finish_place, regular_prizes.regular_prize_money
FROM config_map
INNER JOIN regular_prizes
    ON regular_prizes.buyin_total = config_map.buyin_total
ON CONFLICT (buyin_config_id, finish_place) DO UPDATE
SET regular_prize_money = EXCLUDED.regular_prize_money;

WITH config_map AS (
    SELECT cfg.id, cfg.buyin_total
    FROM ref.mbr_buyin_configs AS cfg
    INNER JOIN core.rooms AS r
        ON r.id = cfg.room_id
    INNER JOIN core.formats AS f
        ON f.id = cfg.format_id
    WHERE r.code = 'gg'
      AND f.code = 'mbr'
      AND cfg.currency = 'USD'
      AND cfg.max_players = 18
),
mystery_envelopes(buyin_total, sort_order, payout_money, frequency_per_100m) AS (
    VALUES
        (0.25::numeric(12, 2), 1, 5000.00::numeric(12, 2), 30::bigint),
        (0.25::numeric(12, 2), 2, 250.00::numeric(12, 2), 400::bigint),
        (0.25::numeric(12, 2), 3, 25.00::numeric(12, 2), 4000::bigint),
        (0.25::numeric(12, 2), 4, 2.50::numeric(12, 2), 3500000::bigint),
        (0.25::numeric(12, 2), 5, 0.50::numeric(12, 2), 3600000::bigint),
        (0.25::numeric(12, 2), 6, 0.37::numeric(12, 2), 3800000::bigint),
        (0.25::numeric(12, 2), 7, 0.25::numeric(12, 2), 4000000::bigint),
        (0.25::numeric(12, 2), 8, 0.18::numeric(12, 2), 23000000::bigint),
        (0.25::numeric(12, 2), 9, 0.13::numeric(12, 2), 35046650::bigint),
        (0.25::numeric(12, 2), 10, 0.06::numeric(12, 2), 27048920::bigint),
        (1.00::numeric(12, 2), 1, 10000.00::numeric(12, 2), 60::bigint),
        (1.00::numeric(12, 2), 2, 1000.00::numeric(12, 2), 400::bigint),
        (1.00::numeric(12, 2), 3, 100.00::numeric(12, 2), 4000::bigint),
        (1.00::numeric(12, 2), 4, 10.00::numeric(12, 2), 3500000::bigint),
        (1.00::numeric(12, 2), 5, 2.00::numeric(12, 2), 3600000::bigint),
        (1.00::numeric(12, 2), 6, 1.50::numeric(12, 2), 3800000::bigint),
        (1.00::numeric(12, 2), 7, 1.00::numeric(12, 2), 4000000::bigint),
        (1.00::numeric(12, 2), 8, 0.75::numeric(12, 2), 23000000::bigint),
        (1.00::numeric(12, 2), 9, 0.50::numeric(12, 2), 33704460::bigint),
        (1.00::numeric(12, 2), 10, 0.25::numeric(12, 2), 28391080::bigint),
        (3.00::numeric(12, 2), 1, 30000.00::numeric(12, 2), 80::bigint),
        (3.00::numeric(12, 2), 2, 3000.00::numeric(12, 2), 400::bigint),
        (3.00::numeric(12, 2), 3, 300.00::numeric(12, 2), 4000::bigint),
        (3.00::numeric(12, 2), 4, 30.00::numeric(12, 2), 3500000::bigint),
        (3.00::numeric(12, 2), 5, 6.00::numeric(12, 2), 3600000::bigint),
        (3.00::numeric(12, 2), 6, 4.50::numeric(12, 2), 3800000::bigint),
        (3.00::numeric(12, 2), 7, 3.00::numeric(12, 2), 4000000::bigint),
        (3.00::numeric(12, 2), 8, 2.25::numeric(12, 2), 23000000::bigint),
        (3.00::numeric(12, 2), 9, 1.50::numeric(12, 2), 32904480::bigint),
        (3.00::numeric(12, 2), 10, 0.75::numeric(12, 2), 29191040::bigint),
        (10.00::numeric(12, 2), 1, 100000.00::numeric(12, 2), 100::bigint),
        (10.00::numeric(12, 2), 2, 10000.00::numeric(12, 2), 400::bigint),
        (10.00::numeric(12, 2), 3, 1000.00::numeric(12, 2), 4000::bigint),
        (10.00::numeric(12, 2), 4, 100.00::numeric(12, 2), 3500000::bigint),
        (10.00::numeric(12, 2), 5, 20.00::numeric(12, 2), 3600000::bigint),
        (10.00::numeric(12, 2), 6, 15.00::numeric(12, 2), 3800000::bigint),
        (10.00::numeric(12, 2), 7, 10.00::numeric(12, 2), 4000000::bigint),
        (10.00::numeric(12, 2), 8, 7.50::numeric(12, 2), 23000000::bigint),
        (10.00::numeric(12, 2), 9, 5.00::numeric(12, 2), 32104500::bigint),
        (10.00::numeric(12, 2), 10, 2.50::numeric(12, 2), 29991000::bigint),
        (25.00::numeric(12, 2), 1, 250000.00::numeric(12, 2), 100::bigint),
        (25.00::numeric(12, 2), 2, 25000.00::numeric(12, 2), 400::bigint),
        (25.00::numeric(12, 2), 3, 2500.00::numeric(12, 2), 4000::bigint),
        (25.00::numeric(12, 2), 4, 250.00::numeric(12, 2), 3500000::bigint),
        (25.00::numeric(12, 2), 5, 50.00::numeric(12, 2), 3600000::bigint),
        (25.00::numeric(12, 2), 6, 37.00::numeric(12, 2), 3800000::bigint),
        (25.00::numeric(12, 2), 7, 25.00::numeric(12, 2), 4000000::bigint),
        (25.00::numeric(12, 2), 8, 18.00::numeric(12, 2), 23000000::bigint),
        (25.00::numeric(12, 2), 9, 13.00::numeric(12, 2), 33618140::bigint),
        (25.00::numeric(12, 2), 10, 6.00::numeric(12, 2), 28477360::bigint)
)
INSERT INTO ref.mbr_mystery_envelopes (buyin_config_id, sort_order, payout_money, frequency_per_100m)
SELECT config_map.id, mystery_envelopes.sort_order, mystery_envelopes.payout_money, mystery_envelopes.frequency_per_100m
FROM config_map
INNER JOIN mystery_envelopes
    ON mystery_envelopes.buyin_total = config_map.buyin_total
ON CONFLICT (buyin_config_id, sort_order) DO UPDATE
SET
    payout_money = EXCLUDED.payout_money,
    frequency_per_100m = EXCLUDED.frequency_per_100m;
