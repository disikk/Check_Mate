use std::{fs, path::PathBuf};

use serde_json::to_value;
use tracker_parser_core::{
    ParserError, SourceKind, detect_source_kind, quick_detect_source_kind,
    quick_extract_gg_tournament_id,
    models::{ActionType, AllInReason, ParseIssue, ParseIssueCode, ParseIssueSeverity, Street},
    normalizer::normalize_hand,
    parsers::{
        hand_history::{parse_canonical_hand, parse_hand_header, split_hand_history},
        tournament_summary::parse_tournament_summary,
    },
};

const HH_RUSH: &str =
    include_str!("../../../fixtures/mbr/hh/GG20260316-0307 - Mystery Battle Royale 25.txt");
const HH_FT: &str =
    include_str!("../../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");
const TS_BUBBLE: &str = include_str!(
    "../../../fixtures/mbr/ts/GG20260316 - Tournament #271767530 - Mystery Battle Royale 25.txt"
);
const TS_WINNER: &str = include_str!(
    "../../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
);
const TS_TAIL_EXTRA: &str = include_str!(
    "../../../fixtures/mbr/ts/GG20260325 - Tournament #271770266 - Tail extra lines.txt"
);
const TS_TAIL_CONFLICT: &str =
    include_str!("../../../fixtures/mbr/ts/GG20260325 - Tournament #271770266 - Tail conflict.txt");
const TS_WINNER_FRACTIONAL_TAIL: &str = r#"Tournament #206882713, Mystery Battle Royale $10, Hold'em No Limit
Buy-in: $5+$0.8+$4.2
18 Players
Total Prize Pool: $165.6
Tournament started 2025/05/15 11:13:36
1st : Hero, $57.5
You finished the tournament in 1st place.
You received a total of $57.5.
"#;
const HH_LEGACY_HEADER_WITH_ANTE_IN_ACTIONS: &str = r#"Poker Hand #BR727555280: Tournament #206881479, Mystery Battle Royale $10 Hold'em No Limit - Level4(40/80) - 2025/05/15 11:05:11
Table '52' 5-max Seat #1 is the button
Seat 1: 9099365c (582 in chips)
Seat 2: 738a57ba (1,686 in chips)
Seat 3: 2a089a47 (770 in chips)
Seat 4: 2d655e5e (1,751 in chips)
Seat 5: Hero (836 in chips)
2d655e5e: posts the ante 16
738a57ba: posts the ante 16
Hero: posts the ante 16
9099365c: posts the ante 16
2a089a47: posts the ante 16
738a57ba: posts small blind 40
2a089a47: posts big blind 80
*** HOLE CARDS ***
Dealt to Hero [Js Jh]
Hero: folds
"#;

const HH_FIXTURE_FILES: &[&str] = &[
    "GG20260316-0307 - Mystery Battle Royale 25.txt",
    "GG20260316-0312 - Mystery Battle Royale 25.txt",
    "GG20260316-0316 - Mystery Battle Royale 25.txt",
    "GG20260316-0319 - Mystery Battle Royale 25.txt",
    "GG20260316-0323 - Mystery Battle Royale 25.txt",
    "GG20260316-0338 - Mystery Battle Royale 25.txt",
    "GG20260316-0342 - Mystery Battle Royale 25.txt",
    "GG20260316-0344 - Mystery Battle Royale 25.txt",
    "GG20260316-0351 - Mystery Battle Royale 25.txt",
];

const TS_FIXTURE_FILES: &[&str] = &[
    "GG20260316 - Tournament #271767530 - Mystery Battle Royale 25.txt",
    "GG20260316 - Tournament #271767841 - Mystery Battle Royale 25.txt",
    "GG20260316 - Tournament #271768265 - Mystery Battle Royale 25.txt",
    "GG20260316 - Tournament #271768505 - Mystery Battle Royale 25.txt",
    "GG20260316 - Tournament #271768917 - Mystery Battle Royale 25.txt",
    "GG20260316 - Tournament #271769484 - Mystery Battle Royale 25.txt",
    "GG20260316 - Tournament #271769772 - Mystery Battle Royale 25.txt",
    "GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
    "GG20260316 - Tournament #271771269 - Mystery Battle Royale 25.txt",
];

#[test]
fn detects_fixture_kinds() {
    assert_eq!(
        detect_source_kind(HH_RUSH).unwrap(),
        SourceKind::HandHistory
    );
    assert_eq!(
        detect_source_kind(TS_BUBBLE).unwrap(),
        SourceKind::TournamentSummary
    );
}

#[test]
fn quick_detector_matches_full_detector_on_committed_headers() {
    assert_eq!(
        quick_detect_source_kind(HH_FT).unwrap(),
        detect_source_kind(HH_FT).unwrap()
    );
    assert_eq!(
        quick_detect_source_kind(TS_WINNER).unwrap(),
        detect_source_kind(TS_WINNER).unwrap()
    );
}

#[test]
fn quick_extracts_tournament_id_from_hh_and_ts_headers() {
    assert_eq!(quick_extract_gg_tournament_id(HH_FT).unwrap(), Some(271_770_266));
    assert_eq!(
        quick_extract_gg_tournament_id(TS_WINNER).unwrap(),
        Some(271_770_266)
    );
    assert_eq!(
        quick_extract_gg_tournament_id(TS_TAIL_CONFLICT).unwrap(),
        Some(271_770_266)
    );
}

#[test]
fn quick_extract_surfaces_missing_and_unsupported_tournament_headers() {
    const HH_MISSING_TOURNAMENT_ID: &str =
        "Poker Hand #BR727555280: Hold'em No Limit - Level4(40/80) - 2025/05/15 11:05:11";
    const TS_MISSING_TOURNAMENT_ID: &str = "Tournament #, Mystery Battle Royale $25, Hold'em No Limit";

    assert_eq!(
        quick_extract_gg_tournament_id(HH_MISSING_TOURNAMENT_ID).unwrap(),
        None
    );
    assert_eq!(
        quick_extract_gg_tournament_id(TS_MISSING_TOURNAMENT_ID).unwrap(),
        None
    );
    assert!(matches!(
        quick_extract_gg_tournament_id("not a gg export"),
        Err(ParserError::UnsupportedSourceFormat)
    ));
}

#[test]
fn parses_tournament_summary_fixture() {
    let summary = parse_tournament_summary(TS_WINNER).unwrap();

    assert_eq!(summary.tournament_id, 271_770_266);
    assert_eq!(summary.tournament_name, "Mystery Battle Royale $25");
    assert_eq!(summary.game_name, "Hold'em No Limit");
    assert_eq!(summary.buy_in_cents, 1_250);
    assert_eq!(summary.rake_cents, 200);
    assert_eq!(summary.bounty_cents, 1_050);
    assert_eq!(summary.entrants, 18);
    assert_eq!(summary.total_prize_pool_cents, 41_400);
    assert_eq!(summary.started_at, "2026/03/16 10:44:11");
    assert_eq!(summary.hero_name, "Hero");
    assert_eq!(summary.finish_place, 1);
    assert_eq!(summary.payout_cents, 20_500);
    assert_eq!(summary.confirmed_finish_place, Some(1));
    assert_eq!(summary.confirmed_payout_cents, Some(20_500));
    assert!(summary.parse_issues.is_empty());
}

#[test]
fn parses_tournament_summary_tail_confirmations_with_harmless_extra_lines() {
    let summary = parse_tournament_summary(TS_TAIL_EXTRA).unwrap();

    assert_eq!(summary.buy_in_cents, 1_250);
    assert_eq!(summary.rake_cents, 200);
    assert_eq!(summary.bounty_cents, 1_050);
    assert_eq!(summary.finish_place, 1);
    assert_eq!(summary.payout_cents, 20_500);
    assert_eq!(summary.confirmed_finish_place, Some(1));
    assert_eq!(summary.confirmed_payout_cents, Some(20_500));
    assert!(summary.parse_issues.is_empty());
}

#[test]
fn surfaces_tournament_summary_tail_conflicts_as_validation_issues() {
    let summary = parse_tournament_summary(TS_TAIL_CONFLICT).unwrap();

    assert_eq!(summary.finish_place, 1);
    assert_eq!(summary.payout_cents, 20_500);
    assert_eq!(summary.confirmed_finish_place, Some(2));
    assert_eq!(summary.confirmed_payout_cents, Some(20_400));
    assert_eq!(
        summary
            .parse_issues
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        vec![
            ParseIssueCode::TsTailFinishPlaceMismatch,
            ParseIssueCode::TsTailTotalReceivedMismatch,
        ]
    );
}

#[test]
fn parses_fractional_tournament_summary_tail_payout_with_sentence_period() {
    let summary = parse_tournament_summary(TS_WINNER_FRACTIONAL_TAIL).unwrap();

    assert_eq!(summary.finish_place, 1);
    assert_eq!(summary.payout_cents, 5_750);
    assert_eq!(summary.confirmed_finish_place, Some(1));
    assert_eq!(summary.confirmed_payout_cents, Some(5_750));
    assert!(summary.parse_issues.is_empty());
}

#[test]
fn parses_legacy_hand_header_without_inline_ante_by_inferring_from_actions() {
    let header = parse_hand_header(HH_LEGACY_HEADER_WITH_ANTE_IN_ACTIONS).unwrap();

    assert_eq!(header.hand_id, "BR727555280");
    assert_eq!(header.tournament_id, 206_881_479);
    assert_eq!(header.level_name, "Level4");
    assert_eq!(header.small_blind, 40);
    assert_eq!(header.big_blind, 80);
    assert_eq!(header.ante, 16);
    assert_eq!(header.played_at, "2025/05/15 11:05:11");
    assert_eq!(header.table_name, "52");
    assert_eq!(header.max_players, 5);
    assert_eq!(header.button_seat, 1);
}

#[test]
fn splits_and_parses_rush_hand_history_fixture() {
    let hands = split_hand_history(HH_RUSH).unwrap();

    assert_eq!(hands.len(), 20);
    assert_eq!(hands[0].header.hand_id, "BR1064992721");
    assert_eq!(hands[0].header.tournament_id, 271_767_530);
    assert_eq!(hands[0].header.level_name, "Level5");
    assert_eq!(hands[0].header.small_blind, 50);
    assert_eq!(hands[0].header.big_blind, 100);
    assert_eq!(hands[0].header.ante, 20);
    assert_eq!(hands[0].header.played_at, "2026/03/16 10:15:46");
    assert_eq!(hands[0].header.table_name, "52");
    assert_eq!(hands[0].header.max_players, 5);
    assert_eq!(hands[0].header.button_seat, 1);
}

#[test]
fn parses_first_ft_hand_header_from_short_handed_table() {
    let first_hand = HH_FT.split("\n\n").next().unwrap();
    let header = parse_hand_header(first_hand).unwrap();

    assert_eq!(header.hand_id, "BR1064987693");
    assert_eq!(header.tournament_id, 271_770_266);
    assert_eq!(header.level_name, "Level10");
    assert_eq!(header.small_blind, 200);
    assert_eq!(header.big_blind, 400);
    assert_eq!(header.ante, 80);
    assert_eq!(header.played_at, "2026/03/16 11:07:34");
    assert_eq!(header.table_name, "1");
    assert_eq!(header.max_players, 9);
    assert_eq!(header.button_seat, 3);
}

#[test]
fn parses_canonical_rush_hand_for_replay_primitives() {
    let first_hand = HH_RUSH.split("\n\n").next().unwrap();
    let hand = parse_canonical_hand(first_hand).unwrap();

    assert_eq!(hand.seats.len(), 5);
    assert_eq!(hand.hero_name.as_deref(), Some("Hero"));
    assert_eq!(
        hand.hero_hole_cards,
        Some(vec!["Kc".to_string(), "Ad".to_string()])
    );
    assert_eq!(
        hand.board_final,
        vec![
            "7c".to_string(),
            "4s".to_string(),
            "3h".to_string(),
            "Th".to_string(),
            "As".to_string()
        ]
    );
    assert_eq!(hand.collected_amounts.get("5d455a01"), Some(&4_620));
    assert_eq!(hand.actions[0].action_type, ActionType::PostAnte);
    assert_eq!(hand.actions[0].street, Street::Preflop);
    assert_eq!(hand.actions[0].player_name.as_deref(), Some("5d455a01"));
    assert_eq!(
        hand.actions.last().unwrap().action_type,
        ActionType::Collect
    );
}

#[test]
fn parses_call_actions_with_delta_amount_and_without_to_amount() {
    let first_hand = HH_RUSH.split("\n\n").next().unwrap();
    let hand = parse_canonical_hand(first_hand).unwrap();
    let call_actions = hand
        .actions
        .iter()
        .filter(|event| event.action_type == ActionType::Call)
        .collect::<Vec<_>>();

    assert_eq!(call_actions.len(), 4);
    assert_eq!(call_actions[0].player_name.as_deref(), Some("5d455a01"));
    assert_eq!(call_actions[0].amount, Some(200));
    assert!(call_actions.iter().all(|event| event.to_amount.is_none()));
}

#[test]
fn keeps_short_handed_ft_state_inside_nine_max_header() {
    let first_hand = HH_FT.split("\n\n").next().unwrap();
    let hand = parse_canonical_hand(first_hand).unwrap();

    assert_eq!(hand.header.max_players, 9);
    assert_eq!(hand.seats.len(), 2);
    assert_eq!(
        hand.board_final,
        vec![
            "7d".to_string(),
            "2s".to_string(),
            "8h".to_string(),
            "2c".to_string(),
            "Kh".to_string()
        ]
    );
    assert!(
        hand.actions
            .iter()
            .any(|event| event.action_type == ActionType::RaiseTo && event.is_all_in)
    );
    assert!(
        hand.actions
            .iter()
            .any(|event| event.action_type == ActionType::Show)
    );
}

#[test]
fn parses_summary_fields_without_false_positive_warnings() {
    let first_hand = HH_FT.split("\n\n").next().unwrap();
    let hand = parse_canonical_hand(first_hand).unwrap();

    assert_eq!(hand.summary_total_pot, Some(3_984));
    assert_eq!(hand.summary_rake_amount, Some(0));
    assert_eq!(
        hand.summary_board,
        vec![
            "7d".to_string(),
            "2s".to_string(),
            "8h".to_string(),
            "2c".to_string(),
            "Kh".to_string()
        ]
    );
    assert!(
        !hand
            .parse_issues
            .iter()
            .any(|issue| issue.message.contains("Dealt to f02e54a6"))
    );
    assert!(
        !hand
            .parse_issues
            .iter()
            .any(|issue| issue.message.contains("Total pot 3,984"))
    );
    assert!(
        !hand
            .parse_issues
            .iter()
            .any(|issue| issue.message.contains("Board [7d 2s 8h 2c Kh]"))
    );
}

#[test]
fn accumulates_repeated_collect_lines_for_same_player() {
    let hand_text = split_hand_history(HH_FT)
        .unwrap()
        .into_iter()
        .find(|hand| hand.header.hand_id == "BR1064987148")
        .map(|hand| hand.raw_text)
        .unwrap();
    let hand = parse_canonical_hand(&hand_text).unwrap();

    assert_eq!(hand.collected_amounts.get("aaab99dd"), Some(&6_900));
    assert_eq!(
        hand.actions
            .iter()
            .filter(|event| {
                event.action_type == ActionType::Collect
                    && event.player_name.as_deref() == Some("aaab99dd")
            })
            .count(),
        2
    );
}

#[test]
fn parses_committed_ante_hidden_dealt_and_ranked_show_surface_without_warnings() {
    let raw = read_fixture("hh", "GG20260316-0338 - Mystery Battle Royale 25.txt");
    let hand_text = find_hand_text(&raw, "Hero: posts the ante 60");
    let hand = parse_canonical_hand(&hand_text).unwrap();

    assert!(
        hand.actions
            .iter()
            .any(|event| event.action_type == ActionType::PostAnte)
    );
    assert_eq!(hand.hero_name.as_deref(), Some("Hero"));
    assert_eq!(
        hand.hero_hole_cards,
        Some(vec!["Qh".to_string(), "Kh".to_string()])
    );
    assert_eq!(
        hand.showdown_hands.get("Hero"),
        Some(&vec!["Qh".to_string(), "Kh".to_string()])
    );
    assert_eq!(
        hand.showdown_hands.get("ae7eda73"),
        Some(&vec!["2s".to_string(), "6c".to_string()])
    );
    assert_eq!(hand.summary_total_pot, Some(1_944));
    assert_eq!(hand.summary_rake_amount, Some(0));
    assert_eq!(hand.summary_board.len(), 5);
    assert!(hand.parse_issues.is_empty());
}

#[test]
fn parses_committed_check_bet_uncalled_and_collect_surface_without_warnings() {
    let raw = read_fixture("hh", "GG20260316-0316 - Mystery Battle Royale 25.txt");
    let hand_text = find_hand_text(&raw, "Hero: bets 73");
    let hand = parse_canonical_hand(&hand_text).unwrap();

    assert!(
        hand.actions
            .iter()
            .any(|event| event.action_type == ActionType::Check)
    );
    assert!(
        hand.actions
            .iter()
            .any(|event| event.action_type == ActionType::Bet)
    );
    assert!(
        hand.actions
            .iter()
            .any(|event| event.action_type == ActionType::ReturnUncalled)
    );
    assert!(
        hand.actions
            .iter()
            .any(|event| event.action_type == ActionType::Collect)
    );
    assert_eq!(hand.collected_amounts.get("Hero"), Some(&220));
    assert!(hand.parse_issues.is_empty());
}

#[test]
fn parses_summary_seat_result_lines_into_structured_outcomes() {
    let hand = parse_canonical_hand(&summary_outcome_hand_text()).unwrap();

    assert_eq!(hand.summary_seat_outcomes.len(), 9);
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 1)
            .unwrap()
            .position_marker,
        Some(tracker_parser_core::models::SummarySeatMarker::Button)
    );
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 1)
            .unwrap()
            .outcome_kind,
        tracker_parser_core::models::SummarySeatOutcomeKind::Won
    );
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 2)
            .unwrap()
            .folded_at,
        Some(Street::Preflop)
    );
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 3)
            .unwrap()
            .folded_at,
        Some(Street::Flop)
    );
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 4)
            .unwrap()
            .outcome_kind,
        tracker_parser_core::models::SummarySeatOutcomeKind::ShowedLost
    );
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 4)
            .unwrap()
            .shown_cards,
        Some(vec!["Qh".to_string(), "Kh".to_string()])
    );
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 5)
            .unwrap()
            .outcome_kind,
        tracker_parser_core::models::SummarySeatOutcomeKind::ShowedWon
    );
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 5)
            .unwrap()
            .won_amount,
        Some(1_944)
    );
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 6)
            .unwrap()
            .outcome_kind,
        tracker_parser_core::models::SummarySeatOutcomeKind::Lost
    );
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 7)
            .unwrap()
            .outcome_kind,
        tracker_parser_core::models::SummarySeatOutcomeKind::Mucked
    );
    assert_eq!(
        hand.summary_seat_outcomes
            .iter()
            .find(|outcome| outcome.seat_no == 8)
            .unwrap()
            .outcome_kind,
        tracker_parser_core::models::SummarySeatOutcomeKind::Collected
    );
    assert!(
        hand.summary_seat_outcomes
            .iter()
            .any(|outcome| outcome.seat_no == 2 && outcome.player_name == "Hero")
    );
    assert!(hand.parse_issues.iter().any(|issue| {
        issue.code == ParseIssueCode::UnparsedSummarySeatTail
            && issue.severity == ParseIssueSeverity::Warning
            && issue.raw_line.as_deref() == Some("Seat 9: VillainX (button) ???")
    }));
}

#[test]
fn unknown_summary_tail_uses_tail_specific_warning_when_head_is_valid() {
    let hand = parse_canonical_hand(&summary_unknown_tail_hand_text()).unwrap();

    assert_eq!(hand.summary_seat_outcomes.len(), 2);
    assert!(
        hand.summary_seat_outcomes
            .iter()
            .any(|outcome| outcome.seat_no == 2
                && outcome.outcome_kind
                    == tracker_parser_core::models::SummarySeatOutcomeKind::Folded)
    );
    assert!(
        hand.summary_seat_outcomes
            .iter()
            .any(|outcome| outcome.seat_no == 3
                && outcome.outcome_kind
                    == tracker_parser_core::models::SummarySeatOutcomeKind::Collected)
    );
    assert!(
        hand.parse_issues.iter().any(|issue| {
            issue.code == ParseIssueCode::UnparsedSummarySeatTail
                && issue.raw_line.as_deref() == Some("Seat 1: Hero (button) celebrated the win")
        }),
        "expected tail-specific warning, got {:?}",
        hand.parse_issues
    );
    assert!(
        !hand.parse_issues.iter().any(|issue| {
            issue.code == ParseIssueCode::UnparsedSummarySeatLine
                && issue.raw_line.as_deref() == Some("Seat 1: Hero (button) celebrated the win")
        }),
        "head-valid unknown tail must not downgrade to line-level warning: {:?}",
        hand.parse_issues
    );
}

#[test]
fn parses_showed_collected_summary_tail_as_showed_won_surface() {
    let hand = parse_canonical_hand(&summary_showed_collected_hand_text()).unwrap();
    let outcome = hand
        .summary_seat_outcomes
        .iter()
        .find(|outcome| outcome.seat_no == 1)
        .unwrap();

    assert_eq!(
        outcome.outcome_kind,
        tracker_parser_core::models::SummarySeatOutcomeKind::ShowedWon
    );
    assert_eq!(
        outcome.shown_cards,
        Some(vec!["Ah".to_string(), "Ad".to_string()])
    );
    assert_eq!(outcome.won_amount, Some(300));
    assert!(hand.parse_issues.is_empty(), "got {:?}", hand.parse_issues);
}

#[test]
fn parses_post_dead_muck_and_sitting_out_surface() {
    let hand = parse_canonical_hand(&cm04_parser_surface_hand_text()).unwrap();

    assert!(
        hand.seats
            .iter()
            .find(|seat| seat.seat_no == 2)
            .unwrap()
            .is_sitting_out
    );
    assert!(hand.actions.iter().any(|event| {
        event.player_name.as_deref() == Some("VillainDead")
            && event.action_type == ActionType::PostDead
            && event.amount == Some(100)
    }));
    assert!(hand.actions.iter().any(|event| {
        event.player_name.as_deref() == Some("VillainMuck") && event.action_type == ActionType::Muck
    }));
}

#[test]
fn marks_no_show_and_partial_reveal_surface_explicitly() {
    let hand = parse_canonical_hand(&cm04_show_surface_hand_text()).unwrap();

    assert!(hand.parse_issues.iter().any(|issue| {
        issue.code == ParseIssueCode::PartialRevealShowLine
            && issue.raw_line.as_deref() == Some("VillainPartial: shows [5d]")
    }));
    assert!(hand.parse_issues.iter().any(|issue| {
        issue.code == ParseIssueCode::UnsupportedNoShowLine
            && issue.raw_line.as_deref() == Some("VillainNoShow: doesn't show hand")
    }));
    assert!(!hand.parse_issues.iter().any(|issue| {
        issue.code == ParseIssueCode::UnparsedLine
            && matches!(
                issue.raw_line.as_deref(),
                Some("VillainPartial: shows [5d]" | "VillainNoShow: doesn't show hand")
            )
    }));
}

#[test]
fn annotates_forced_all_in_reasons_for_ante_and_blind_exhaustion() {
    let ante_hand = parse_canonical_hand(&cm04_ante_exhausted_hand_text()).unwrap();
    let blind_hand = parse_canonical_hand(&cm04_blind_exhausted_hand_text()).unwrap();

    let ante_action = ante_hand
        .actions
        .iter()
        .find(|event| {
            event.player_name.as_deref() == Some("ShortAnte")
                && event.action_type == ActionType::PostAnte
        })
        .unwrap();
    assert!(ante_action.is_all_in);
    assert_eq!(ante_action.all_in_reason, Some(AllInReason::AnteExhausted));
    assert!(ante_action.forced_all_in_preflop);

    let blind_action = blind_hand
        .actions
        .iter()
        .find(|event| {
            event.player_name.as_deref() == Some("ShortBlind")
                && event.action_type == ActionType::PostSb
        })
        .unwrap();
    assert!(blind_action.is_all_in);
    assert_eq!(
        blind_action.all_in_reason,
        Some(AllInReason::BlindExhausted)
    );
    assert!(blind_action.forced_all_in_preflop);
}

#[test]
fn preserves_unknown_non_seat_lines_as_unparsed_warnings() {
    let hand = parse_canonical_hand(
        r#"Poker Hand #BRPARSER1: Tournament #999101, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:30:00
Table '1' 2-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: Villain (1,000 in chips)
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Dealer note: this line is not part of the parser contract
Hero: raises 100 to 200
Villain: folds
Uncalled bet (100) returned to Hero
*** SHOWDOWN ***
Hero collected 200 from pot
*** SUMMARY ***
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Seat 1: Hero collected (200)
Seat 2: Villain folded before Flop"#,
    )
    .unwrap();

    assert_eq!(
        hand.parse_issues,
        vec![ParseIssue {
            severity: ParseIssueSeverity::Warning,
            code: ParseIssueCode::UnparsedLine,
            message: "unparsed_line: Dealer note: this line is not part of the parser contract"
                .to_string(),
            raw_line: Some("Dealer note: this line is not part of the parser contract".to_string()),
            payload: None,
        }]
    );
}

#[test]
fn surfaces_malformed_dealt_line_as_explicit_parse_issue() {
    let hand = parse_canonical_hand(&malformed_dealt_line_hand_text()).unwrap();

    assert!(hand.parse_issues.iter().any(|issue| {
        issue.raw_line.as_deref() == Some("Dealt to Hero [Ah Ad")
            && issue
                .message
                .starts_with("malformed_dealt_to_line: Dealt to Hero [Ah Ad")
            && to_value(issue)
                .unwrap()
                .get("code")
                .and_then(|value| value.as_str())
                == Some("malformed_dealt_to_line")
    }));
}

#[test]
fn preserves_hero_name_on_hidden_hero_dealt_surface() {
    let hand = parse_canonical_hand(&hidden_hero_dealt_hand_text()).unwrap();

    assert_eq!(hand.hero_name.as_deref(), Some("Hero"));
    assert_eq!(hand.hero_hole_cards, None);
    assert!(
        hand.parse_issues.is_empty(),
        "hidden hero dealt line must stay parser-clean: {:?}",
        hand.parse_issues
    );
}

#[test]
fn normalizes_hidden_hero_dealt_surface_without_missing_hero_name() {
    let hand = parse_canonical_hand(&hidden_hero_dealt_hand_text()).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(normalized.hand_id, "BRHIDDENHERO1");
}

#[test]
fn parses_all_committed_tournament_summary_fixtures() {
    for fixture in TS_FIXTURE_FILES {
        let raw = read_fixture("ts", fixture);
        let summary = parse_tournament_summary(&raw)
            .unwrap_or_else(|error| panic!("fixture `{fixture}` failed to parse: {error}"));

        assert!(
            summary.tournament_id > 0,
            "fixture `{fixture}` has no tournament id"
        );
        assert!(
            summary.finish_place >= 1,
            "fixture `{fixture}` has invalid finish place"
        );
        assert!(
            summary.entrants >= summary.finish_place,
            "fixture `{fixture}` has finish_place beyond entrants"
        );
        assert!(
            summary.parse_issues.is_empty(),
            "fixture `{fixture}` has unexpected TS validation issues: {:?}",
            summary.parse_issues
        );
    }
}

#[test]
fn parses_all_committed_hand_history_fixtures_without_unexpected_warnings() {
    let mut unexpected = Vec::new();

    for fixture in HH_FIXTURE_FILES {
        let raw = read_fixture("hh", fixture);
        let hands = split_hand_history(&raw)
            .unwrap_or_else(|error| panic!("fixture `{fixture}` failed to split: {error}"));

        for hand in hands {
            let parsed = parse_canonical_hand(&hand.raw_text).unwrap_or_else(|error| {
                panic!(
                    "fixture `{fixture}` hand `{}` failed to parse: {error}",
                    hand.header.hand_id
                )
            });

            let unexpected_warnings = parsed
                .parse_issues
                .iter()
                .filter(|issue| !is_expected_explicit_surface_issue(issue))
                .map(|issue| format!("{:?}", issue))
                .collect::<Vec<_>>();

            if !unexpected_warnings.is_empty() {
                unexpected.push(format!(
                    "{fixture} :: {} :: {:?}",
                    parsed.header.hand_id, unexpected_warnings
                ));
            }
        }
    }

    assert!(
        unexpected.is_empty(),
        "unexpected parser warnings across committed HH fixtures:\n{}",
        unexpected.join("\n")
    );
}

fn summary_outcome_hand_text() -> String {
    r#"Poker Hand #BRSUMMARY1: Tournament #999101, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:30:00
Table '1' 8-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainA (1,000 in chips)
Seat 3: VillainB (1,000 in chips)
Seat 4: VillainC (1,000 in chips)
Seat 5: VillainD (1,000 in chips)
Seat 6: VillainE (1,000 in chips)
Seat 7: VillainF (1,000 in chips)
Seat 8: VillainG (1,000 in chips)
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
*** SHOWDOWN ***
Hero collected 110 from pot
*** SUMMARY ***
Total pot 3,454 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) won (110)
Seat 2: VillainA (small blind) folded before Flop
Seat 3: VillainB (big blind) folded on the Flop
Seat 4: VillainC showed [Qh Kh] and lost with a pair of Kings
    Seat 5: VillainD showed [2s 6c] and won (1,944) with two pair, Sixes and Twos
Seat 6: VillainE lost
Seat 7: VillainF mucked
Seat 8: VillainG collected (200)
Seat 2: Hero lost
Seat 9: VillainX (button) ???"#.to_string()
}

fn summary_unknown_tail_hand_text() -> String {
    r#"Poker Hand #BRSUMTAIL1: Tournament #999102, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:31:00
Table '1' 3-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainA (1,000 in chips)
Seat 3: VillainB (1,000 in chips)
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
*** SHOWDOWN ***
VillainB collected 300 from pot
*** SUMMARY ***
Total pot 300 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) celebrated the win
Seat 2: VillainA (small blind) folded before Flop
Seat 3: VillainB (big blind) collected (300)"#
        .to_string()
}

fn summary_showed_collected_hand_text() -> String {
    r#"Poker Hand #BRSUMTAIL2: Tournament #999103, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:32:00
Table '1' 2-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: Villain (1,000 in chips)
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
*** SHOWDOWN ***
Hero collected 300 from pot
*** SUMMARY ***
Total pot 300 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and collected (300)
Seat 2: Villain lost"#
        .to_string()
}

fn cm04_parser_surface_hand_text() -> String {
    r#"Poker Hand #BRCM0401: Tournament #999201, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 13:00:00
Table '1' 4-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: Sitout (1,000 in chips) is sitting out
Seat 3: VillainDead (1,000 in chips)
Seat 4: VillainMuck (1,000 in chips)
VillainDead: posts dead 100
VillainMuck: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Hero: folds
VillainMuck: mucks hand
*** SHOWDOWN ***
VillainDead collected 200 from pot
*** SUMMARY ***
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Seat 1: Hero folded before Flop
Seat 3: VillainDead collected (200)
Seat 4: VillainMuck mucked"#.to_string()
}

fn cm04_show_surface_hand_text() -> String {
    r#"Poker Hand #BRCM0402: Tournament #999202, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 13:05:00
Table '1' 3-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainPartial (1,000 in chips)
Seat 3: VillainNoShow (1,000 in chips)
VillainPartial: posts small blind 50
VillainNoShow: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Hero: calls 100
VillainPartial: calls 50
VillainNoShow: checks
*** FLOP *** [2c 7d 9h]
VillainPartial: checks
VillainNoShow: checks
Hero: checks
*** TURN *** [2c 7d 9h] [Qs]
VillainPartial: checks
VillainNoShow: checks
Hero: checks
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
VillainPartial: shows [5d]
VillainNoShow: doesn't show hand
Hero: shows [Ah Ad]
Hero collected 300 from pot
*** SUMMARY ***
Total pot 300 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and won (300)
Seat 2: VillainPartial (small blind) showed [5d] and lost
Seat 3: VillainNoShow (big blind) lost"#.to_string()
}

fn cm04_ante_exhausted_hand_text() -> String {
    r#"Poker Hand #BRCM0403: Tournament #999203, Mystery Battle Royale $25 Hold'em No Limit - Level1(0/0(100)) - 2026/03/16 13:10:00
Table '1' 2-max Seat #1 is the button
Seat 1: ShortAnte (100 in chips)
Seat 2: Hero (1,000 in chips)
ShortAnte: posts the ante 100
Hero: posts the ante 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Hero: checks
*** FLOP *** [2c 7d 9h]
*** TURN *** [2c 7d 9h] [Qs]
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
ShortAnte: shows [Kd Kh]
Hero: shows [Ah Ad]
Hero collected 200 from pot
*** SUMMARY ***
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: ShortAnte (button) showed [Kd Kh] and lost
Seat 2: Hero showed [Ah Ad] and won (200)"#.to_string()
}

fn cm04_blind_exhausted_hand_text() -> String {
    r#"Poker Hand #BRCM0404: Tournament #999204, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 13:15:00
Table '1' 2-max Seat #1 is the button
Seat 1: ShortBlind (50 in chips)
Seat 2: Hero (1,000 in chips)
ShortBlind: posts small blind 50
Hero: posts big blind 100
*** HOLE CARDS ***
Dealt to ShortBlind
Dealt to Hero [Ah Ad]
Uncalled bet (50) returned to Hero
*** FLOP *** [2c 7d 9h]
*** TURN *** [2c 7d 9h] [Qs]
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
ShortBlind: shows [Kd Kh]
Hero: shows [Ah Ad]
Hero collected 100 from pot
*** SUMMARY ***
Total pot 100 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: ShortBlind (button) showed [Kd Kh] and lost
Seat 2: Hero (big blind) showed [Ah Ad] and won (100)"#.to_string()
}

fn malformed_dealt_line_hand_text() -> String {
    r#"Poker Hand #BRMALFORMDEALT1: Tournament #999205, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 13:20:00
Table '1' 2-max Seat #1 is the button
Seat 1: Villain (1,000 in chips)
Seat 2: Hero (1,000 in chips)
Villain: posts small blind 50
Hero: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad
Villain: folds
Uncalled bet (50) returned to Hero
*** SHOWDOWN ***
Hero collected 100 from pot
*** SUMMARY ***
Total pot 100 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Seat 1: Villain (button) folded before Flop
Seat 2: Hero (big blind) collected (100)"#
        .to_string()
}

fn hidden_hero_dealt_hand_text() -> String {
    r#"Poker Hand #BRHIDDENHERO1: Tournament #999206, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 13:25:00
Table '1' 2-max Seat #1 is the button
Seat 1: Villain (1,000 in chips)
Seat 2: Hero (1,000 in chips)
Villain: posts small blind 50
Hero: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero
Villain: folds
Uncalled bet (50) returned to Hero
*** SHOWDOWN ***
Hero collected 100 from pot
*** SUMMARY ***
Total pot 100 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Seat 1: Villain (button) folded before Flop
Seat 2: Hero (big blind) collected (100)"#
        .to_string()
}

fn read_fixture(kind: &str, filename: &str) -> String {
    fs::read_to_string(fixture_path(kind, filename)).unwrap()
}

fn find_hand_text(raw: &str, needle: &str) -> String {
    split_hand_history(raw)
        .unwrap()
        .into_iter()
        .find(|hand| hand.raw_text.contains(needle))
        .map(|hand| hand.raw_text)
        .unwrap_or_else(|| panic!("no hand in fixture contains `{needle}`"))
}

fn fixture_path(kind: &str, filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/mbr/{kind}/{filename}"))
}

fn is_expected_explicit_surface_issue(issue: &ParseIssue) -> bool {
    matches!(
        issue.code,
        ParseIssueCode::PartialRevealShowLine | ParseIssueCode::PartialRevealSummaryShowSurface
    )
}
