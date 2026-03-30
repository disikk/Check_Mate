mod support;

use std::{collections::BTreeMap, env, fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use support::street_strength_reference::reference_draw_rows_for_seat;
use tracker_parser_core::{
    parsers::hand_history::parse_canonical_hand, street_strength::evaluate_street_hand_strength,
};

#[test]
fn curated_states_match_independent_reference_draw_surface() {
    let hand = parse_canonical_hand(&showdown_hand(
        "BRSTRREF001",
        ["Jh", "Th", "2h"],
        Some("3h"),
        Some("As"),
        "Hero",
        "[8h 7d]",
        "Villain",
        "[Ac Ad]",
        "Hero collected 200 from pot",
        "Seat 1: Villain (small blind) showed [Ac Ad] and lost with a pair of Aces\nSeat 2: Hero (big blind) showed [8h 7d] and collected (200) with a flush, Jack high",
    ))
    .unwrap();

    assert_draw_surface_matches_reference(&hand, 2);
}

#[test]
fn curated_backdoor_and_missed_cases_match_independent_reference_draw_surface() {
    let hand = parse_canonical_hand(&showdown_hand(
        "BRSTRREF002",
        ["Qc", "7h", "2d"],
        Some("4s"),
        Some("3h"),
        "Hero",
        "[Ac Kc]",
        "Villain",
        "[9c 9d]",
        "Villain collected 200 from pot",
        "Seat 1: Villain (small blind) showed [9c 9d] and collected (200)\nSeat 2: Hero (big blind) showed [Ac Kc] and lost",
    ))
    .unwrap();

    assert_draw_surface_matches_reference(&hand, 2);
}

#[test]
fn randomized_flop_and_turn_states_match_independent_reference_draw_surface() {
    let mut rng = DeterministicRng::new(0xC0FFEE_u64);

    for case_index in 0..24 {
        let state = sample_unique_cards(&mut rng, if case_index % 2 == 0 { 7 } else { 8 });
        let hero_cards = [state[0].as_str(), state[1].as_str()];
        let villain_cards = [state[2].as_str(), state[3].as_str()];
        let flop = [state[4].as_str(), state[5].as_str(), state[6].as_str()];
        let turn = (case_index % 2 == 1).then(|| state[7].as_str());
        let hand = parse_canonical_hand(&showdown_hand(
            &format!("BRSTRRAND{case_index:03}"),
            flop,
            turn,
            None,
            "Hero",
            &format!("[{} {}]", hero_cards[0], hero_cards[1]),
            "Villain",
            &format!("[{} {}]", villain_cards[0], villain_cards[1]),
            if case_index % 3 == 0 {
                "Hero collected 200 from pot"
            } else {
                "Villain collected 200 from pot"
            },
            &format!(
                "Seat 1: Villain (small blind) showed [{} {}] and {}\nSeat 2: Hero (big blind) showed [{} {}] and {}",
                villain_cards[0],
                villain_cards[1],
                if case_index % 3 == 0 { "lost" } else { "collected (200)" },
                hero_cards[0],
                hero_cards[1],
                if case_index % 3 == 0 { "collected (200)" } else { "lost" },
            ),
        ))
        .unwrap();

        assert_draw_surface_matches_reference(&hand, 2);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GoldenDrawRow {
    street: String,
    draw_category: String,
    missed_flush_draw: bool,
    missed_straight_draw: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GoldenCaseSnapshot {
    hero_cards: Vec<String>,
    board: Vec<String>,
    rows: Vec<GoldenDrawRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StreetStrengthGoldenSnapshot {
    cases: BTreeMap<String, GoldenCaseSnapshot>,
}

#[test]
fn street_strength_golden_matches_committed_reference_cases() {
    let actual = build_street_strength_golden_snapshot();
    let golden_path = street_strength_golden_path();

    if env::var("UPDATE_GOLDENS").is_ok() {
        fs::create_dir_all(
            golden_path
                .parent()
                .expect("golden path must have parent directory"),
        )
        .expect("golden directory must be creatable");
        let json = serde_json::to_string_pretty(&actual).expect("golden JSON must serialize");
        fs::write(&golden_path, json).expect("golden file must be writable");
        eprintln!("Golden updated at: {}", golden_path.display());
        return;
    }

    assert!(
        golden_path.exists(),
        "Missing street-strength golden at {}.\nRun with UPDATE_GOLDENS=1 cargo test -p tracker_parser_core --test street_strength_reference",
        golden_path.display()
    );

    let expected_json = fs::read_to_string(&golden_path).expect("golden file must be readable");
    let expected: StreetStrengthGoldenSnapshot =
        serde_json::from_str(&expected_json).expect("golden JSON must parse");

    assert_eq!(actual, expected);
}

fn assert_draw_surface_matches_reference(
    hand: &tracker_parser_core::models::CanonicalParsedHand,
    seat_no: u8,
) {
    let production_rows = evaluate_street_hand_strength(hand).unwrap();
    let reference_rows = reference_draw_rows_for_seat(hand, seat_no);

    let production_draw_surface = production_rows
        .iter()
        .filter(|row| row.seat_no == seat_no)
        .map(|row| {
            (
                row.street,
                row.draw_category,
                row.missed_flush_draw,
                row.missed_straight_draw,
            )
        })
        .collect::<Vec<_>>();
    let reference_draw_surface = reference_rows
        .iter()
        .map(|row| {
            (
                row.street,
                row.draw_category,
                row.missed_flush_draw,
                row.missed_straight_draw,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        production_draw_surface.len(),
        reference_draw_surface.len(),
        "production and reference should materialize the same number of rows"
    );
    assert_eq!(production_draw_surface, reference_draw_surface);
}

struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        (self.state >> 32) as u32
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for index in (1..values.len()).rev() {
            let swap_index = (self.next_u32() as usize) % (index + 1);
            values.swap(index, swap_index);
        }
    }
}

fn sample_unique_cards(rng: &mut DeterministicRng, count: usize) -> Vec<String> {
    let mut deck = full_deck_strings();
    rng.shuffle(&mut deck);
    deck.into_iter().take(count).collect()
}

fn full_deck_strings() -> Vec<String> {
    let mut deck = Vec::with_capacity(52);
    for rank in [
        "2", "3", "4", "5", "6", "7", "8", "9", "T", "J", "Q", "K", "A",
    ] {
        for suit in ["c", "d", "h", "s"] {
            deck.push(format!("{rank}{suit}"));
        }
    }
    deck
}

fn build_street_strength_golden_snapshot() -> StreetStrengthGoldenSnapshot {
    let cases = [
        (
            "false_positive_flush_turn",
            parse_canonical_hand(&showdown_hand(
                "BRSTRGOLD001",
                ["Jh", "Th", "2h"],
                Some("3h"),
                Some("As"),
                "Hero",
                "[8h 7d]",
                "Villain",
                "[Ac Ad]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [Ac Ad] and lost\nSeat 2: Hero (big blind) showed [8h 7d] and collected (200)",
            ))
            .unwrap(),
        ),
        (
            "runner_runner_backdoor",
            parse_canonical_hand(&showdown_hand(
                "BRSTRGOLD002",
                ["Qc", "7h", "2d"],
                Some("4s"),
                Some("3h"),
                "Hero",
                "[Ac Kc]",
                "Villain",
                "[9c 9d]",
                "Villain collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and collected (200)\nSeat 2: Hero (big blind) showed [Ac Kc] and lost",
            ))
            .unwrap(),
        ),
        (
            "missed_flush_persists_through_two_pair",
            parse_canonical_hand(&showdown_hand(
                "BRSTRGOLD003",
                ["Kd", "7h", "2h"],
                Some("9c"),
                Some("Kc"),
                "Hero",
                "[Ah 9h]",
                "Villain",
                "[Qc Qd]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [Qc Qd] and lost\nSeat 2: Hero (big blind) showed [Ah 9h] and collected (200)",
            ))
            .unwrap(),
        ),
        (
            "ordinary_combo_draw",
            parse_canonical_hand(&showdown_hand(
                "BRSTRGOLD004",
                ["Qh", "9h", "2c"],
                Some("4d"),
                Some("3s"),
                "Hero",
                "[Jh Th]",
                "Villain",
                "[Ac Ad]",
                "Villain collected 200 from pot",
                "Seat 1: Villain (small blind) showed [Ac Ad] and collected (200)\nSeat 2: Hero (big blind) showed [Jh Th] and lost",
            ))
            .unwrap(),
        ),
    ]
    .into_iter()
    .map(|(case_id, hand)| {
        let hero_cards = hand
            .hero_hole_cards
            .clone()
            .expect("golden case must have hero hole cards");
        let board = if !hand.board_final.is_empty() {
            hand.board_final.clone()
        } else {
            hand.summary_board.clone()
        };
        let rows = evaluate_street_hand_strength(&hand)
            .expect("golden case must evaluate")
            .into_iter()
            .filter(|row| row.seat_no == 2)
            .map(|row| GoldenDrawRow {
                street: street_label(row.street).to_string(),
                draw_category: row.draw_category.as_str().to_string(),
                missed_flush_draw: row.missed_flush_draw,
                missed_straight_draw: row.missed_straight_draw,
            })
            .collect::<Vec<_>>();

        (
            case_id.to_string(),
            GoldenCaseSnapshot {
                hero_cards,
                board,
                rows,
            },
        )
    })
    .collect::<BTreeMap<_, _>>();

    StreetStrengthGoldenSnapshot { cases }
}

fn street_strength_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
        .join("street_strength_reference_states.json")
}

fn street_label(street: tracker_parser_core::models::Street) -> &'static str {
    match street {
        tracker_parser_core::models::Street::Preflop => "preflop",
        tracker_parser_core::models::Street::Flop => "flop",
        tracker_parser_core::models::Street::Turn => "turn",
        tracker_parser_core::models::Street::River => "river",
        tracker_parser_core::models::Street::Showdown => "showdown",
        tracker_parser_core::models::Street::Summary => "summary",
    }
}

#[allow(clippy::too_many_arguments)]
fn showdown_hand(
    hand_id: &str,
    flop: [&str; 3],
    turn: Option<&str>,
    river: Option<&str>,
    hero_name: &str,
    hero_cards: &str,
    villain_name: &str,
    villain_cards: &str,
    collect_line: &str,
    summary_showdown_lines: &str,
) -> String {
    let turn_line = turn
        .map(|card| {
            format!(
                "*** TURN *** [{} {} {}] [{}]\n{villain_name}: checks\n{hero_name}: checks\n",
                flop[0], flop[1], flop[2], card
            )
        })
        .unwrap_or_default();
    let board_after_turn = turn
        .map(|card| format!("{} {} {} {}", flop[0], flop[1], flop[2], card))
        .unwrap_or_else(|| format!("{} {} {}", flop[0], flop[1], flop[2]));
    let river_line = river
        .map(|card| format!("*** RIVER *** [{board_after_turn}] [{card}]\n"))
        .unwrap_or_default();
    let final_board = match (turn, river) {
        (Some(turn_card), Some(river_card)) => {
            format!(
                "{} {} {} {} {}",
                flop[0], flop[1], flop[2], turn_card, river_card
            )
        }
        (Some(turn_card), None) => format!("{} {} {} {}", flop[0], flop[1], flop[2], turn_card),
        (None, None) => format!("{} {} {}", flop[0], flop[1], flop[2]),
        (None, Some(_)) => unreachable!("river without turn is invalid for GG hands"),
    };

    format!(
        "Poker Hand #{hand_id}: Tournament #999001, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:00:00\n\
Table '1' 9-max Seat #1 is the button\n\
Seat 1: {villain_name} (1,000 in chips)\n\
Seat 2: {hero_name} (1,000 in chips)\n\
{villain_name}: posts small blind 50\n\
{hero_name}: posts big blind 100\n\
*** HOLE CARDS ***\n\
Dealt to {hero_name} {hero_cards}\n\
{villain_name}: calls 50\n\
{hero_name}: checks\n\
*** FLOP *** [{flop0} {flop1} {flop2}]\n\
{villain_name}: checks\n\
{hero_name}: checks\n\
{turn_line}\
{river_line}\
*** SHOWDOWN ***\n\
{villain_name}: shows {villain_cards}\n\
{hero_name}: shows {hero_cards}\n\
{collect_line}\n\
*** SUMMARY ***\n\
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0\n\
Board [{final_board}]\n\
{summary_showdown_lines}",
        flop0 = flop[0],
        flop1 = flop[1],
        flop2 = flop[2],
    )
}
