use tracker_parser_core::{
    models::Street,
    parsers::hand_history::parse_canonical_hand,
    street_strength::{
        BestHandClass, PairStrength, STREET_HAND_STRENGTH_VERSION, evaluate_street_hand_strength,
    },
};

#[test]
fn materializes_rows_for_hero_and_showdown_known_opponent_on_all_reached_streets() {
    let hand = parse_canonical_hand(
        &showdown_hand(
            "BRSTR001",
            ["Kd", "7h", "2h"],
            Some("4c"),
            Some("3s"),
            "Hero",
            "[Ah Kh]",
            "Villain",
            "[Qc Qd]",
            "Hero collected 200 from pot",
            "Seat 1: Villain (small blind) showed [Qc Qd] and lost with a pair of Queens\nSeat 2: Hero (big blind) showed [Ah Kh] and collected (200) with a pair of Kings",
        ),
    )
    .unwrap();

    let rows = evaluate_street_hand_strength(&hand).unwrap();

    assert_eq!(rows.len(), 6);
    assert!(row(&rows, 1, Street::Flop).is_some());
    assert!(row(&rows, 1, Street::Turn).is_some());
    assert!(row(&rows, 1, Street::River).is_some());
    assert!(row(&rows, 2, Street::Flop).is_some());
    assert!(row(&rows, 2, Street::Turn).is_some());
    assert!(row(&rows, 2, Street::River).is_some());

    for descriptor in rows {
        assert_eq!(descriptor.descriptor_version, STREET_HAND_STRENGTH_VERSION);
        assert_eq!(descriptor.certainty_state.as_str(), "exact");
    }
}

#[test]
fn skips_partial_showdown_reveals_instead_of_failing_the_import() {
    let hand = parse_canonical_hand(
        r#"Poker Hand #BRSTRPARTIAL: Tournament #999001, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:00:00
Table '1' 9-max Seat #1 is the button
Seat 1: Villain (1,000 in chips)
Seat 2: Hero (1,000 in chips)
Villain: posts small blind 50
Hero: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Kh]
Villain: calls 50
Hero: checks
*** FLOP *** [Kd 7h 2h]
Villain: checks
Hero: checks
*** TURN *** [Kd 7h 2h] [4c]
Villain: checks
Hero: checks
*** RIVER *** [Kd 7h 2h 4c] [3s]
*** SHOWDOWN ***
Villain: shows [5d]
Hero: shows [Ah Kh]
Hero collected 200 from pot
*** SUMMARY ***
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [Kd 7h 2h 4c 3s]
Seat 1: Villain (small blind) showed [5d] and lost with a pair of Fives
Seat 2: Hero (big blind) showed [Ah Kh] and collected (200) with a pair of Kings"#,
    )
    .unwrap();

    let rows = evaluate_street_hand_strength(&hand).unwrap();

    assert_eq!(rows.len(), 3);
    assert!(row(&rows, 1, Street::Flop).is_none());
    assert!(row(&rows, 2, Street::Flop).is_some());
    assert!(row(&rows, 2, Street::Turn).is_some());
    assert!(row(&rows, 2, Street::River).is_some());
}

#[test]
fn classifies_top_pair_ace_kicker_flush_draw_pair_plus_draw_and_river_miss() {
    let hand = parse_canonical_hand(
        &showdown_hand(
            "BRSTR002",
            ["Kd", "7h", "2h"],
            Some("4c"),
            Some("3s"),
            "Hero",
            "[Ah Kh]",
            "Villain",
            "[Qc Qd]",
            "Hero collected 200 from pot",
            "Seat 1: Villain (small blind) showed [Qc Qd] and lost with a pair of Queens\nSeat 2: Hero (big blind) showed [Ah Kh] and collected (200) with a pair of Kings",
        ),
    )
    .unwrap();

    let rows = evaluate_street_hand_strength(&hand).unwrap();
    let flop = row(&rows, 2, Street::Flop).unwrap();
    let river = row(&rows, 2, Street::River).unwrap();

    assert_eq!(flop.best_hand_class, BestHandClass::Pair);
    assert_eq!(flop.pair_strength, PairStrength::TopPairAceKicker);
    assert!(flop.has_flush_draw);
    assert!(!flop.has_backdoor_flush_draw);
    assert!(flop.has_pair_plus_draw);
    assert!(!flop.has_overcards);
    assert!(!flop.has_air);

    assert_eq!(river.best_hand_class, BestHandClass::Pair);
    assert_eq!(river.pair_strength, PairStrength::TopPairAceKicker);
    assert!(river.has_missed_draw_by_river);
    assert!(!river.has_flush_draw);
}

#[test]
fn separates_overcards_from_air() {
    let overcards_hand = parse_canonical_hand(
        &showdown_hand(
            "BRSTR003",
            ["Qd", "7s", "2h"],
            Some("4c"),
            Some("3d"),
            "Hero",
            "[Ah Kc]",
            "Villain",
            "[9c 9d]",
            "Villain collected 200 from pot",
            "Seat 1: Villain (small blind) showed [9c 9d] and collected (200) with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah Kc] and lost with Ace high",
        ),
    )
    .unwrap();
    let air_hand = parse_canonical_hand(
        &showdown_hand(
            "BRSTR004",
            ["Qs", "7h", "2c"],
            Some("4d"),
            Some("3s"),
            "Hero",
            "[8c 4h]",
            "Villain",
            "[9c 9d]",
            "Villain collected 200 from pot",
            "Seat 1: Villain (small blind) showed [9c 9d] and collected (200) with a pair of Nines\nSeat 2: Hero (big blind) showed [8c 4h] and lost with Queen high",
        ),
    )
    .unwrap();

    let overcard_rows = evaluate_street_hand_strength(&overcards_hand).unwrap();
    let air_rows = evaluate_street_hand_strength(&air_hand).unwrap();

    let overcard_flop = row(&overcard_rows, 2, Street::Flop).unwrap();
    let air_flop = row(&air_rows, 2, Street::Flop).unwrap();

    assert_eq!(overcard_flop.best_hand_class, BestHandClass::HighCard);
    assert!(overcard_flop.has_overcards);
    assert!(!overcard_flop.has_air);

    assert_eq!(air_flop.best_hand_class, BestHandClass::HighCard);
    assert!(!air_flop.has_overcards);
    assert!(air_flop.has_air);
}

#[test]
fn detects_open_ended_gutshot_and_double_gutshot_draws() {
    let open_ended_hand = parse_canonical_hand(
        &showdown_hand(
            "BRSTR005",
            ["9c", "Tc", "2s"],
            Some("4d"),
            Some("Ah"),
            "Hero",
            "[8h 7d]",
            "Villain",
            "[Ac Ad]",
            "Villain collected 200 from pot",
            "Seat 1: Villain (small blind) showed [Ac Ad] and collected (200) with a pair of Aces\nSeat 2: Hero (big blind) showed [8h 7d] and lost with Ten high",
        ),
    )
    .unwrap();
    let gutshot_hand = parse_canonical_hand(
        &showdown_hand(
            "BRSTR006",
            ["9c", "Jc", "2s"],
            Some("4d"),
            Some("Ah"),
            "Hero",
            "[8h 7d]",
            "Villain",
            "[Ac Ad]",
            "Villain collected 200 from pot",
            "Seat 1: Villain (small blind) showed [Ac Ad] and collected (200) with a pair of Aces\nSeat 2: Hero (big blind) showed [8h 7d] and lost with Jack high",
        ),
    )
    .unwrap();
    let double_gutshot_hand = parse_canonical_hand(
        &showdown_hand(
            "BRSTR007",
            ["Ks", "9h", "7c"],
            Some("2d"),
            Some("3s"),
            "Hero",
            "[Jc Td]",
            "Villain",
            "[Ac Ad]",
            "Villain collected 200 from pot",
            "Seat 1: Villain (small blind) showed [Ac Ad] and collected (200) with a pair of Aces\nSeat 2: Hero (big blind) showed [Jc Td] and lost with King high",
        ),
    )
    .unwrap();

    let open_ended_rows = evaluate_street_hand_strength(&open_ended_hand).unwrap();
    let gutshot_rows = evaluate_street_hand_strength(&gutshot_hand).unwrap();
    let double_gutshot_rows = evaluate_street_hand_strength(&double_gutshot_hand).unwrap();

    let open_ended_flop = row(&open_ended_rows, 2, Street::Flop).unwrap();
    let gutshot_flop = row(&gutshot_rows, 2, Street::Flop).unwrap();
    let double_gutshot_flop = row(&double_gutshot_rows, 2, Street::Flop).unwrap();

    assert!(open_ended_flop.has_open_ended);
    assert!(!open_ended_flop.has_gutshot);
    assert!(!open_ended_flop.has_double_gutshot);

    assert!(!gutshot_flop.has_open_ended);
    assert!(gutshot_flop.has_gutshot);
    assert!(!gutshot_flop.has_double_gutshot);

    assert!(!double_gutshot_flop.has_open_ended);
    assert!(!double_gutshot_flop.has_gutshot);
    assert!(double_gutshot_flop.has_double_gutshot);
}

#[test]
fn detects_backdoor_flush_draw_on_flop_only() {
    let hand = parse_canonical_hand(
        &showdown_hand(
            "BRSTR017",
            ["Qh", "7h", "2c"],
            Some("4d"),
            Some("3s"),
            "Hero",
            "[Ah Kc]",
            "Villain",
            "[9c 9d]",
            "Villain collected 200 from pot",
            "Seat 1: Villain (small blind) showed [9c 9d] and collected (200) with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah Kc] and lost with Ace high",
        ),
    )
    .unwrap();

    let rows = evaluate_street_hand_strength(&hand).unwrap();
    let flop = row(&rows, 2, Street::Flop).unwrap();
    let turn = row(&rows, 2, Street::Turn).unwrap();

    assert!(!flop.has_flush_draw);
    assert!(flop.has_backdoor_flush_draw);
    assert!(!turn.has_backdoor_flush_draw);
}

#[test]
fn classifies_remaining_pair_strength_buckets() {
    let cases: Vec<(String, PairStrength)> = vec![
        (
            showdown_hand(
                "BRSTR018",
                ["Kd", "Ks", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah Qc]",
                "Villain",
                "[9c 9d]",
                "Villain collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and collected (200) with two pair, Kings and Nines\nSeat 2: Hero (big blind) showed [Ah Qc] and lost with a pair of Kings",
            ),
            PairStrength::BoardPair,
        ),
        (
            showdown_hand(
                "BRSTR019",
                ["Kd", "7s", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Qc Qh]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Qc Qh] and collected (200) with a pair of Queens",
            ),
            PairStrength::Underpair,
        ),
        (
            showdown_hand(
                "BRSTR020",
                ["Kd", "7s", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah 2c]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah 2c] and collected (200) with a pair of Twos",
            ),
            PairStrength::BottomPair,
        ),
        (
            showdown_hand(
                "BRSTR021",
                ["Kd", "7s", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah 4h]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah 4h] and collected (200) with a pair of Fours",
            ),
            PairStrength::MiddlePair,
        ),
        (
            showdown_hand(
                "BRSTR022",
                ["Kd", "7s", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Qh Kc]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Qh Kc] and collected (200) with a pair of Kings",
            ),
            PairStrength::TopPairBroadwayKicker,
        ),
        (
            showdown_hand(
                "BRSTR023",
                ["Kd", "7s", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[9h Kc]",
                "Villain",
                "[Qc Qd]",
                "Villain collected 200 from pot",
                "Seat 1: Villain (small blind) showed [Qc Qd] and collected (200) with a pair of Queens\nSeat 2: Hero (big blind) showed [9h Kc] and lost with a pair of Kings",
            ),
            PairStrength::TopPairWeakKicker,
        ),
        (
            showdown_hand(
                "BRSTR024",
                ["Kd", "7s", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah Ac]",
                "Villain",
                "[Qc Qd]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [Qc Qd] and lost with a pair of Queens\nSeat 2: Hero (big blind) showed [Ah Ac] and collected (200) with a pair of Aces",
            ),
            PairStrength::Overpair,
        ),
        (
            showdown_hand(
                "BRSTR025",
                ["Kd", "Ks", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah Kc]",
                "Villain",
                "[Qc Qd]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [Qc Qd] and lost with a pair of Queens\nSeat 2: Hero (big blind) showed [Ah Kc] and collected (200) with three of a kind, Kings",
            ),
            PairStrength::Trips,
        ),
        (
            showdown_hand(
                "BRSTR026",
                ["Kd", "7s", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Kc Kh]",
                "Villain",
                "[Qc Qd]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [Qc Qd] and lost with a pair of Queens\nSeat 2: Hero (big blind) showed [Kc Kh] and collected (200) with three of a kind, Kings",
            ),
            PairStrength::Set,
        ),
    ];

    for (raw_hand, expected_strength) in cases {
        let hand = parse_canonical_hand(&raw_hand).unwrap();
        let rows = evaluate_street_hand_strength(&hand).unwrap();
        let river = row(&rows, 2, Street::River).unwrap();
        assert_eq!(river.pair_strength, expected_strength);
    }
}

#[test]
fn recognizes_all_best_hand_classes_on_river() {
    let cases: Vec<(String, BestHandClass)> = vec![
        (
            showdown_hand(
                "BRSTR008",
                ["Qd", "7s", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah Kc]",
                "Villain",
                "[9c 9d]",
                "Villain collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and collected (200) with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah Kc] and lost with Ace high",
            ),
            BestHandClass::HighCard,
        ),
        (
            showdown_hand(
                "BRSTR009",
                ["Kd", "7s", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah Kc]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah Kc] and collected (200) with a pair of Kings",
            ),
            BestHandClass::Pair,
        ),
        (
            showdown_hand(
                "BRSTR010",
                ["Kd", "As", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah Kc]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah Kc] and collected (200) with two pair, Aces and Kings",
            ),
            BestHandClass::TwoPair,
        ),
        (
            showdown_hand(
                "BRSTR011",
                ["Kd", "Ks", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah Kc]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah Kc] and collected (200) with three of a kind, Kings",
            ),
            BestHandClass::Trips,
        ),
        (
            showdown_hand(
                "BRSTR012",
                ["Qd", "Js", "Th"],
                Some("2c"),
                Some("3d"),
                "Hero",
                "[Ah Kc]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah Kc] and collected (200) with a straight, Ace high",
            ),
            BestHandClass::Straight,
        ),
        (
            showdown_hand(
                "BRSTR013",
                ["Kh", "7h", "2h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah 9h]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah 9h] and collected (200) with a flush, Ace high",
            ),
            BestHandClass::Flush,
        ),
        (
            showdown_hand(
                "BRSTR014",
                ["Ad", "7s", "7h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah Ac]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah Ac] and collected (200) with a full house, Aces full of Sevens",
            ),
            BestHandClass::FullHouse,
        ),
        (
            showdown_hand(
                "BRSTR015",
                ["Ad", "As", "7h"],
                Some("4c"),
                Some("3d"),
                "Hero",
                "[Ah Ac]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah Ac] and collected (200) with four of a kind, Aces",
            ),
            BestHandClass::Quads,
        ),
        (
            showdown_hand(
                "BRSTR016",
                ["Qh", "Jh", "Th"],
                Some("2c"),
                Some("3d"),
                "Hero",
                "[Ah Kh]",
                "Villain",
                "[9c 9d]",
                "Hero collected 200 from pot",
                "Seat 1: Villain (small blind) showed [9c 9d] and lost with a pair of Nines\nSeat 2: Hero (big blind) showed [Ah Kh] and collected (200) with a straight flush, Ace high",
            ),
            BestHandClass::StraightFlush,
        ),
    ];

    for (raw_hand, expected_class) in cases {
        let hand = parse_canonical_hand(&raw_hand).unwrap();
        let rows = evaluate_street_hand_strength(&hand).unwrap();
        let river = row(&rows, 2, Street::River).unwrap();
        assert_eq!(river.best_hand_class, expected_class);
    }
}

fn row(
    rows: &[tracker_parser_core::street_strength::StreetHandStrength],
    seat_no: u8,
    street: Street,
) -> Option<&tracker_parser_core::street_strength::StreetHandStrength> {
    rows.iter()
        .find(|descriptor| descriptor.seat_no == seat_no && descriptor.street == street)
}

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
        .map(|card| format!("*** TURN *** [{} {} {}] [{}]\n{villain_name}: checks\n{hero_name}: checks\n", flop[0], flop[1], flop[2], card))
        .unwrap_or_default();
    let board_after_turn = turn
        .map(|card| format!("{} {} {} {}", flop[0], flop[1], flop[2], card))
        .unwrap_or_else(|| format!("{} {} {}", flop[0], flop[1], flop[2]));
    let river_line = river
        .map(|card| format!("*** RIVER *** [{board_after_turn}] [{card}]\n"))
        .unwrap_or_default();
    let final_board = match (turn, river) {
        (Some(turn_card), Some(river_card)) => {
            format!("{} {} {} {} {}", flop[0], flop[1], flop[2], turn_card, river_card)
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
