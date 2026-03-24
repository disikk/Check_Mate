use tracker_parser_core::{
    models::{CertaintyState, PlayerStatus, Street},
    normalizer::normalize_hand,
    parsers::hand_history::parse_canonical_hand,
};

const HH_FT: &str =
    include_str!("../../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");
const SPLIT_KO_HAND: &str = r#"Poker Hand #BRSPLIT1: Tournament #999001, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:00:00
Table '1' 9-max Seat #1 is the button
Seat 1: VillainA (1,000 in chips)
Seat 2: Hero (1,000 in chips)
Seat 3: VillainB (1,000 in chips)
VillainA: posts small blind 50
Hero: posts big blind 100
*** HOLE CARDS ***
Dealt to VillainA
Dealt to Hero [Ah Ad]
Dealt to VillainB
VillainB: calls 100
VillainA: calls 50
Hero: checks
*** FLOP *** [2c 2d 7h]
VillainA: bets 900 and is all-in
Hero: calls 900 and is all-in
VillainB: calls 900 and is all-in
*** TURN *** [2c 2d 7h] [9s]
*** RIVER *** [2c 2d 7h 9s] [Kc]
*** SHOWDOWN ***
VillainA: shows [Qs Qd]
Hero: shows [Ah Ad]
VillainB: shows [Kc Kh]
Hero collected 1,500 from pot
VillainB collected 1,500 from pot
*** SUMMARY ***
Total pot 3,000 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 2d 7h 9s Kc]
Seat 1: VillainA (small blind) showed [Qs Qd] and lost
Seat 2: Hero (big blind) showed [Ah Ad] and collected (1,500)
Seat 3: VillainB showed [Kc Kh] and collected (1,500)"#;
const SIDEPOT_KO_HAND: &str = r#"Poker Hand #BRSIDE1: Tournament #999002, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:05:00
Table '2' 9-max Seat #1 is the button
Seat 1: Shorty (500 in chips)
Seat 2: Hero (1,000 in chips)
Seat 3: Medium (1,000 in chips)
Seat 4: BigStack (1,500 in chips)
Shorty: posts small blind 50
Hero: posts big blind 100
*** HOLE CARDS ***
Dealt to Shorty
Dealt to Hero [Ac Qc]
Dealt to Medium
Dealt to BigStack
Medium: calls 100
BigStack: raises 400 to 500
Shorty: calls 450 and is all-in
Hero: folds
Medium: raises 500 to 1,000 and is all-in
BigStack: calls 500
*** FLOP *** [2h 7d Tc]
*** TURN *** [2h 7d Tc] [Js]
*** RIVER *** [2h 7d Tc Js] [Kd]
*** SHOWDOWN ***
Medium: shows [Jh Jc]
BigStack: shows [As Ad]
BigStack collected 400 from pot
BigStack collected 1,200 from pot
BigStack collected 1,000 from pot
*** SUMMARY ***
Total pot 2,600 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2h 7d Tc Js Kd]
Seat 1: Shorty (small blind) lost
Seat 2: Hero (big blind) folded before Flop
Seat 3: Medium showed [Jh Jc] and lost
Seat 4: BigStack showed [As Ad] and collected (2,600)"#;

#[test]
fn captures_terminal_all_in_snapshot_with_exact_pot_and_stacks() {
    let first_hand = HH_FT.split("\n\n").next().unwrap();
    let hand = parse_canonical_hand(first_hand).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let snapshot = normalized.snapshot.as_ref().expect("snapshot must exist");
    assert_eq!(snapshot.snapshot_street, Street::Preflop);
    assert_eq!(snapshot.snapshot_event_seq, 5);
    assert_eq!(snapshot.known_board_cards.len(), 0);
    assert_eq!(snapshot.future_board_cards_count, 5);
    assert_eq!(snapshot.pots.len(), 1);
    assert_eq!(snapshot.pots[0].amount, 3_984);
    assert_eq!(
        snapshot.pots[0].eligible_players,
        vec!["f02e54a6".to_string(), "Hero".to_string()]
    );

    let hero = snapshot
        .players
        .iter()
        .find(|player| player.player_name == "Hero")
        .unwrap();
    let villain = snapshot
        .players
        .iter()
        .find(|player| player.player_name == "f02e54a6")
        .unwrap();

    assert_eq!(hero.status, PlayerStatus::Live);
    assert_eq!(hero.stack_at_snapshot, 14_016);
    assert_eq!(hero.committed_total, 1_992);
    assert_eq!(villain.status, PlayerStatus::AllIn);
    assert_eq!(villain.stack_at_snapshot, 0);
    assert_eq!(villain.committed_total, 1_992);

    assert_eq!(
        normalized.actual.stacks_after_actual.get("Hero"),
        Some(&18_000)
    );
    assert_eq!(
        normalized.actual.stacks_after_actual.get("f02e54a6"),
        Some(&0)
    );
    assert_eq!(
        normalized.actual.winner_collections.get("Hero"),
        Some(&3_984)
    );
    assert_eq!(
        normalized.actual.committed_total_by_player.get("Hero"),
        Some(&1_992)
    );
    assert_eq!(
        normalized.actual.committed_total_by_player.get("f02e54a6"),
        Some(&1_992)
    );
    assert_eq!(normalized.actual.rake_amount, 0);
    assert_eq!(normalized.final_pots.len(), 1);
    assert_eq!(normalized.final_pots[0].pot_no, 1);
    assert!(normalized.final_pots[0].is_main);
    assert_eq!(normalized.final_pots[0].amount, 3_984);
    assert!(normalized.returns.is_empty());
    assert_eq!(normalized.pot_contributions.len(), 2);
    assert_eq!(normalized.pot_winners.len(), 1);
    assert_eq!(normalized.pot_winners[0].pot_no, 1);
    assert_eq!(normalized.pot_winners[0].seat_no, 7);
    assert_eq!(normalized.pot_winners[0].player_name, "Hero");
    assert_eq!(normalized.pot_winners[0].share_amount, 3_984);
    assert_eq!(normalized.eliminations.len(), 1);
    assert_eq!(normalized.eliminations[0].eliminated_seat_no, 3);
    assert_eq!(normalized.eliminations[0].eliminated_player_name, "f02e54a6");
    assert_eq!(normalized.eliminations[0].resolved_by_pot_no, Some(1));
    assert_eq!(normalized.eliminations[0].ko_involved_winner_count, 1);
    assert!(normalized.eliminations[0].hero_involved);
    assert_eq!(normalized.eliminations[0].hero_share_fraction, Some(1.0));
    assert!(!normalized.eliminations[0].is_split_ko);
    assert_eq!(normalized.eliminations[0].split_n, Some(1));
    assert!(!normalized.eliminations[0].is_sidepot_based);
    assert_eq!(normalized.eliminations[0].certainty_state, CertaintyState::Exact);
    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn handles_uncalled_return_without_creating_fake_snapshot() {
    let second_hand = HH_FT.split("\n\n").nth(1).unwrap();
    let hand = parse_canonical_hand(second_hand).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.snapshot.is_none());
    assert_eq!(
        normalized.actual.stacks_after_actual.get("Hero"),
        Some(&16_008)
    );
    assert_eq!(
        normalized.actual.stacks_after_actual.get("f02e54a6"),
        Some(&1_992)
    );
    assert_eq!(normalized.actual.winner_collections.get("Hero"), Some(&960));
    assert_eq!(
        normalized.actual.committed_total_by_player.get("Hero"),
        Some(&480)
    );
    assert_eq!(
        normalized.actual.committed_total_by_player.get("f02e54a6"),
        Some(&480)
    );
    assert_eq!(normalized.actual.rake_amount, 0);
    assert_eq!(normalized.final_pots.len(), 1);
    assert_eq!(normalized.final_pots[0].amount, 960);
    assert_eq!(normalized.returns.len(), 1);
    assert_eq!(normalized.returns[0].seat_no, 7);
    assert_eq!(normalized.returns[0].player_name, "Hero");
    assert_eq!(normalized.returns[0].amount, 15_048);
    assert_eq!(normalized.returns[0].reason, "uncalled");
    assert_eq!(normalized.pot_winners.len(), 1);
    assert_eq!(normalized.pot_winners[0].player_name, "Hero");
    assert_eq!(normalized.pot_winners[0].share_amount, 960);
    assert!(normalized.eliminations.is_empty());
    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn resolves_split_ko_with_exact_hero_share_fraction() {
    let hand = parse_canonical_hand(SPLIT_KO_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(normalized.final_pots.len(), 1);
    assert_eq!(normalized.final_pots[0].amount, 3_000);
    assert_eq!(normalized.pot_winners.len(), 2);
    assert_eq!(normalized.eliminations.len(), 1);
    assert_eq!(normalized.eliminations[0].eliminated_player_name, "VillainA");
    assert_eq!(normalized.eliminations[0].resolved_by_pot_no, Some(1));
    assert!(normalized.eliminations[0].hero_involved);
    assert_eq!(normalized.eliminations[0].hero_share_fraction, Some(0.5));
    assert!(normalized.eliminations[0].is_split_ko);
    assert_eq!(normalized.eliminations[0].split_n, Some(2));
    assert!(!normalized.eliminations[0].is_sidepot_based);
    assert_eq!(normalized.eliminations[0].certainty_state, CertaintyState::Exact);
}

#[test]
fn resolves_sidepot_ko_without_marking_hero_involved() {
    let hand = parse_canonical_hand(SIDEPOT_KO_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(normalized.final_pots.len(), 3);
    assert_eq!(normalized.final_pots[0].amount, 400);
    assert_eq!(normalized.final_pots[1].amount, 1_200);
    assert_eq!(normalized.final_pots[2].amount, 1_000);
    assert_eq!(normalized.returns.len(), 0);

    let medium = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "Medium")
        .unwrap();
    assert_eq!(medium.resolved_by_pot_no, Some(3));
    assert!(!medium.hero_involved);
    assert_eq!(medium.hero_share_fraction, Some(0.0));
    assert!(!medium.is_split_ko);
    assert_eq!(medium.split_n, Some(1));
    assert!(medium.is_sidepot_based);
    assert_eq!(medium.certainty_state, CertaintyState::Exact);
}
