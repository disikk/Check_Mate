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

INSERT INTO analytics.feature_catalog (
    feature_key,
    feature_version,
    table_family,
    value_kind,
    exactness_class,
    description
)
VALUES
    ('played_ft_hand', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Exact final-table participation flag sourced from derived.mbr_stage_resolution.'),
    ('has_exact_ko', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Whether Hero has at least one exact KO on the hand.'),
    ('has_split_ko', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Whether Hero shares at least one exact split KO on the hand.'),
    ('has_sidepot_ko', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Whether Hero records at least one exact side-pot KO on the hand.'),
    ('ft_table_size', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Observed seat count on exact final-table hands.'),
    ('hero_exact_ko_count', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Exact KO event count attributed to Hero on the hand.'),
    ('hero_split_ko_count', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Exact split-KO event count attributed to Hero on the hand.'),
    ('hero_sidepot_ko_count', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Exact side-pot KO event count attributed to Hero on the hand.'),
    ('ft_stage_bucket', 'mbr_runtime_v1', 'enum', 'enum', 'exact', 'Derived final-table stage bucket from exact table size.')
ON CONFLICT (feature_key, feature_version) DO UPDATE
SET
    table_family = EXCLUDED.table_family,
    value_kind = EXCLUDED.value_kind,
    exactness_class = EXCLUDED.exactness_class,
    description = EXCLUDED.description,
    is_active = TRUE;

INSERT INTO analytics.stat_catalog (
    stat_key,
    stat_family,
    exactness_class,
    output_kind,
    description
)
VALUES
    ('roi_pct', 'seed_snapshot', 'exact', 'percent', 'ROI over summary-covered tournaments.'),
    ('avg_finish_place', 'seed_snapshot', 'exact', 'double', 'Average finish place over summary-covered tournaments.'),
    ('final_table_reach_percent', 'seed_snapshot', 'exact', 'percent', 'Share of HH-covered tournaments where Hero reaches the final table.'),
    ('total_ko', 'seed_snapshot', 'exact', 'integer', 'Total exact KO count over HH-covered tournaments.'),
    ('avg_ko_per_tournament', 'seed_snapshot', 'exact', 'double', 'Average exact KO count per HH-covered tournament.')
ON CONFLICT (stat_key) DO UPDATE
SET
    stat_family = EXCLUDED.stat_family,
    exactness_class = EXCLUDED.exactness_class,
    output_kind = EXCLUDED.output_kind,
    description = EXCLUDED.description,
    is_active = TRUE;

INSERT INTO analytics.stat_dependencies (
    stat_key,
    dependency_kind,
    dependency_key,
    dependency_version
)
VALUES
    ('roi_pct', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('roi_pct', 'summary_field', 'core.tournament_entries.total_payout_money', ''),
    ('avg_finish_place', 'summary_field', 'core.tournament_entries.finish_place', ''),
    ('final_table_reach_percent', 'feature', 'played_ft_hand', 'mbr_runtime_v1'),
    ('final_table_reach_percent', 'coverage_metric', 'hand_tournament_count', ''),
    ('total_ko', 'feature', 'hero_exact_ko_count', 'mbr_runtime_v1'),
    ('avg_ko_per_tournament', 'feature', 'hero_exact_ko_count', 'mbr_runtime_v1'),
    ('avg_ko_per_tournament', 'coverage_metric', 'hand_tournament_count', '')
ON CONFLICT (stat_key, dependency_kind, dependency_key, dependency_version) DO NOTHING;

INSERT INTO analytics.materialization_policies (
    target_kind,
    target_key,
    target_version,
    policy_code,
    refresh_mode
)
VALUES
    ('feature', 'played_ft_hand', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'has_exact_ko', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'has_split_ko', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'has_sidepot_ko', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'ft_table_size', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'hero_exact_ko_count', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'hero_split_ko_count', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'hero_sidepot_ko_count', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'ft_stage_bucket', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('stat', 'roi_pct', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'avg_finish_place', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'final_table_reach_percent', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'total_ko', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'avg_ko_per_tournament', '', 'query_only_seed_snapshot', 'full_refresh')
ON CONFLICT (target_kind, target_key, target_version, policy_code) DO UPDATE
SET
    refresh_mode = EXCLUDED.refresh_mode,
    is_active = TRUE;

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
