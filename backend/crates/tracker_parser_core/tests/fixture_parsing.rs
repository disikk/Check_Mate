use tracker_parser_core::{
    SourceKind, detect_source_kind,
    models::{ActionType, Street},
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
    assert!(!hand
        .parse_warnings
        .iter()
        .any(|warning| warning.contains("Dealt to f02e54a6")));
    assert!(!hand
        .parse_warnings
        .iter()
        .any(|warning| warning.contains("Total pot 3,984")));
    assert!(!hand
        .parse_warnings
        .iter()
        .any(|warning| warning.contains("Board [7d 2s 8h 2c Kh]")));
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
