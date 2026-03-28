use std::{collections::BTreeMap, fs, path::PathBuf};

use serde_json::to_value;
use tracker_parser_core::{
    models::{CertaintyState, NormalizedHand},
    normalizer::normalize_hand,
    parsers::hand_history::{parse_canonical_hand, split_hand_history},
};

const MONEY_STATE_FIXTURE: &str = "GG20260328-p0-money-state-safety.txt";

#[derive(Debug)]
struct SafetyContract {
    invariant_codes: &'static [&'static str],
    settlement_codes: &'static [&'static str],
}

#[test]
fn malformed_money_surface_enters_fail_safe_without_negative_outputs() {
    let normalized = normalized_hands();
    let expected = expected_contracts();

    assert_eq!(normalized.len(), expected.len());

    for (hand_id, contract) in expected {
        let hand = normalized
            .get(hand_id)
            .unwrap_or_else(|| panic!("missing normalized malformed hand `{hand_id}`"));

        assert_eq!(
            relevant_invariant_codes(hand),
            materialize(contract.invariant_codes),
            "relevant invariant codes mismatch for `{hand_id}`"
        );
        assert_eq!(
            relevant_settlement_codes(hand),
            materialize(contract.settlement_codes),
            "relevant settlement codes mismatch for `{hand_id}`"
        );
        assert_eq!(
            hand.settlement.certainty_state,
            CertaintyState::Inconsistent,
            "hand `{hand_id}` should stop claiming exact settlement"
        );
        assert!(
            hand.settlement
                .pots
                .iter()
                .all(|pot| pot.selected_allocation.is_none()),
            "hand `{hand_id}` should not materialize exact allocations after fail-safe"
        );
        assert!(
            hand.actual
                .committed_total_by_player
                .values()
                .all(|amount| *amount >= 0),
            "hand `{hand_id}` produced negative committed totals: {:?}",
            hand.actual.committed_total_by_player
        );
        assert!(
            hand.actual
                .stacks_after_observed
                .values()
                .all(|amount| *amount >= 0),
            "hand `{hand_id}` produced negative final stacks: {:?}",
            hand.actual.stacks_after_observed
        );
        assert!(
            hand.returns.iter().all(|entry| entry.amount >= 0),
            "hand `{hand_id}` produced negative return rows: {:?}",
            hand.returns
        );
    }
}

fn expected_contracts() -> BTreeMap<&'static str, SafetyContract> {
    [
        (
            "P0MS001",
            SafetyContract {
                invariant_codes: &["action_amount_exceeds_stack"],
                settlement_codes: &["replay_state_invalid"],
            },
        ),
        (
            "P0MS002",
            SafetyContract {
                invariant_codes: &["action_amount_exceeds_stack"],
                settlement_codes: &["replay_state_invalid"],
            },
        ),
        (
            "P0MS003",
            SafetyContract {
                invariant_codes: &["action_amount_exceeds_stack"],
                settlement_codes: &["replay_state_invalid"],
            },
        ),
        (
            "P0MS004",
            SafetyContract {
                invariant_codes: &["uncalled_return_amount_mismatch"],
                settlement_codes: &["replay_state_invalid"],
            },
        ),
        (
            "P0MS005",
            SafetyContract {
                invariant_codes: &[
                    "refund_exceeds_committed",
                    "refund_exceeds_betting_round_contrib",
                    "uncalled_return_amount_mismatch",
                ],
                settlement_codes: &["replay_state_invalid"],
            },
        ),
        (
            "P0MS006",
            SafetyContract {
                invariant_codes: &[
                    "refund_exceeds_betting_round_contrib",
                    "uncalled_return_amount_mismatch",
                ],
                settlement_codes: &["replay_state_invalid"],
            },
        ),
    ]
    .into_iter()
    .collect()
}

fn normalized_hands() -> BTreeMap<String, NormalizedHand> {
    split_hand_history(&read_fixture())
        .unwrap()
        .into_iter()
        .map(|hand| {
            let parsed = parse_canonical_hand(&hand.raw_text).unwrap_or_else(|error| {
                panic!(
                    "malformed fixture hand `{}` failed to parse: {error}",
                    hand.header.hand_id
                )
            });
            let normalized = normalize_hand(&parsed).unwrap_or_else(|error| {
                panic!(
                    "malformed fixture hand `{}` failed to normalize: {error}",
                    parsed.header.hand_id
                )
            });
            (parsed.header.hand_id.clone(), normalized)
        })
        .collect()
}

fn relevant_invariant_codes(hand: &NormalizedHand) -> Vec<String> {
    let mut codes = hand
        .invariants
        .issues
        .iter()
        .map(code_from_json)
        .filter(|code| {
            matches!(
                code.as_str(),
                "action_amount_exceeds_stack"
                    | "refund_exceeds_committed"
                    | "refund_exceeds_betting_round_contrib"
                    | "uncalled_return_amount_mismatch"
            )
        })
        .collect::<Vec<_>>();
    codes.sort();
    codes
}

fn relevant_settlement_codes(hand: &NormalizedHand) -> Vec<String> {
    let mut codes = hand
        .settlement
        .issues
        .iter()
        .map(code_from_json)
        .filter(|code| code == "replay_state_invalid")
        .collect::<Vec<_>>();
    codes.sort();
    codes
}

fn code_from_json<T: serde::Serialize>(value: &T) -> String {
    to_value(value)
        .unwrap()
        .get("code")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown")
        .to_string()
}

fn materialize(codes: &[&'static str]) -> Vec<String> {
    let mut values = codes
        .iter()
        .map(|code| (*code).to_string())
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn read_fixture() -> String {
    fs::read_to_string(fixture_path(MONEY_STATE_FIXTURE)).unwrap()
}

fn fixture_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../fixtures/mbr/hh_synthetic/{filename}"))
}
