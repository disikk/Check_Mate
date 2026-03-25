use std::{fs, path::PathBuf};

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
    assert!(
        !hand
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Dealt to f02e54a6"))
    );
    assert!(
        !hand
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Total pot 3,984"))
    );
    assert!(
        !hand
            .parse_warnings
            .iter()
            .any(|warning| warning.contains("Board [7d 2s 8h 2c Kh]"))
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
    assert_eq!(hand.showdown_hands.get("Hero"), Some(&vec!["Qh".to_string(), "Kh".to_string()]));
    assert_eq!(
        hand.showdown_hands.get("ae7eda73"),
        Some(&vec!["2s".to_string(), "6c".to_string()])
    );
    assert_eq!(hand.summary_total_pot, Some(1_944));
    assert_eq!(hand.summary_rake_amount, Some(0));
    assert_eq!(hand.summary_board.len(), 5);
    assert!(hand.parse_warnings.is_empty());
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
    assert!(hand.parse_warnings.is_empty());
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
        hand.parse_warnings,
        vec!["unparsed_line: Dealer note: this line is not part of the parser contract"]
    );
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

            if !parsed.parse_warnings.is_empty() {
                unexpected.push(format!(
                    "{fixture} :: {} :: {:?}",
                    parsed.header.hand_id, parsed.parse_warnings
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
