use tracker_parser_core::{
    models::CertaintyState,
    parsers::hand_history::parse_canonical_hand,
    preflop_starting_hands::{canonical_starting_hand_class, evaluate_preflop_starting_hands},
};

#[test]
fn canonicalizes_matrix_classes_with_rank_order_and_suitedness() {
    assert_eq!(canonical_starting_hand_class("Ah", "Ad").unwrap(), "AA");
    assert_eq!(canonical_starting_hand_class("Kd", "Ah").unwrap(), "AKo");
    assert_eq!(canonical_starting_hand_class("Kh", "Ah").unwrap(), "AKs");
    assert_eq!(canonical_starting_hand_class("Jh", "Qc").unwrap(), "QJo");
}

#[test]
fn evaluates_preflop_rows_only_for_known_exact_hole_cards() {
    let hand = parse_canonical_hand(
        r#"Poker Hand #BRPREFLOP1: Tournament #999500, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 15:00:00
Table '7' 3-max Seat #1 is the button
Seat 1: Hero (2,000 in chips)
Seat 2: VillainKnown (2,000 in chips)
Seat 3: VillainUnknown (2,000 in chips)
Hero: posts small blind 50
VillainKnown: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Kd]
Dealt to VillainKnown
Dealt to VillainUnknown
VillainUnknown: folds
Hero: raises 200 to 300
VillainKnown: calls 200
*** FLOP *** [2c 7d 9h]
Hero: bets 200
VillainKnown: calls 200
*** TURN *** [2c 7d 9h] [Qs]
Hero: checks
VillainKnown: checks
*** RIVER *** [2c 7d 9h Qs] [3c]
Hero: checks
VillainKnown: checks
*** SHOWDOWN ***
Hero: shows [Ah Kd]
VillainKnown: shows [Qc Jh]
VillainKnown collected 1,100 from pot
*** SUMMARY ***
Total pot 1,100 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Kd] and lost
Seat 2: VillainKnown (big blind) showed [Qc Jh] and won (1,100)
Seat 3: VillainUnknown folded before Flop"#,
    )
    .unwrap();

    let rows = evaluate_preflop_starting_hands(&hand).unwrap();
    let actual = rows
        .iter()
        .map(|row| {
            (
                row.seat_no,
                row.starter_hand_class.as_str(),
                row.certainty_state,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        actual,
        vec![
            (1, "AKo", CertaintyState::Exact),
            (2, "QJo", CertaintyState::Exact),
        ]
    );
}
