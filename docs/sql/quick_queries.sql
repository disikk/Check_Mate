-- tournaments
SELECT id, external_tournament_id, buyin_total, max_players, created_at
FROM core.tournaments
ORDER BY created_at DESC;

-- hands
SELECT id, external_hand_id, table_name, table_max_seats, small_blind, big_blind, ante, created_at
FROM core.hands
ORDER BY created_at DESC
LIMIT 50;

-- parse issues
SELECT severity, code, message, raw_line, created_at
FROM core.parse_issues
ORDER BY created_at DESC
LIMIT 100;

-- eliminations
SELECT hand_id, eliminated_player_name, resolved_by_pot_no, hero_involved, hero_share_fraction, is_split_ko, split_n, is_sidepot_based, certainty_state
FROM derived.hand_eliminations
ORDER BY created_at DESC;

-- mbr stage resolution
SELECT hand_id, played_ft_hand, played_ft_hand_state, entered_boundary_zone, entered_boundary_zone_state, ft_table_size, boundary_ko_state
FROM derived.mbr_stage_resolution
ORDER BY created_at DESC;

-- bool features
SELECT hand_id, feature_key, value
FROM analytics.player_hand_bool_features
ORDER BY created_at DESC
LIMIT 100;

-- num features
SELECT hand_id, feature_key, value
FROM analytics.player_hand_num_features
ORDER BY created_at DESC
LIMIT 100;

-- enum features
SELECT hand_id, feature_key, value
FROM analytics.player_hand_enum_features
ORDER BY created_at DESC
LIMIT 100;
