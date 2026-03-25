use std::{collections::BTreeMap, fs, path::PathBuf};

use tracker_parser_core::{
    models::{CanonicalParsedHand, CertaintyState},
    normalizer::normalize_hand,
    parsers::hand_history::{parse_canonical_hand, split_hand_history},
};

const EDGE_MATRIX_FIXTURE: &str = "GG20260325-phase0-exact-core-edge-matrix.txt";

#[test]
fn parses_phase0_exact_core_edge_matrix_with_only_reason_coded_explicit_warnings() {
    let mut unexpected = Vec::new();
    let mut saw_partial_reveal = false;
    let mut saw_no_show = false;
    let hands = split_hand_history(&read_edge_fixture()).unwrap();

    assert_eq!(hands.len(), 10);

    for hand in hands {
        let parsed = parse_canonical_hand(&hand.raw_text).unwrap_or_else(|error| {
            panic!(
                "edge-matrix hand `{}` failed to parse: {error}",
                hand.header.hand_id
            )
        });

        for warning in &parsed.parse_warnings {
            if !is_allowed_edge_warning(warning) {
                unexpected.push(format!("{} :: {warning}", parsed.header.hand_id));
            }
        }

        if parsed.header.hand_id == "BRCM0402" {
            saw_partial_reveal = parsed
                .parse_warnings
                .iter()
                .any(|warning| warning.starts_with("partial_reveal_show_line: "));
            saw_no_show = parsed
                .parse_warnings
                .iter()
                .any(|warning| warning.starts_with("unsupported_no_show_line: "));
        }
    }

    assert!(
        saw_partial_reveal,
        "expected explicit partial-reveal warning"
    );
    assert!(saw_no_show, "expected explicit no-show warning");
    assert!(
        unexpected.is_empty(),
        "unexpected parser warnings across exact-core edge matrix:\n{}",
        unexpected.join("\n")
    );
}

#[test]
fn normalizes_phase0_exact_core_edge_matrix_with_reason_coded_contracts() {
    let normalized = normalized_edge_matrix();

    let ante_exhausted = normalized.get("BRCM0403").unwrap();
    assert!(ante_exhausted.invariants.invariant_errors.is_empty());
    assert!(ante_exhausted.invariants.uncertain_reason_codes.is_empty());

    let hu_preflop_illegal = normalized.get("BRLEGAL2").unwrap();
    assert!(
        hu_preflop_illegal
            .invariants
            .invariant_errors
            .iter()
            .any(|issue| issue.starts_with("illegal_actor_order:"))
    );

    let hu_postflop_illegal = normalized.get("BRLEGAL3").unwrap();
    assert!(
        hu_postflop_illegal
            .invariants
            .invariant_errors
            .iter()
            .any(|issue| issue.starts_with("illegal_actor_order:"))
    );

    let short_all_in_non_reopen = normalized.get("BRLEGAL4").unwrap();
    assert!(
        short_all_in_non_reopen
            .invariants
            .invariant_errors
            .iter()
            .any(|issue| issue.starts_with("action_not_reopened_after_short_all_in:"))
    );

    let sidepot_ko = normalized.get("BRSIDE1").unwrap();
    let medium = sidepot_ko
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "Medium")
        .unwrap();
    assert_eq!(medium.resolved_by_pot_nos, vec![1, 2, 3]);
    assert_eq!(medium.certainty_state, CertaintyState::Exact);
    assert!(medium.is_sidepot_based);

    let hidden_showdown = normalized.get("BRCM0502").unwrap();
    assert!(hidden_showdown.pot_winners.is_empty());
    assert!(
        hidden_showdown
            .invariants
            .uncertain_reason_codes
            .iter()
            .any(|issue| issue.starts_with("pot_settlement_ambiguous_hidden_showdown:"))
    );

    let odd_chip = normalized.get("BRCM0503").unwrap();
    assert!(odd_chip.invariants.invariant_errors.is_empty());
    assert_eq!(odd_chip.pot_winners.len(), 4);
    assert_eq!(
        odd_chip
            .pot_winners
            .iter()
            .map(|winner| winner.share_amount)
            .sum::<i64>(),
        401
    );

    let joint_ko = normalized.get("BRCM0601").unwrap();
    let medium = joint_ko
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "Medium")
        .unwrap();
    assert_eq!(medium.resolved_by_pot_nos, vec![1, 2]);
    assert_eq!(
        medium.ko_involved_winners,
        vec!["Shorty".to_string(), "Hero".to_string()]
    );
    assert_eq!(medium.hero_share_fraction, Some(0.4));
    assert_eq!(medium.hero_ko_share_total, Some(0.4));
    assert!(medium.joint_ko);
    assert_eq!(medium.certainty_state, CertaintyState::Exact);
}

fn normalized_edge_matrix() -> BTreeMap<String, tracker_parser_core::models::NormalizedHand> {
    parsed_edge_matrix()
        .into_iter()
        .map(|(hand_id, hand)| {
            let normalized = normalize_hand(&hand).unwrap_or_else(|error| {
                panic!("edge-matrix hand `{hand_id}` failed to normalize: {error}")
            });
            (hand_id, normalized)
        })
        .collect()
}

fn parsed_edge_matrix() -> BTreeMap<String, CanonicalParsedHand> {
    split_hand_history(&read_edge_fixture())
        .unwrap()
        .into_iter()
        .map(|hand| {
            let parsed = parse_canonical_hand(&hand.raw_text).unwrap_or_else(|error| {
                panic!(
                    "edge-matrix hand `{}` failed to parse: {error}",
                    hand.header.hand_id
                )
            });
            (parsed.header.hand_id.clone(), parsed)
        })
        .collect()
}

fn read_edge_fixture() -> String {
    fs::read_to_string(fixture_path(EDGE_MATRIX_FIXTURE)).unwrap()
}

fn fixture_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/mbr/hh/{filename}"))
}

fn is_allowed_edge_warning(warning: &str) -> bool {
    warning.starts_with("partial_reveal_show_line: ")
        || warning.starts_with("partial_reveal_summary_show_surface: ")
        || warning.starts_with("unsupported_no_show_line: ")
        || warning.starts_with("unparsed_summary_seat_line: ")
}
