use tracker_parser_core::{
    models::{PlayerStatus, Street},
    normalizer::normalize_hand,
    parsers::hand_history::parse_canonical_hand,
};

const HH_FT: &str =
    include_str!("../../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");

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
    assert_eq!(normalized.eliminations.len(), 1);
    assert_eq!(normalized.eliminations[0].eliminated_seat_no, 3);
    assert_eq!(normalized.eliminations[0].eliminated_player_name, "f02e54a6");
    assert_eq!(normalized.eliminations[0].resolved_by_pot_no, None);
    assert_eq!(normalized.eliminations[0].ko_involved_winner_count, 1);
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
    assert!(normalized.eliminations.is_empty());
    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}
