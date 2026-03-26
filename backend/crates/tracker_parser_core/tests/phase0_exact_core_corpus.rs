use std::{collections::BTreeMap, fs, path::PathBuf};

use tracker_parser_core::{
    models::{
        ActionType, AllInReason, CanonicalParsedHand, CertaintyState, NormalizedHand, Street,
    },
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

    assert_eq!(hands.len(), 12);

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
fn parses_phase0_exact_core_edge_matrix_acceptance_rows() {
    let parsed = parsed_edge_matrix();

    let short_bb_forced = parsed.get("BRCM0404").unwrap();
    let short_bb_post = find_action(short_bb_forced, "ShortBb", ActionType::PostBb);
    assert_eq!(short_bb_post.amount, Some(60));
    assert!(short_bb_post.is_forced);
    assert!(short_bb_post.is_all_in);
    assert_eq!(short_bb_post.all_in_reason, Some(AllInReason::BlindExhausted));
    assert!(short_bb_post.forced_all_in_preflop);

    let dead_blind_with_ante = parsed.get("BRCM0405").unwrap();
    assert!(dead_blind_with_ante.actions.iter().any(|event| {
        event.player_name.as_deref() == Some("VillainDead")
            && event.action_type == ActionType::PostAnte
            && event.amount == Some(20)
    }));
    assert!(dead_blind_with_ante.actions.iter().any(|event| {
        event.player_name.as_deref() == Some("VillainDead")
            && event.action_type == ActionType::PostDead
            && event.amount == Some(100)
    }));
}

#[test]
fn normalizes_phase0_exact_core_edge_matrix_with_reason_coded_contracts() {
    let normalized = normalized_edge_matrix();

    let ante_exhausted = normalized.get("BRCM0403").unwrap();
    assert!(ante_exhausted.invariants.invariant_errors.is_empty());
    assert!(ante_exhausted.invariants.uncertain_reason_codes.is_empty());

    let blind_exhausted = normalized.get("BRCM0404").unwrap();
    assert!(blind_exhausted.invariants.invariant_errors.is_empty());
    assert!(blind_exhausted.invariants.uncertain_reason_codes.is_empty());
    assert_eq!(blind_exhausted.final_pots.len(), 1);
    assert_eq!(blind_exhausted.final_pots[0].amount, 120);
    assert_eq!(
        blind_exhausted
            .pot_contributions
            .iter()
            .map(|contribution| (
                contribution.pot_no,
                contribution.player_name.as_str(),
                contribution.amount,
            ))
            .collect::<Vec<_>>(),
        vec![(1, "Hero", 60), (1, "ShortBb", 60)]
    );
    assert_eq!(
        blind_exhausted
            .pot_eligibilities
            .iter()
            .map(|eligibility| (eligibility.pot_no, eligibility.player_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "Hero"), (1, "ShortBb")]
    );

    let dead_blind_with_ante = normalized.get("BRCM0405").unwrap();
    assert!(dead_blind_with_ante.invariants.invariant_errors.is_empty());
    assert!(dead_blind_with_ante.invariants.uncertain_reason_codes.is_empty());

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
    assert_eq!(
        sidepot_ko
            .pot_contributions
            .iter()
            .map(|contribution| (
                contribution.pot_no,
                contribution.player_name.as_str(),
                contribution.amount,
            ))
            .collect::<Vec<_>>(),
        vec![
            (1, "Shorty", 100),
            (1, "Hero", 100),
            (1, "Medium", 100),
            (1, "BigStack", 100),
            (2, "Shorty", 400),
            (2, "Medium", 400),
            (2, "BigStack", 400),
            (3, "Medium", 500),
            (3, "BigStack", 500),
        ]
    );
    assert_eq!(
        sidepot_ko
            .pot_eligibilities
            .iter()
            .map(|eligibility| (eligibility.pot_no, eligibility.player_name.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (1, "Shorty"),
            (1, "Medium"),
            (1, "BigStack"),
            (2, "Shorty"),
            (2, "Medium"),
            (2, "BigStack"),
            (3, "Medium"),
            (3, "BigStack"),
        ]
    );
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
    // GG rule: KO-credit по последнему side pot (pot 2), Hero единственный winner.
    assert_eq!(medium.ko_credit_pot_no, Some(2));
    assert_eq!(
        medium.ko_involved_winners,
        vec!["Hero".to_string()]
    );
    assert_eq!(medium.hero_share_fraction, Some(1.0));
    assert_eq!(medium.hero_ko_share_total, Some(1.0));
    assert!(!medium.joint_ko);
    assert_eq!(medium.certainty_state, CertaintyState::Exact);
}

#[test]
fn parses_phase0_exact_core_edge_matrix_with_explicit_manifest_contracts() {
    let parsed = parsed_edge_matrix();
    let expected = expected_parse_contracts();

    assert_eq!(parsed.len(), expected.len());

    for (hand_id, contract) in expected {
        let hand = parsed
            .get(hand_id)
            .unwrap_or_else(|| panic!("missing parsed edge hand `{hand_id}`"));

        assert_eq!(
            action_contracts(hand),
            materialize_action_contracts(&contract.actions),
            "action contract mismatch for `{hand_id}`"
        );
        assert_eq!(
            hand.parse_warnings
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            contract.warnings,
            "warning contract mismatch for `{hand_id}`"
        );
    }
}

#[test]
fn normalizes_phase0_exact_core_edge_matrix_with_explicit_manifest_contracts() {
    let normalized = normalized_edge_matrix();
    let expected = expected_normalization_contracts();

    assert_eq!(normalized.len(), expected.len());

    for (hand_id, contract) in expected {
        let hand = normalized
            .get(hand_id)
            .unwrap_or_else(|| panic!("missing normalized edge hand `{hand_id}`"));

        assert_eq!(
            committed_contract(hand),
            materialize_committed_contract(&contract.committed),
            "committed-total contract mismatch for `{hand_id}`"
        );
        assert_eq!(
            returns_contract(hand),
            materialize_returns_contract(&contract.returns),
            "returns contract mismatch for `{hand_id}`"
        );
        assert_eq!(
            final_pots_contract(hand),
            contract.final_pots,
            "final-pot contract mismatch for `{hand_id}`"
        );
        assert_eq!(
            pot_contributions_contract(hand),
            materialize_pot_contributions_contract(&contract.pot_contributions),
            "pot-contribution contract mismatch for `{hand_id}`"
        );
        assert_eq!(
            pot_eligibilities_contract(hand),
            materialize_pot_eligibilities_contract(&contract.pot_eligibilities),
            "pot-eligibility contract mismatch for `{hand_id}`"
        );
        assert_eq!(
            hand.invariants
                .invariant_errors
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            contract.invariant_errors,
            "invariant-error contract mismatch for `{hand_id}`"
        );
        assert_eq!(
            hand.invariants
                .uncertain_reason_codes
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            contract.uncertain_reason_codes,
            "uncertainty contract mismatch for `{hand_id}`"
        );
    }
}

type ActionContract = (
    usize,
    Street,
    Option<&'static str>,
    ActionType,
    bool,
    bool,
    Option<AllInReason>,
    bool,
    Option<i64>,
    Option<i64>,
);
type CommittedContract = (&'static str, i64);
type ReturnContract = (&'static str, i64, &'static str);
type FinalPotContract = (u8, i64, bool);
type PotContributionContract = (u8, &'static str, i64);
type PotEligibilityContract = (u8, &'static str);

struct EdgeParseContract {
    actions: Vec<ActionContract>,
    warnings: Vec<&'static str>,
}

struct EdgeNormalizationContract {
    committed: Vec<CommittedContract>,
    returns: Vec<ReturnContract>,
    final_pots: Vec<FinalPotContract>,
    pot_contributions: Vec<PotContributionContract>,
    pot_eligibilities: Vec<PotEligibilityContract>,
    invariant_errors: Vec<&'static str>,
    uncertain_reason_codes: Vec<&'static str>,
}

fn expected_parse_contracts() -> BTreeMap<&'static str, EdgeParseContract> {
    [
        (
            "BRCM0401",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("VillainDead"),
                        ActionType::PostDead,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("VillainMuck"),
                        ActionType::PostBb,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Fold,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        3,
                        Street::Preflop,
                        Some("VillainMuck"),
                        ActionType::Muck,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        4,
                        Street::Showdown,
                        Some("VillainDead"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(200),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
        (
            "BRCM0402",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("VillainPartial"),
                        ActionType::PostSb,
                        true,
                        false,
                        None,
                        false,
                        Some(50),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("VillainNoShow"),
                        ActionType::PostBb,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Call,
                        false,
                        false,
                        None,
                        false,
                        Some(100),
                        Some(100),
                    ),
                    (
                        3,
                        Street::Preflop,
                        Some("VillainPartial"),
                        ActionType::Call,
                        false,
                        false,
                        None,
                        false,
                        Some(50),
                        Some(50),
                    ),
                    (
                        4,
                        Street::Preflop,
                        Some("VillainNoShow"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        5,
                        Street::Flop,
                        Some("VillainPartial"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        6,
                        Street::Flop,
                        Some("VillainNoShow"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        7,
                        Street::Flop,
                        Some("Hero"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        8,
                        Street::Turn,
                        Some("VillainPartial"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        9,
                        Street::Turn,
                        Some("VillainNoShow"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        10,
                        Street::Turn,
                        Some("Hero"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        11,
                        Street::Showdown,
                        Some("VillainPartial"),
                        ActionType::Show,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        12,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Show,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        13,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(300),
                        None,
                    ),
                ],
                warnings: vec![
                    "unsupported_no_show_line: VillainNoShow: doesn't show hand",
                    "partial_reveal_show_line: VillainPartial: shows [5d]",
                    "partial_reveal_summary_show_surface: Seat 2: VillainPartial (small blind) showed [5d] and lost",
                ],
            },
        ),
        (
            "BRCM0403",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("ShortAnte"),
                        ActionType::PostAnte,
                        true,
                        true,
                        Some(AllInReason::AnteExhausted),
                        true,
                        Some(100),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::PostAnte,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        3,
                        Street::Showdown,
                        Some("ShortAnte"),
                        ActionType::Show,
                        false,
                        true,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        4,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Show,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        5,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(200),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
        (
            "BRCM0404",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::PostSb,
                        true,
                        false,
                        None,
                        false,
                        Some(50),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("ShortBb"),
                        ActionType::PostBb,
                        true,
                        true,
                        Some(AllInReason::BlindExhausted),
                        true,
                        Some(60),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Call,
                        false,
                        false,
                        None,
                        false,
                        Some(10),
                        Some(10),
                    ),
                    (
                        3,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Show,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        4,
                        Street::Showdown,
                        Some("ShortBb"),
                        ActionType::Show,
                        false,
                        true,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        5,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(120),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
        (
            "BRCM0405",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::PostAnte,
                        true,
                        false,
                        None,
                        false,
                        Some(20),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("VillainDead"),
                        ActionType::PostAnte,
                        true,
                        false,
                        None,
                        false,
                        Some(20),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("VillainMuck"),
                        ActionType::PostAnte,
                        true,
                        false,
                        None,
                        false,
                        Some(20),
                        None,
                    ),
                    (
                        3,
                        Street::Preflop,
                        Some("VillainDead"),
                        ActionType::PostDead,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        4,
                        Street::Preflop,
                        Some("VillainMuck"),
                        ActionType::PostBb,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        5,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Fold,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        6,
                        Street::Preflop,
                        Some("VillainDead"),
                        ActionType::Fold,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        7,
                        Street::Preflop,
                        Some("VillainMuck"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(260),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
        (
            "BRCM0502",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("ShortyA"),
                        ActionType::PostAnte,
                        true,
                        true,
                        Some(AllInReason::AnteExhausted),
                        true,
                        Some(100),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("ShortyB"),
                        ActionType::PostAnte,
                        true,
                        true,
                        Some(AllInReason::AnteExhausted),
                        true,
                        Some(100),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::PostAnte,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        3,
                        Street::Preflop,
                        Some("Villain"),
                        ActionType::PostAnte,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        4,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Bet,
                        false,
                        true,
                        Some(AllInReason::Voluntary),
                        false,
                        Some(200),
                        None,
                    ),
                    (
                        5,
                        Street::Preflop,
                        Some("Villain"),
                        ActionType::Call,
                        false,
                        true,
                        Some(AllInReason::CallExhausted),
                        false,
                        Some(200),
                        Some(200),
                    ),
                    (
                        6,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Show,
                        false,
                        true,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        7,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Collect,
                        false,
                        true,
                        None,
                        false,
                        Some(400),
                        None,
                    ),
                    (
                        8,
                        Street::Showdown,
                        Some("Villain"),
                        ActionType::Collect,
                        false,
                        true,
                        None,
                        false,
                        Some(400),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
        (
            "BRCM0503",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::PostAnte,
                        true,
                        false,
                        None,
                        false,
                        Some(1),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("Villain"),
                        ActionType::PostAnte,
                        true,
                        false,
                        None,
                        false,
                        Some(1),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("DeadMoney"),
                        ActionType::PostAnte,
                        true,
                        true,
                        Some(AllInReason::AnteExhausted),
                        true,
                        Some(1),
                        None,
                    ),
                    (
                        3,
                        Street::Preflop,
                        Some("Villain"),
                        ActionType::Bet,
                        false,
                        true,
                        Some(AllInReason::Voluntary),
                        false,
                        Some(199),
                        None,
                    ),
                    (
                        4,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Call,
                        false,
                        true,
                        Some(AllInReason::CallExhausted),
                        false,
                        Some(199),
                        Some(199),
                    ),
                    (
                        5,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Show,
                        false,
                        true,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        6,
                        Street::Showdown,
                        Some("Villain"),
                        ActionType::Show,
                        false,
                        true,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        7,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Collect,
                        false,
                        true,
                        None,
                        false,
                        Some(201),
                        None,
                    ),
                    (
                        8,
                        Street::Showdown,
                        Some("Villain"),
                        ActionType::Collect,
                        false,
                        true,
                        None,
                        false,
                        Some(200),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
        (
            "BRCM0601",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("Shorty"),
                        ActionType::PostSb,
                        true,
                        false,
                        None,
                        false,
                        Some(50),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("Medium"),
                        ActionType::PostBb,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::RaiseTo,
                        false,
                        false,
                        None,
                        false,
                        Some(400),
                        Some(500),
                    ),
                    (
                        3,
                        Street::Preflop,
                        Some("Shorty"),
                        ActionType::Call,
                        false,
                        true,
                        Some(AllInReason::CallExhausted),
                        false,
                        Some(450),
                        Some(450),
                    ),
                    (
                        4,
                        Street::Preflop,
                        Some("Medium"),
                        ActionType::Call,
                        false,
                        false,
                        None,
                        false,
                        Some(400),
                        Some(400),
                    ),
                    (
                        5,
                        Street::Flop,
                        Some("Medium"),
                        ActionType::Bet,
                        false,
                        true,
                        Some(AllInReason::Voluntary),
                        false,
                        Some(500),
                        None,
                    ),
                    (
                        6,
                        Street::Flop,
                        Some("Hero"),
                        ActionType::Call,
                        false,
                        false,
                        None,
                        false,
                        Some(500),
                        Some(500),
                    ),
                    (
                        7,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Show,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        8,
                        Street::Showdown,
                        Some("Shorty"),
                        ActionType::Show,
                        false,
                        true,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        9,
                        Street::Showdown,
                        Some("Medium"),
                        ActionType::Show,
                        false,
                        true,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        10,
                        Street::Showdown,
                        Some("Shorty"),
                        ActionType::Collect,
                        false,
                        true,
                        None,
                        false,
                        Some(1500),
                        None,
                    ),
                    (
                        11,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(1000),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
        (
            "BRLEGAL2",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::PostSb,
                        true,
                        false,
                        None,
                        false,
                        Some(50),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("Villain"),
                        ActionType::PostBb,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("Villain"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        3,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Call,
                        false,
                        false,
                        None,
                        false,
                        Some(50),
                        Some(50),
                    ),
                    (
                        4,
                        Street::Flop,
                        Some("Villain"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        5,
                        Street::Flop,
                        Some("Hero"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        6,
                        Street::Turn,
                        Some("Villain"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        7,
                        Street::Turn,
                        Some("Hero"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        8,
                        Street::River,
                        Some("Villain"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        9,
                        Street::River,
                        Some("Hero"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        10,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Show,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        11,
                        Street::Showdown,
                        Some("Villain"),
                        ActionType::Show,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        12,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(200),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
        (
            "BRLEGAL3",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::PostSb,
                        true,
                        false,
                        None,
                        false,
                        Some(50),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("Villain"),
                        ActionType::PostBb,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Call,
                        false,
                        false,
                        None,
                        false,
                        Some(50),
                        Some(50),
                    ),
                    (
                        3,
                        Street::Preflop,
                        Some("Villain"),
                        ActionType::Check,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        4,
                        Street::Flop,
                        Some("Hero"),
                        ActionType::Bet,
                        false,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        5,
                        Street::Flop,
                        Some("Villain"),
                        ActionType::Fold,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        6,
                        Street::Flop,
                        Some("Hero"),
                        ActionType::ReturnUncalled,
                        false,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        7,
                        Street::Flop,
                        Some("Hero"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(200),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
        (
            "BRLEGAL4",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("VillainA"),
                        ActionType::PostSb,
                        true,
                        false,
                        None,
                        false,
                        Some(50),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("Shorty"),
                        ActionType::PostBb,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::RaiseTo,
                        false,
                        false,
                        None,
                        false,
                        Some(200),
                        Some(300),
                    ),
                    (
                        3,
                        Street::Preflop,
                        Some("VillainA"),
                        ActionType::Call,
                        false,
                        false,
                        None,
                        false,
                        Some(250),
                        Some(250),
                    ),
                    (
                        4,
                        Street::Preflop,
                        Some("Shorty"),
                        ActionType::RaiseTo,
                        false,
                        true,
                        Some(AllInReason::RaiseExhausted),
                        false,
                        Some(300),
                        Some(400),
                    ),
                    (
                        5,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::RaiseTo,
                        false,
                        false,
                        None,
                        false,
                        Some(400),
                        Some(800),
                    ),
                    (
                        6,
                        Street::Preflop,
                        Some("VillainA"),
                        ActionType::Fold,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        7,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::ReturnUncalled,
                        false,
                        false,
                        None,
                        false,
                        Some(400),
                        None,
                    ),
                    (
                        8,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Show,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        9,
                        Street::Showdown,
                        Some("Shorty"),
                        ActionType::Show,
                        false,
                        true,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        10,
                        Street::Showdown,
                        Some("Hero"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(1100),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
        (
            "BRSIDE1",
            EdgeParseContract {
                actions: vec![
                    (
                        0,
                        Street::Preflop,
                        Some("Shorty"),
                        ActionType::PostSb,
                        true,
                        false,
                        None,
                        false,
                        Some(50),
                        None,
                    ),
                    (
                        1,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::PostBb,
                        true,
                        false,
                        None,
                        false,
                        Some(100),
                        None,
                    ),
                    (
                        2,
                        Street::Preflop,
                        Some("Medium"),
                        ActionType::Call,
                        false,
                        false,
                        None,
                        false,
                        Some(100),
                        Some(100),
                    ),
                    (
                        3,
                        Street::Preflop,
                        Some("BigStack"),
                        ActionType::RaiseTo,
                        false,
                        false,
                        None,
                        false,
                        Some(400),
                        Some(500),
                    ),
                    (
                        4,
                        Street::Preflop,
                        Some("Shorty"),
                        ActionType::Call,
                        false,
                        true,
                        Some(AllInReason::CallExhausted),
                        false,
                        Some(450),
                        Some(450),
                    ),
                    (
                        5,
                        Street::Preflop,
                        Some("Hero"),
                        ActionType::Fold,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        6,
                        Street::Preflop,
                        Some("Medium"),
                        ActionType::RaiseTo,
                        false,
                        true,
                        Some(AllInReason::RaiseExhausted),
                        false,
                        Some(500),
                        Some(1000),
                    ),
                    (
                        7,
                        Street::Preflop,
                        Some("BigStack"),
                        ActionType::Call,
                        false,
                        false,
                        None,
                        false,
                        Some(500),
                        Some(500),
                    ),
                    (
                        8,
                        Street::Showdown,
                        Some("Medium"),
                        ActionType::Show,
                        false,
                        true,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        9,
                        Street::Showdown,
                        Some("BigStack"),
                        ActionType::Show,
                        false,
                        false,
                        None,
                        false,
                        None,
                        None,
                    ),
                    (
                        10,
                        Street::Showdown,
                        Some("BigStack"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(400),
                        None,
                    ),
                    (
                        11,
                        Street::Showdown,
                        Some("BigStack"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(1200),
                        None,
                    ),
                    (
                        12,
                        Street::Showdown,
                        Some("BigStack"),
                        ActionType::Collect,
                        false,
                        false,
                        None,
                        false,
                        Some(1000),
                        None,
                    ),
                ],
                warnings: vec![],
            },
        ),
    ]
    .into_iter()
    .collect()
}

fn expected_normalization_contracts() -> BTreeMap<&'static str, EdgeNormalizationContract> {
    [
        (
            "BRCM0401",
            EdgeNormalizationContract {
                committed: vec![
                    ("Hero", 0),
                    ("Sitout", 0),
                    ("VillainDead", 100),
                    ("VillainMuck", 100),
                ],
                returns: vec![],
                final_pots: vec![(1, 200, true)],
                pot_contributions: vec![(1, "VillainDead", 100), (1, "VillainMuck", 100)],
                pot_eligibilities: vec![(1, "VillainDead"), (1, "VillainMuck")],
                invariant_errors: vec![
                    "premature_street_close: street=preflop pending=VillainDead,VillainMuck",
                ],
                uncertain_reason_codes: vec![],
            },
        ),
        (
            "BRCM0402",
            EdgeNormalizationContract {
                committed: vec![
                    ("Hero", 100),
                    ("VillainNoShow", 100),
                    ("VillainPartial", 100),
                ],
                returns: vec![],
                final_pots: vec![(1, 300, true)],
                pot_contributions: vec![
                    (1, "Hero", 100),
                    (1, "VillainPartial", 100),
                    (1, "VillainNoShow", 100),
                ],
                pot_eligibilities: vec![
                    (1, "Hero"),
                    (1, "VillainPartial"),
                    (1, "VillainNoShow"),
                ],
                invariant_errors: vec![],
                uncertain_reason_codes: vec![],
            },
        ),
        (
            "BRCM0403",
            EdgeNormalizationContract {
                committed: vec![("Hero", 100), ("ShortAnte", 100)],
                returns: vec![],
                final_pots: vec![(1, 200, true)],
                pot_contributions: vec![(1, "ShortAnte", 100), (1, "Hero", 100)],
                pot_eligibilities: vec![(1, "ShortAnte"), (1, "Hero")],
                invariant_errors: vec![],
                uncertain_reason_codes: vec![],
            },
        ),
        (
            "BRCM0404",
            EdgeNormalizationContract {
                committed: vec![("Hero", 60), ("ShortBb", 60)],
                returns: vec![],
                final_pots: vec![(1, 120, true)],
                pot_contributions: vec![(1, "Hero", 60), (1, "ShortBb", 60)],
                pot_eligibilities: vec![(1, "Hero"), (1, "ShortBb")],
                invariant_errors: vec![],
                uncertain_reason_codes: vec![],
            },
        ),
        (
            "BRCM0405",
            EdgeNormalizationContract {
                committed: vec![("Hero", 20), ("VillainDead", 120), ("VillainMuck", 120)],
                returns: vec![],
                final_pots: vec![(1, 60, true), (2, 200, false)],
                pot_contributions: vec![
                    (1, "Hero", 20),
                    (1, "VillainDead", 20),
                    (1, "VillainMuck", 20),
                    (2, "VillainDead", 100),
                    (2, "VillainMuck", 100),
                ],
                pot_eligibilities: vec![(1, "VillainMuck"), (2, "VillainMuck")],
                invariant_errors: vec![],
                uncertain_reason_codes: vec![],
            },
        ),
        (
            "BRCM0502",
            EdgeNormalizationContract {
                committed: vec![
                    ("Hero", 300),
                    ("ShortyA", 100),
                    ("ShortyB", 100),
                    ("Villain", 300),
                ],
                returns: vec![],
                final_pots: vec![(1, 400, true), (2, 400, false)],
                pot_contributions: vec![
                    (1, "ShortyA", 100),
                    (1, "ShortyB", 100),
                    (1, "Hero", 100),
                    (1, "Villain", 100),
                    (2, "Hero", 200),
                    (2, "Villain", 200),
                ],
                pot_eligibilities: vec![
                    (1, "ShortyA"),
                    (1, "ShortyB"),
                    (1, "Hero"),
                    (1, "Villain"),
                    (2, "Hero"),
                    (2, "Villain"),
                ],
                invariant_errors: vec![],
                uncertain_reason_codes: vec![
                    "pot_settlement_ambiguous_hidden_showdown: pot_no=1, eligible_players=Hero|Villain",
                    "pot_settlement_ambiguous_hidden_showdown: pot_no=2, eligible_players=Hero|Villain",
                ],
            },
        ),
        (
            "BRCM0503",
            EdgeNormalizationContract {
                committed: vec![("DeadMoney", 1), ("Hero", 200), ("Villain", 200)],
                returns: vec![],
                final_pots: vec![(1, 3, true), (2, 398, false)],
                pot_contributions: vec![
                    (1, "Hero", 1),
                    (1, "Villain", 1),
                    (1, "DeadMoney", 1),
                    (2, "Hero", 199),
                    (2, "Villain", 199),
                ],
                pot_eligibilities: vec![
                    (1, "Hero"),
                    (1, "Villain"),
                    (1, "DeadMoney"),
                    (2, "Hero"),
                    (2, "Villain"),
                ],
                invariant_errors: vec![],
                uncertain_reason_codes: vec![],
            },
        ),
        (
            "BRCM0601",
            EdgeNormalizationContract {
                committed: vec![("Hero", 1000), ("Medium", 1000), ("Shorty", 500)],
                returns: vec![],
                final_pots: vec![(1, 1500, true), (2, 1000, false)],
                pot_contributions: vec![
                    (1, "Hero", 500),
                    (1, "Shorty", 500),
                    (1, "Medium", 500),
                    (2, "Hero", 500),
                    (2, "Medium", 500),
                ],
                pot_eligibilities: vec![
                    (1, "Hero"),
                    (1, "Shorty"),
                    (1, "Medium"),
                    (2, "Hero"),
                    (2, "Medium"),
                ],
                invariant_errors: vec![],
                uncertain_reason_codes: vec![],
            },
        ),
        (
            "BRLEGAL2",
            EdgeNormalizationContract {
                committed: vec![("Hero", 100), ("Villain", 100)],
                returns: vec![],
                final_pots: vec![(1, 200, true)],
                pot_contributions: vec![(1, "Hero", 100), (1, "Villain", 100)],
                pot_eligibilities: vec![(1, "Hero"), (1, "Villain")],
                invariant_errors: vec![
                    "illegal_actor_order: street=preflop seq=2 expected=Hero actual=Villain raw_line=Villain: checks",
                ],
                uncertain_reason_codes: vec![],
            },
        ),
        (
            "BRLEGAL3",
            EdgeNormalizationContract {
                committed: vec![("Hero", 100), ("Villain", 100)],
                returns: vec![("Hero", 100, "uncalled")],
                final_pots: vec![(1, 200, true)],
                pot_contributions: vec![(1, "Hero", 100), (1, "Villain", 100)],
                pot_eligibilities: vec![(1, "Hero")],
                invariant_errors: vec![
                    "illegal_actor_order: street=flop seq=4 expected=Villain actual=Hero raw_line=Hero: bets 100",
                ],
                uncertain_reason_codes: vec![],
            },
        ),
        (
            "BRLEGAL4",
            EdgeNormalizationContract {
                committed: vec![("Hero", 400), ("Shorty", 400), ("VillainA", 300)],
                returns: vec![("Hero", 400, "uncalled")],
                final_pots: vec![(1, 900, true), (2, 200, false)],
                pot_contributions: vec![
                    (1, "Hero", 300),
                    (1, "VillainA", 300),
                    (1, "Shorty", 300),
                    (2, "Hero", 100),
                    (2, "Shorty", 100),
                ],
                pot_eligibilities: vec![
                    (1, "Hero"),
                    (1, "Shorty"),
                    (2, "Hero"),
                    (2, "Shorty"),
                ],
                invariant_errors: vec![
                    "action_not_reopened_after_short_all_in: street=preflop seq=5 player=Hero raw_line=Hero: raises 400 to 800",
                ],
                uncertain_reason_codes: vec![],
            },
        ),
        (
            "BRSIDE1",
            EdgeNormalizationContract {
                committed: vec![
                    ("BigStack", 1000),
                    ("Hero", 100),
                    ("Medium", 1000),
                    ("Shorty", 500),
                ],
                returns: vec![],
                final_pots: vec![(1, 400, true), (2, 1200, false), (3, 1000, false)],
                pot_contributions: vec![
                    (1, "Shorty", 100),
                    (1, "Hero", 100),
                    (1, "Medium", 100),
                    (1, "BigStack", 100),
                    (2, "Shorty", 400),
                    (2, "Medium", 400),
                    (2, "BigStack", 400),
                    (3, "Medium", 500),
                    (3, "BigStack", 500),
                ],
                pot_eligibilities: vec![
                    (1, "Shorty"),
                    (1, "Medium"),
                    (1, "BigStack"),
                    (2, "Shorty"),
                    (2, "Medium"),
                    (2, "BigStack"),
                    (3, "Medium"),
                    (3, "BigStack"),
                ],
                invariant_errors: vec![
                    "illegal_small_blind_actor: seq=0 expected=Hero actual=Shorty raw_line=Shorty: posts small blind 50",
                    "illegal_big_blind_actor: seq=1 expected=Medium actual=Hero raw_line=Hero: posts big blind 100",
                    "illegal_actor_order: street=preflop seq=2 expected=BigStack actual=Medium raw_line=Medium: calls 100",
                ],
                uncertain_reason_codes: vec![],
            },
        ),
    ]
    .into_iter()
    .collect()
}

fn action_contracts(
    hand: &CanonicalParsedHand,
) -> Vec<(
    usize,
    Street,
    Option<String>,
    ActionType,
    bool,
    bool,
    Option<AllInReason>,
    bool,
    Option<i64>,
    Option<i64>,
)> {
    hand.actions
        .iter()
        .map(|event| {
            (
                event.seq,
                event.street,
                event.player_name.clone(),
                event.action_type,
                event.is_forced,
                event.is_all_in,
                event.all_in_reason,
                event.forced_all_in_preflop,
                event.amount,
                event.to_amount,
            )
        })
        .collect::<Vec<_>>()
}

fn committed_contract(hand: &NormalizedHand) -> Vec<(String, i64)> {
    hand.actual
        .committed_total_by_player
        .iter()
        .map(|(player_name, amount)| (player_name.clone(), *amount))
        .collect()
}

fn returns_contract(hand: &NormalizedHand) -> Vec<(String, i64, String)> {
    hand.returns
        .iter()
        .map(|hand_return| {
            (
                hand_return.player_name.clone(),
                hand_return.amount,
                hand_return.reason.clone(),
            )
        })
        .collect()
}

fn final_pots_contract(hand: &NormalizedHand) -> Vec<FinalPotContract> {
    hand.final_pots
        .iter()
        .map(|pot| (pot.pot_no, pot.amount, pot.is_main))
        .collect()
}

fn pot_contributions_contract(hand: &NormalizedHand) -> Vec<(u8, String, i64)> {
    hand.pot_contributions
        .iter()
        .map(|contribution| {
            (
                contribution.pot_no,
                contribution.player_name.clone(),
                contribution.amount,
            )
        })
        .collect()
}

fn pot_eligibilities_contract(hand: &NormalizedHand) -> Vec<(u8, String)> {
    hand.pot_eligibilities
        .iter()
        .map(|eligibility| (eligibility.pot_no, eligibility.player_name.clone()))
        .collect()
}

fn materialize_action_contracts(
    contracts: &[ActionContract],
) -> Vec<(
    usize,
    Street,
    Option<String>,
    ActionType,
    bool,
    bool,
    Option<AllInReason>,
    bool,
    Option<i64>,
    Option<i64>,
)> {
    contracts
        .iter()
        .map(
            |(
                seq,
                street,
                player_name,
                action_type,
                is_forced,
                is_all_in,
                all_in_reason,
                forced_all_in_preflop,
                amount,
                to_amount,
            )| {
                (
                    *seq,
                    *street,
                    player_name.map(str::to_string),
                    *action_type,
                    *is_forced,
                    *is_all_in,
                    *all_in_reason,
                    *forced_all_in_preflop,
                    *amount,
                    *to_amount,
                )
            },
        )
        .collect()
}

fn materialize_committed_contract(contracts: &[CommittedContract]) -> Vec<(String, i64)> {
    contracts
        .iter()
        .map(|(player_name, amount)| ((*player_name).to_string(), *amount))
        .collect()
}

fn materialize_returns_contract(contracts: &[ReturnContract]) -> Vec<(String, i64, String)> {
    contracts
        .iter()
        .map(|(player_name, amount, reason)| ((*player_name).to_string(), *amount, (*reason).to_string()))
        .collect()
}

fn materialize_pot_contributions_contract(
    contracts: &[PotContributionContract],
) -> Vec<(u8, String, i64)> {
    contracts
        .iter()
        .map(|(pot_no, player_name, amount)| (*pot_no, (*player_name).to_string(), *amount))
        .collect()
}

fn materialize_pot_eligibilities_contract(
    contracts: &[PotEligibilityContract],
) -> Vec<(u8, String)> {
    contracts
        .iter()
        .map(|(pot_no, player_name)| (*pot_no, (*player_name).to_string()))
        .collect()
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

fn find_action<'a>(
    hand: &'a CanonicalParsedHand,
    player_name: &str,
    action_type: ActionType,
) -> &'a tracker_parser_core::models::HandActionEvent {
    hand.actions
        .iter()
        .find(|event| {
            event.player_name.as_deref() == Some(player_name) && event.action_type == action_type
        })
        .unwrap_or_else(|| panic!("missing action `{action_type:?}` for `{player_name}`"))
}
