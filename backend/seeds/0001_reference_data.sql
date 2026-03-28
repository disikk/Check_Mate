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

DELETE FROM analytics.stat_dependencies
WHERE stat_key IN ('total_ko', 'avg_ko_per_tournament')
   OR dependency_key IN (
       'hero_exact_ko_count',
       'hero_split_ko_count',
       'hero_sidepot_ko_count'
   );

DELETE FROM analytics.materialization_policies
WHERE (target_kind = 'feature' AND target_version = 'mbr_runtime_v1' AND target_key IN (
           'has_exact_ko',
           'has_split_ko',
           'has_sidepot_ko',
           'hero_exact_ko_count',
           'hero_split_ko_count',
           'hero_sidepot_ko_count'
       ))
   OR (target_kind = 'stat' AND target_key IN ('total_ko', 'avg_ko_per_tournament'));

DELETE FROM analytics.feature_catalog
WHERE feature_version = 'mbr_runtime_v1'
  AND feature_key IN (
      'has_exact_ko',
      'has_split_ko',
      'has_sidepot_ko',
      'hero_exact_ko_count',
      'hero_split_ko_count',
      'hero_sidepot_ko_count'
  );

DELETE FROM analytics.stat_catalog
WHERE stat_key IN ('total_ko', 'avg_ko_per_tournament');

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
    ('is_ft_hand', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Canonical exact final-table hand predicate sourced from derived.mbr_stage_resolution.'),
    ('is_stage_2', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Exact heads-up final-table hand predicate sourced from derived.mbr_stage_resolution.'),
    ('is_stage_3_4', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Exact stage 3-4 hand predicate sourced from derived.mbr_stage_resolution.'),
    ('is_stage_4_5', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Exact stage 4-5 hand predicate sourced from derived.mbr_stage_resolution.'),
    ('is_stage_5_6', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Exact stage 5-6 hand predicate sourced from derived.mbr_stage_resolution.'),
    ('is_stage_6_9', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Exact stage 6-9 hand predicate sourced from derived.mbr_stage_resolution.'),
    ('is_boundary_hand', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Formal boundary-resolution hand predicate sourced from derived.mbr_stage_resolution.'),
    ('has_exact_ko_event', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Whether Hero has at least one exact KO event on the hand.'),
    ('has_split_ko_event', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Whether Hero shares at least one exact split-KO event on the hand.'),
    ('has_sidepot_ko_event', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Whether Hero records at least one exact side-pot KO event on the hand.'),
    ('ft_table_size', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Observed seat count on exact final-table hands.'),
    ('ft_players_remaining_exact', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Exact number of players remaining on the hand when the hand is provably a final-table hand.'),
    ('hero_exact_ko_event_count', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Exact KO event count attributed to Hero on the hand.'),
    ('hero_split_ko_event_count', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Exact split-KO event count attributed to Hero on the hand.'),
    ('hero_sidepot_ko_event_count', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Exact side-pot KO event count attributed to Hero on the hand.'),
    ('ft_stage_bucket', 'mbr_runtime_v1', 'enum', 'enum', 'exact', 'Derived final-table stage bucket from exact table size.'),
    ('best_hand_class', 'mbr_runtime_v1', 'enum', 'enum', 'exact', 'Exact street-grain best hand class from derived.street_hand_strength.'),
    ('best_hand_rank_value', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Exact street-grain rank value for the best made hand.'),
    ('made_hand_category', 'mbr_runtime_v1', 'enum', 'enum', 'exact', 'Exact street-grain made-hand descriptor category.'),
    ('draw_category', 'mbr_runtime_v1', 'enum', 'enum', 'exact', 'Exact street-grain draw descriptor category.'),
    ('overcards_count', 'mbr_runtime_v1', 'num', 'double', 'exact', 'Exact street-grain overcards count.'),
    ('starter_hand_class', 'mbr_runtime_v1', 'enum', 'enum', 'exact', 'Exact preflop matrix starter-hand class from derived.preflop_starting_hands.'),
    ('has_air', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Exact street-grain air-like descriptor flag.'),
    ('missed_flush_draw', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Exact street-grain missed flush draw flag.'),
    ('missed_straight_draw', 'mbr_runtime_v1', 'bool', 'bool', 'exact', 'Exact street-grain missed straight draw flag.'),
    ('certainty_state', 'mbr_runtime_v1', 'enum', 'enum', 'exact', 'Exactness state for the street-grain descriptor row.')
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
    ('avg_finish_place_ft', 'canonical_query_time', 'exact', 'double', 'Average finish place over summary-covered tournaments where the exact FT helper proves Hero reached FT.'),
    ('avg_finish_place_no_ft', 'canonical_query_time', 'exact', 'double', 'Average finish place over summary-covered tournaments where the exact FT helper proves Hero did not reach FT.'),
    ('avg_ft_initial_stack_chips', 'canonical_query_time', 'exact', 'double', 'Average Hero chip stack on the first exact FT hand.'),
    ('avg_ft_initial_stack_bb', 'canonical_query_time', 'exact', 'double', 'Average Hero stack in big blinds on the first exact FT hand.'),
    ('avg_ko_attempts_per_ft', 'canonical_query_time', 'exact', 'double', 'Average exact KO attempts per FT-reached tournament during stage 6-9.'),
    ('avg_ko_event_per_tournament', 'seed_snapshot', 'exact', 'double', 'Average exact KO event count per HH-covered tournament from exact elimination rows.'),
    ('big_ko_x10000_count', 'canonical_query_time', 'estimated', 'double', 'Expected count mass of Hero KO-money events in the x10000 bucket from official envelope frequencies.'),
    ('big_ko_x1000_count', 'canonical_query_time', 'estimated', 'double', 'Expected count mass of Hero KO-money events in the x1000 bucket from official envelope frequencies.'),
    ('big_ko_x100_count', 'canonical_query_time', 'estimated', 'double', 'Expected count mass of Hero KO-money events in the x100 bucket from official envelope frequencies.'),
    ('big_ko_x10_count', 'canonical_query_time', 'estimated', 'double', 'Expected count mass of Hero KO-money events in the x10 bucket from official envelope frequencies.'),
    ('big_ko_x1_5_count', 'canonical_query_time', 'estimated', 'double', 'Expected count mass of Hero KO-money events in the x1.5 bucket from official envelope frequencies.'),
    ('big_ko_x2_count', 'canonical_query_time', 'estimated', 'double', 'Expected count mass of Hero KO-money events in the x2 bucket from official envelope frequencies.'),
    ('deep_ft_reach_percent', 'canonical_query_time', 'exact', 'percent', 'Share of HH-covered tournaments where Hero reaches an exact deep-FT state at five or fewer players.'),
    ('deep_ft_avg_stack_chips', 'canonical_query_time', 'exact', 'double', 'Average Hero chip stack on the first exact deep-FT hand.'),
    ('deep_ft_avg_stack_bb', 'canonical_query_time', 'exact', 'double', 'Average Hero stack in big blinds on the first exact deep-FT hand.'),
    ('deep_ft_roi_pct', 'canonical_query_time', 'exact', 'percent', 'ROI over summary-covered tournaments where Hero reaches deep FT exactly.'),
    ('early_ft_bust_count', 'canonical_query_time', 'exact', 'integer', 'Count of exact Hero bust events on stage 6-9 FT hands.'),
    ('early_ft_bust_per_tournament', 'canonical_query_time', 'exact', 'double', 'Average exact early-FT bust count per FT-reached tournament.'),
    ('early_ft_ko_event_count', 'seed_snapshot', 'exact', 'integer', 'Total exact Hero KO event count on stage 6-9 FT hands.'),
    ('early_ft_ko_event_per_tournament', 'seed_snapshot', 'exact', 'double', 'Average exact Hero KO event count on stage 6-9 FT hands per FT-reached tournament.'),
    ('final_table_reach_percent', 'seed_snapshot', 'exact', 'percent', 'Share of HH-covered tournaments where the exact FT helper proves Hero reached the final table.'),
    ('ft_stack_conversion', 'canonical_query_time', 'exact', 'double', 'Exact early-FT KO event count divided by the summed Hero FT-entry stack in big blinds.'),
    ('ft_stack_conversion_3_4', 'canonical_query_time', 'exact', 'double', 'Exact stage 3-4 KO event count divided by the summed Hero stage-entry stack in big blinds.'),
    ('ft_stack_conversion_3_4_attempts', 'canonical_query_time', 'exact', 'double', 'Average exact KO attempts per tournament that reaches exact stage 3-4.'),
    ('ft_stack_conversion_5_6', 'canonical_query_time', 'exact', 'double', 'Exact stage 5-6 KO event count divided by the summed Hero stage-entry stack in big blinds.'),
    ('ft_stack_conversion_5_6_attempts', 'canonical_query_time', 'exact', 'double', 'Average exact KO attempts per tournament that reaches exact stage 5-6.'),
    ('ft_stack_conversion_7_9', 'canonical_query_time', 'exact', 'double', 'Exact early-FT KO event count divided by the summed Hero FT-entry stack in big blinds.'),
    ('ft_stack_conversion_7_9_attempts', 'canonical_query_time', 'exact', 'double', 'Average exact KO attempts per FT-reached tournament during stage 6-9.'),
    ('incomplete_ft_percent', 'canonical_query_time', 'exact', 'percent', 'Share of exact FT-reached tournaments where the first exact FT hand started incomplete.'),
    ('itm_percent', 'canonical_query_time', 'exact', 'percent', 'Share of summary-covered tournaments where regular prize money is positive.'),
    ('ko_attempts_success_rate', 'canonical_query_time', 'exact', 'double', 'Exact early-FT KO event count divided by exact early-FT KO attempt count.'),
    ('ko_contribution_adjusted_percent', 'canonical_query_time', 'estimated', 'percent', 'Estimated share of supported tournament payouts attributable to KO money under official envelope frequencies.'),
    ('ko_contribution_percent', 'canonical_query_time', 'exact', 'percent', 'Share of summary-covered tournament payouts that came from realized mystery money totals.'),
    ('ko_luck_money_delta', 'canonical_query_time', 'estimated', 'double', 'Difference between realized KO money and frequency-weighted expected KO money on supported tournaments.'),
    ('ko_stage_2_3_attempts_per_tournament', 'canonical_query_time', 'exact', 'double', 'Average exact KO attempts per tournament that reaches exact stage 2-3.'),
    ('ko_stage_2_3_event_count', 'canonical_query_time', 'exact', 'integer', 'Count of exact Hero KO events on exact stage 2-3 hands.'),
    ('ko_stage_2_3_money_total', 'canonical_query_time', 'estimated', 'double', 'Frequency-weighted expected Hero KO money on exact stage 2-3 hands.'),
    ('ko_stage_3_4_attempts_per_tournament', 'canonical_query_time', 'exact', 'double', 'Average exact KO attempts per tournament that reaches exact stage 3-4.'),
    ('ko_stage_3_4_event_count', 'canonical_query_time', 'exact', 'integer', 'Count of exact Hero KO events on exact stage 3-4 hands.'),
    ('ko_stage_3_4_money_total', 'canonical_query_time', 'estimated', 'double', 'Frequency-weighted expected Hero KO money on exact stage 3-4 hands.'),
    ('ko_stage_4_5_attempts_per_tournament', 'canonical_query_time', 'exact', 'double', 'Average exact KO attempts per tournament that reaches exact stage 4-5.'),
    ('ko_stage_4_5_event_count', 'canonical_query_time', 'exact', 'integer', 'Count of exact Hero KO events on exact stage 4-5 hands.'),
    ('ko_stage_4_5_money_total', 'canonical_query_time', 'estimated', 'double', 'Frequency-weighted expected Hero KO money on exact stage 4-5 hands.'),
    ('ko_stage_5_6_attempts_per_tournament', 'canonical_query_time', 'exact', 'double', 'Average exact KO attempts per tournament that reaches exact stage 5-6.'),
    ('ko_stage_5_6_event_count', 'canonical_query_time', 'exact', 'integer', 'Count of exact Hero KO events on exact stage 5-6 hands.'),
    ('ko_stage_5_6_money_total', 'canonical_query_time', 'estimated', 'double', 'Frequency-weighted expected Hero KO money on exact stage 5-6 hands.'),
    ('ko_stage_6_9_event_count', 'canonical_query_time', 'exact', 'integer', 'Count of exact Hero KO events on exact stage 6-9 hands.'),
    ('ko_stage_6_9_money_total', 'canonical_query_time', 'estimated', 'double', 'Frequency-weighted expected Hero KO money on exact stage 6-9 hands.'),
    ('ko_stage_7_9_attempts_per_tournament', 'canonical_query_time', 'exact', 'double', 'Average exact KO attempts per tournament that reaches exact stage 7-9.'),
    ('ko_stage_7_9_event_count', 'canonical_query_time', 'exact', 'integer', 'Count of exact Hero KO events on exact stage 7-9 FT hands.'),
    ('ko_stage_7_9_money_total', 'canonical_query_time', 'estimated', 'double', 'Frequency-weighted expected Hero KO money on exact stage 7-9 FT hands.'),
    ('pre_ft_chipev', 'canonical_query_time', 'exact', 'double', 'Average exact Hero chip delta before the first FT hand and outside the resolved boundary zone.'),
    ('pre_ft_ko_count', 'canonical_query_time', 'exact', 'integer', 'Count of exact Hero KO events before the first exact FT hand and outside the boundary zone.'),
    ('roi_adj_pct', 'canonical_query_time', 'estimated', 'percent', 'ROI using regular prizes plus frequency-weighted expected KO money on supported tournaments.'),
    ('roi_on_ft_pct', 'canonical_query_time', 'exact', 'percent', 'ROI over summary-covered tournaments where the exact FT helper proves Hero reached FT.'),
    ('total_ko_event_count', 'seed_snapshot', 'exact', 'integer', 'Total exact KO event count over HH-covered tournaments from exact elimination rows.'),
    ('winnings_from_itm', 'canonical_query_time', 'exact', 'double', 'Sum of regular prize money over summary-covered tournaments.'),
    ('winnings_from_ko_total', 'canonical_query_time', 'exact', 'double', 'Sum of realized mystery money over summary-covered tournaments.')
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
    ('avg_finish_place_ft', 'summary_field', 'core.tournament_entries.finish_place', ''),
    ('avg_finish_place_ft', 'feature', 'derived.mbr_tournament_ft_helper.reached_ft_exact', 'exact_core'),
    ('avg_finish_place_no_ft', 'summary_field', 'core.tournament_entries.finish_place', ''),
    ('avg_finish_place_no_ft', 'feature', 'derived.mbr_tournament_ft_helper.reached_ft_exact', 'exact_core'),
    ('avg_ft_initial_stack_chips', 'feature', 'derived.mbr_tournament_ft_helper.hero_ft_entry_stack_chips', 'exact_core'),
    ('avg_ft_initial_stack_bb', 'feature', 'derived.mbr_tournament_ft_helper.hero_ft_entry_stack_bb', 'exact_core'),
    ('deep_ft_reach_percent', 'feature', 'derived.mbr_tournament_ft_helper.deepest_ft_size_reached', 'exact_core'),
    ('deep_ft_reach_percent', 'coverage_metric', 'hand_tournament_count', ''),
    ('deep_ft_avg_stack_chips', 'feature', 'derived.mbr_stage_resolution.ft_players_remaining_exact', 'exact_core'),
    ('deep_ft_avg_stack_chips', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('deep_ft_avg_stack_bb', 'feature', 'derived.mbr_stage_resolution.ft_players_remaining_exact', 'exact_core'),
    ('deep_ft_avg_stack_bb', 'summary_field', 'core.hands.big_blind', ''),
    ('deep_ft_roi_pct', 'feature', 'derived.mbr_tournament_ft_helper.deepest_ft_size_reached', 'exact_core'),
    ('deep_ft_roi_pct', 'summary_field', 'core.tournament_entries.total_payout_money', ''),
    ('deep_ft_roi_pct', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('early_ft_bust_count', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('early_ft_bust_count', 'feature', 'derived.hand_eliminations.eliminated_seat_no', 'exact_core'),
    ('early_ft_bust_per_tournament', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('early_ft_bust_per_tournament', 'feature', 'derived.hand_eliminations.eliminated_seat_no', 'exact_core'),
    ('early_ft_bust_per_tournament', 'feature', 'derived.mbr_tournament_ft_helper.reached_ft_exact', 'exact_core'),
    ('early_ft_bust_per_tournament', 'coverage_metric', 'ft_reached_tournament_count', ''),
    ('early_ft_ko_event_count', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('early_ft_ko_event_count', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('early_ft_ko_event_count', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('early_ft_ko_event_per_tournament', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('early_ft_ko_event_per_tournament', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('early_ft_ko_event_per_tournament', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('early_ft_ko_event_per_tournament', 'feature', 'derived.mbr_tournament_ft_helper.reached_ft_exact', 'exact_core'),
    ('early_ft_ko_event_per_tournament', 'coverage_metric', 'ft_reached_tournament_count', ''),
    ('final_table_reach_percent', 'feature', 'derived.mbr_tournament_ft_helper.reached_ft_exact', 'exact_core'),
    ('final_table_reach_percent', 'coverage_metric', 'hand_tournament_count', ''),
    ('ft_stack_conversion', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('ft_stack_conversion', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ft_stack_conversion', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ft_stack_conversion', 'feature', 'derived.mbr_tournament_ft_helper.hero_ft_entry_stack_bb', 'exact_core'),
    ('ft_stack_conversion_3_4', 'feature', 'derived.mbr_stage_resolution.is_stage_3_4', 'exact_core'),
    ('ft_stack_conversion_3_4', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ft_stack_conversion_3_4', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ft_stack_conversion_3_4', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ft_stack_conversion_3_4', 'summary_field', 'core.hands.big_blind', ''),
    ('ft_stack_conversion_3_4_attempts', 'feature', 'derived.mbr_stage_resolution.is_stage_3_4', 'exact_core'),
    ('ft_stack_conversion_3_4_attempts', 'feature', 'core.hand_actions.is_all_in', 'exact_core'),
    ('ft_stack_conversion_3_4_attempts', 'feature', 'core.hand_pot_eligibility', 'exact_core'),
    ('ft_stack_conversion_3_4_attempts', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ft_stack_conversion_3_4_attempts', 'coverage_metric', 'stage_3_4_tournament_count', ''),
    ('ft_stack_conversion_5_6', 'feature', 'derived.mbr_stage_resolution.is_stage_5_6', 'exact_core'),
    ('ft_stack_conversion_5_6', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ft_stack_conversion_5_6', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ft_stack_conversion_5_6', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ft_stack_conversion_5_6', 'summary_field', 'core.hands.big_blind', ''),
    ('ft_stack_conversion_5_6_attempts', 'feature', 'derived.mbr_stage_resolution.is_stage_5_6', 'exact_core'),
    ('ft_stack_conversion_5_6_attempts', 'feature', 'core.hand_actions.is_all_in', 'exact_core'),
    ('ft_stack_conversion_5_6_attempts', 'feature', 'core.hand_pot_eligibility', 'exact_core'),
    ('ft_stack_conversion_5_6_attempts', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ft_stack_conversion_5_6_attempts', 'coverage_metric', 'stage_5_6_tournament_count', ''),
    ('ft_stack_conversion_7_9', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('ft_stack_conversion_7_9', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ft_stack_conversion_7_9', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ft_stack_conversion_7_9', 'feature', 'derived.mbr_tournament_ft_helper.hero_ft_entry_stack_bb', 'exact_core'),
    ('ft_stack_conversion_7_9_attempts', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('ft_stack_conversion_7_9_attempts', 'feature', 'core.hand_actions.is_all_in', 'exact_core'),
    ('ft_stack_conversion_7_9_attempts', 'feature', 'core.hand_pot_eligibility', 'exact_core'),
    ('ft_stack_conversion_7_9_attempts', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ft_stack_conversion_7_9_attempts', 'feature', 'derived.mbr_tournament_ft_helper.reached_ft_exact', 'exact_core'),
    ('ft_stack_conversion_7_9_attempts', 'coverage_metric', 'ft_reached_tournament_count', ''),
    ('incomplete_ft_percent', 'feature', 'derived.mbr_tournament_ft_helper.ft_started_incomplete', 'exact_core'),
    ('incomplete_ft_percent', 'feature', 'derived.mbr_tournament_ft_helper.reached_ft_exact', 'exact_core'),
    ('itm_percent', 'summary_field', 'core.tournament_entries.regular_prize_money', ''),
    ('itm_percent', 'coverage_metric', 'summary_tournament_count', ''),
    ('ko_contribution_adjusted_percent', 'summary_field', 'core.tournament_entries.total_payout_money', ''),
    ('ko_contribution_adjusted_percent', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('ko_contribution_adjusted_percent', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('ko_contribution_adjusted_percent', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('ko_contribution_adjusted_percent', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('ko_contribution_percent', 'summary_field', 'core.tournament_entries.mystery_money_total', ''),
    ('ko_contribution_percent', 'summary_field', 'core.tournament_entries.total_payout_money', ''),
    ('ko_luck_money_delta', 'summary_field', 'core.tournament_entries.mystery_money_total', ''),
    ('ko_luck_money_delta', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('ko_luck_money_delta', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('ko_luck_money_delta', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('ko_luck_money_delta', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('ko_attempts_success_rate', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('ko_attempts_success_rate', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ko_attempts_success_rate', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ko_attempts_success_rate', 'feature', 'core.hand_actions.is_all_in', 'exact_core'),
    ('ko_attempts_success_rate', 'feature', 'core.hand_pot_eligibility', 'exact_core'),
    ('ko_attempts_success_rate', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ko_stage_2_3_event_count', 'feature', 'derived.mbr_stage_resolution.ft_players_remaining_exact', 'exact_core'),
    ('ko_stage_2_3_event_count', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ko_stage_2_3_event_count', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ko_stage_2_3_attempts_per_tournament', 'feature', 'derived.mbr_stage_resolution.ft_players_remaining_exact', 'exact_core'),
    ('ko_stage_2_3_attempts_per_tournament', 'feature', 'core.hand_actions.is_all_in', 'exact_core'),
    ('ko_stage_2_3_attempts_per_tournament', 'feature', 'core.hand_pot_eligibility', 'exact_core'),
    ('ko_stage_2_3_attempts_per_tournament', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ko_stage_2_3_attempts_per_tournament', 'coverage_metric', 'stage_2_3_tournament_count', ''),
    ('ko_stage_2_3_money_total', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('ko_stage_2_3_money_total', 'feature', 'derived.mbr_stage_resolution.ft_players_remaining_exact', 'exact_core'),
    ('ko_stage_2_3_money_total', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('ko_stage_2_3_money_total', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('ko_stage_2_3_money_total', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('ko_stage_3_4_event_count', 'feature', 'derived.mbr_stage_resolution.is_stage_3_4', 'exact_core'),
    ('ko_stage_3_4_event_count', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ko_stage_3_4_event_count', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ko_stage_3_4_attempts_per_tournament', 'feature', 'derived.mbr_stage_resolution.is_stage_3_4', 'exact_core'),
    ('ko_stage_3_4_attempts_per_tournament', 'feature', 'core.hand_actions.is_all_in', 'exact_core'),
    ('ko_stage_3_4_attempts_per_tournament', 'feature', 'core.hand_pot_eligibility', 'exact_core'),
    ('ko_stage_3_4_attempts_per_tournament', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ko_stage_3_4_attempts_per_tournament', 'coverage_metric', 'stage_3_4_tournament_count', ''),
    ('ko_stage_3_4_money_total', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('ko_stage_3_4_money_total', 'feature', 'derived.mbr_stage_resolution.is_stage_3_4', 'exact_core'),
    ('ko_stage_3_4_money_total', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('ko_stage_3_4_money_total', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('ko_stage_3_4_money_total', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('ko_stage_4_5_event_count', 'feature', 'derived.mbr_stage_resolution.is_stage_4_5', 'exact_core'),
    ('ko_stage_4_5_event_count', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ko_stage_4_5_event_count', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ko_stage_4_5_attempts_per_tournament', 'feature', 'derived.mbr_stage_resolution.is_stage_4_5', 'exact_core'),
    ('ko_stage_4_5_attempts_per_tournament', 'feature', 'core.hand_actions.is_all_in', 'exact_core'),
    ('ko_stage_4_5_attempts_per_tournament', 'feature', 'core.hand_pot_eligibility', 'exact_core'),
    ('ko_stage_4_5_attempts_per_tournament', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ko_stage_4_5_attempts_per_tournament', 'coverage_metric', 'stage_4_5_tournament_count', ''),
    ('ko_stage_4_5_money_total', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('ko_stage_4_5_money_total', 'feature', 'derived.mbr_stage_resolution.is_stage_4_5', 'exact_core'),
    ('ko_stage_4_5_money_total', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('ko_stage_4_5_money_total', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('ko_stage_4_5_money_total', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('ko_stage_5_6_event_count', 'feature', 'derived.mbr_stage_resolution.is_stage_5_6', 'exact_core'),
    ('ko_stage_5_6_event_count', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ko_stage_5_6_event_count', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ko_stage_5_6_attempts_per_tournament', 'feature', 'derived.mbr_stage_resolution.is_stage_5_6', 'exact_core'),
    ('ko_stage_5_6_attempts_per_tournament', 'feature', 'core.hand_actions.is_all_in', 'exact_core'),
    ('ko_stage_5_6_attempts_per_tournament', 'feature', 'core.hand_pot_eligibility', 'exact_core'),
    ('ko_stage_5_6_attempts_per_tournament', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ko_stage_5_6_attempts_per_tournament', 'coverage_metric', 'stage_5_6_tournament_count', ''),
    ('ko_stage_5_6_money_total', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('ko_stage_5_6_money_total', 'feature', 'derived.mbr_stage_resolution.is_stage_5_6', 'exact_core'),
    ('ko_stage_5_6_money_total', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('ko_stage_5_6_money_total', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('ko_stage_5_6_money_total', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('ko_stage_6_9_event_count', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('ko_stage_6_9_event_count', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ko_stage_6_9_event_count', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ko_stage_6_9_money_total', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('ko_stage_6_9_money_total', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('ko_stage_6_9_money_total', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('ko_stage_6_9_money_total', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('ko_stage_6_9_money_total', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('ko_stage_7_9_event_count', 'feature', 'derived.mbr_stage_resolution.ft_players_remaining_exact', 'exact_core'),
    ('ko_stage_7_9_event_count', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('ko_stage_7_9_event_count', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('ko_stage_7_9_attempts_per_tournament', 'feature', 'derived.mbr_stage_resolution.ft_players_remaining_exact', 'exact_core'),
    ('ko_stage_7_9_attempts_per_tournament', 'feature', 'core.hand_actions.is_all_in', 'exact_core'),
    ('ko_stage_7_9_attempts_per_tournament', 'feature', 'core.hand_pot_eligibility', 'exact_core'),
    ('ko_stage_7_9_attempts_per_tournament', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('ko_stage_7_9_attempts_per_tournament', 'coverage_metric', 'stage_7_9_tournament_count', ''),
    ('ko_stage_7_9_money_total', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('ko_stage_7_9_money_total', 'feature', 'derived.mbr_stage_resolution.ft_players_remaining_exact', 'exact_core'),
    ('ko_stage_7_9_money_total', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('ko_stage_7_9_money_total', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('ko_stage_7_9_money_total', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('pre_ft_chipev', 'feature', 'derived.hand_state_resolutions.final_stacks', 'exact_core'),
    ('pre_ft_chipev', 'feature', 'derived.mbr_tournament_ft_helper.first_ft_hand_started_local', 'exact_core'),
    ('pre_ft_chipev', 'feature', 'derived.mbr_tournament_ft_helper.boundary_resolution_state', 'exact_core'),
    ('pre_ft_chipev', 'feature', 'derived.mbr_stage_resolution.is_boundary_hand', 'exact_core'),
    ('pre_ft_chipev', 'coverage_metric', 'pre_ft_supported_tournament_count', ''),
    ('pre_ft_ko_count', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('pre_ft_ko_count', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('pre_ft_ko_count', 'feature', 'derived.mbr_tournament_ft_helper.first_ft_hand_started_local', 'exact_core'),
    ('pre_ft_ko_count', 'feature', 'derived.mbr_stage_resolution.is_boundary_hand', 'exact_core'),
    ('roi_adj_pct', 'summary_field', 'core.tournament_entries.regular_prize_money', ''),
    ('roi_adj_pct', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('roi_adj_pct', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('roi_adj_pct', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('roi_adj_pct', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('roi_on_ft_pct', 'summary_field', 'core.tournament_entries.total_payout_money', ''),
    ('roi_on_ft_pct', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('roi_on_ft_pct', 'feature', 'derived.mbr_tournament_ft_helper.reached_ft_exact', 'exact_core'),
    ('total_ko_event_count', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('total_ko_event_count', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('avg_ko_attempts_per_ft', 'feature', 'derived.mbr_stage_resolution.is_stage_6_9', 'exact_core'),
    ('avg_ko_attempts_per_ft', 'feature', 'core.hand_actions.is_all_in', 'exact_core'),
    ('avg_ko_attempts_per_ft', 'feature', 'core.hand_pot_eligibility', 'exact_core'),
    ('avg_ko_attempts_per_ft', 'summary_field', 'core.hand_seats.starting_stack', ''),
    ('avg_ko_attempts_per_ft', 'feature', 'derived.mbr_tournament_ft_helper.reached_ft_exact', 'exact_core'),
    ('avg_ko_attempts_per_ft', 'coverage_metric', 'ft_reached_tournament_count', ''),
    ('avg_ko_event_per_tournament', 'feature', 'derived.hand_eliminations.hero_involved', 'exact_core'),
    ('avg_ko_event_per_tournament', 'feature', 'derived.hand_eliminations.certainty_state', 'exact_core'),
    ('avg_ko_event_per_tournament', 'coverage_metric', 'hand_tournament_count', ''),
    ('big_ko_x10000_count', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('big_ko_x10000_count', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('big_ko_x10000_count', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('big_ko_x10000_count', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('big_ko_x1000_count', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('big_ko_x1000_count', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('big_ko_x1000_count', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('big_ko_x1000_count', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('big_ko_x100_count', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('big_ko_x100_count', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('big_ko_x100_count', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('big_ko_x100_count', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('big_ko_x10_count', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('big_ko_x10_count', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('big_ko_x10_count', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('big_ko_x10_count', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('big_ko_x1_5_count', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('big_ko_x1_5_count', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('big_ko_x1_5_count', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('big_ko_x1_5_count', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('big_ko_x2_count', 'summary_field', 'core.tournaments.buyin_total', ''),
    ('big_ko_x2_count', 'feature', 'derived.hand_eliminations.hero_ko_share_total', 'exact_core'),
    ('big_ko_x2_count', 'feature', 'derived.hand_eliminations.hero_share_fraction', 'exact_core'),
    ('big_ko_x2_count', 'feature', 'ref.mbr_mystery_envelopes.frequency_per_100m', 'reference_data'),
    ('winnings_from_itm', 'summary_field', 'core.tournament_entries.regular_prize_money', ''),
    ('winnings_from_ko_total', 'summary_field', 'core.tournament_entries.mystery_money_total', '')
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
    ('feature', 'is_ft_hand', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'is_stage_2', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'is_stage_3_4', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'is_stage_4_5', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'is_stage_5_6', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'is_stage_6_9', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'is_boundary_hand', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'has_exact_ko_event', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'has_split_ko_event', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'has_sidepot_ko_event', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'ft_table_size', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'ft_players_remaining_exact', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'hero_exact_ko_event_count', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'hero_split_ko_event_count', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'hero_sidepot_ko_event_count', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'ft_stage_bucket', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'best_hand_class', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'best_hand_rank_value', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'made_hand_category', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'draw_category', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'overcards_count', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'has_air', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'missed_flush_draw', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'missed_straight_draw', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('feature', 'certainty_state', 'mbr_runtime_v1', 'import_local_full_refresh', 'full_refresh'),
    ('stat', 'roi_pct', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'avg_finish_place', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'avg_finish_place_ft', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'avg_finish_place_no_ft', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'avg_ft_initial_stack_chips', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'avg_ft_initial_stack_bb', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'deep_ft_reach_percent', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'deep_ft_avg_stack_chips', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'deep_ft_avg_stack_bb', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'deep_ft_roi_pct', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'early_ft_bust_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'early_ft_bust_per_tournament', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'early_ft_ko_event_count', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'early_ft_ko_event_per_tournament', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'final_table_reach_percent', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'ft_stack_conversion', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ft_stack_conversion_3_4', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ft_stack_conversion_3_4_attempts', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ft_stack_conversion_5_6', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ft_stack_conversion_5_6_attempts', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ft_stack_conversion_7_9', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ft_stack_conversion_7_9_attempts', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'incomplete_ft_percent', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'itm_percent', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_contribution_adjusted_percent', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_contribution_percent', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_luck_money_delta', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_attempts_success_rate', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_2_3_event_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_2_3_attempts_per_tournament', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_2_3_money_total', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_3_4_event_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_3_4_attempts_per_tournament', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_3_4_money_total', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_4_5_event_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_4_5_attempts_per_tournament', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_4_5_money_total', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_5_6_event_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_5_6_attempts_per_tournament', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_5_6_money_total', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_6_9_event_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_6_9_money_total', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_7_9_event_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_7_9_attempts_per_tournament', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'ko_stage_7_9_money_total', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'pre_ft_chipev', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'pre_ft_ko_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'roi_adj_pct', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'roi_on_ft_pct', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'total_ko_event_count', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'avg_ko_attempts_per_ft', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'avg_ko_event_per_tournament', '', 'query_only_seed_snapshot', 'full_refresh'),
    ('stat', 'big_ko_x10000_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'big_ko_x1000_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'big_ko_x100_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'big_ko_x10_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'big_ko_x1_5_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'big_ko_x2_count', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'winnings_from_itm', '', 'query_only_canonical_snapshot', 'full_refresh'),
    ('stat', 'winnings_from_ko_total', '', 'query_only_canonical_snapshot', 'full_refresh')
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
