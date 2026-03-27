use tracker_parser_core::{
    models::{CertaintyState, InvariantIssue, NormalizedHand},
    normalizer::normalize_hand,
    parsers::hand_history::parse_canonical_hand,
};

#[test]
fn collect_line_reordering_preserves_canonical_pot_projection() {
    let original = normalize_projection(SPLIT_COLLECT_HAND);
    let mutated = normalize_projection(&reorder_collect_lines(SPLIT_COLLECT_HAND));

    assert_eq!(mutated, original);
}

#[test]
fn summary_seat_line_reordering_preserves_canonical_pot_projection() {
    let original = normalize_projection(SUMMARY_REORDER_HAND);
    let mutated = normalize_projection(&reorder_summary_seat_lines(SUMMARY_REORDER_HAND));

    assert_eq!(mutated, original);
}

#[test]
fn action_reordering_is_not_treated_as_safe_mutation() {
    let original = normalize_projection(ACTION_ORDER_HAND);
    let mutated = normalize_projection(&reorder_action_lines_unsafely(ACTION_ORDER_HAND));

    assert_ne!(mutated, original);
}

#[test]
fn uncalled_return_does_not_create_chips() {
    let projection = normalize_projection(ACTION_ORDER_HAND);

    let ProjectionResult::Normalized(projection) = projection else {
        panic!("expected normalized projection for ACTION_ORDER_HAND");
    };

    assert!(projection.chip_conservation_ok);
    assert!(projection.pot_conservation_ok);
    assert_eq!(
        projection.returns,
        vec![("Hero".to_string(), 200, "uncalled".to_string())]
    );
}

#[test]
fn canonical_smoke_hands_keep_non_negative_stacks() {
    for raw in [SPLIT_COLLECT_HAND, SUMMARY_REORDER_HAND, ACTION_ORDER_HAND] {
        let hand = parse_canonical_hand(raw).unwrap();
        let normalized = normalize_hand(&hand).unwrap();

        assert!(normalized.actual.stacks_after_actual.values().all(|stack| *stack >= 0));
    }
}

const SPLIT_COLLECT_HAND: &str = r#"Poker Hand #QSPROP0001: Tournament #995001, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/17 11:00:00
Table '1' 2-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: Villain (1,000 in chips)
Hero: posts small blind 50
Villain: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Qd]
Hero: calls 50
Villain: checks
*** FLOP *** [2c 7d 9h]
Villain: checks
Hero: checks
*** TURN *** [2c 7d 9h] [Qs]
Villain: checks
Hero: checks
*** RIVER *** [2c 7d 9h Qs] [3c]
Villain: checks
Hero: checks
*** SHOWDOWN ***
Hero: shows [Ah Qd]
Villain: shows [Ad Qh]
Hero collected 100 from pot
Villain collected 100 from pot
*** SUMMARY ***
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Qd] and won (100) with a pair of Queens
Seat 2: Villain (big blind) showed [Ad Qh] and won (100) with a pair of Queens
"#;

const SUMMARY_REORDER_HAND: &str = r#"Poker Hand #QSPROP0002: Tournament #995002, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/17 11:05:00
Table '1' 3-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainA (1,000 in chips)
Seat 3: VillainB (1,000 in chips)
VillainA: posts small blind 50
VillainB: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Hero: raises 200 to 300
VillainA: folds
VillainB: calls 200
*** FLOP *** [2c 7d 9h]
VillainB: checks
Hero: checks
*** TURN *** [2c 7d 9h] [Qs]
VillainB: checks
Hero: checks
*** RIVER *** [2c 7d 9h Qs] [3c]
VillainB: checks
Hero: checks
*** SHOWDOWN ***
Hero: shows [Ah Ad]
VillainB: shows [Kh Kd]
Hero collected 650 from pot
*** SUMMARY ***
Total pot 650 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and won (650)
Seat 2: VillainA (small blind) folded before Flop
Seat 3: VillainB (big blind) showed [Kh Kd] and lost
"#;

const ACTION_ORDER_HAND: &str = r#"Poker Hand #QSPROP0003: Tournament #995003, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/17 11:10:00
Table '1' 2-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: Villain (1,000 in chips)
Hero: posts small blind 50
Villain: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Hero: raises 100 to 200
Villain: calls 100
*** FLOP *** [2c 7d 9h]
Villain: checks
Hero: bets 200
Villain: folds
Uncalled bet (200) returned to Hero
*** SHOWDOWN ***
Hero collected 400 from pot
*** SUMMARY ***
Total pot 400 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h]
Seat 1: Hero (button) won (400)
Seat 2: Villain (big blind) folded on the Flop
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProjectionResult {
    Normalized(CanonicalPotProjection),
    ParseError(String),
    NormalizeError(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CanonicalPotProjection {
    certainty_state: CertaintyState,
    chip_conservation_ok: bool,
    pot_conservation_ok: bool,
    returns: Vec<(String, i64, String)>,
    final_pots: Vec<(u8, i64, bool)>,
    pot_contributions: Vec<(u8, String, i64)>,
    pot_eligibilities: Vec<(u8, String)>,
    pot_winners: Vec<(u8, String, i64)>,
    invariant_issue_codes: Vec<&'static str>,
}

impl CanonicalPotProjection {
    fn from_normalized(normalized: &NormalizedHand) -> Self {
        Self {
            certainty_state: normalized.settlement.certainty_state,
            chip_conservation_ok: normalized.invariants.chip_conservation_ok,
            pot_conservation_ok: normalized.invariants.pot_conservation_ok,
            returns: normalized
                .returns
                .iter()
                .map(|hand_return| {
                    (
                        hand_return.player_name.clone(),
                        hand_return.amount,
                        hand_return.reason.clone(),
                    )
                })
                .collect(),
            final_pots: normalized
                .settlement
                .final_pots()
                .into_iter()
                .map(|pot| (pot.pot_no, pot.amount, pot.is_main))
                .collect(),
            pot_contributions: normalized
                .settlement
                .pot_contributions()
                .into_iter()
                .map(|contribution| {
                    (
                        contribution.pot_no,
                        contribution.player_name,
                        contribution.amount,
                    )
                })
                .collect(),
            pot_eligibilities: normalized
                .settlement
                .pot_eligibilities()
                .into_iter()
                .map(|eligibility| (eligibility.pot_no, eligibility.player_name))
                .collect(),
            pot_winners: normalized
                .settlement
                .pot_winners()
                .into_iter()
                .map(|winner| (winner.pot_no, winner.player_name, winner.share_amount))
                .collect(),
            invariant_issue_codes: normalized
                .invariants
                .issues
                .iter()
                .map(invariant_issue_code)
                .collect(),
        }
    }
}

fn normalize_projection(raw: &str) -> ProjectionResult {
    let hand = match parse_canonical_hand(raw) {
        Ok(hand) => hand,
        Err(error) => return ProjectionResult::ParseError(error.to_string()),
    };
    let normalized = match normalize_hand(&hand) {
        Ok(normalized) => normalized,
        Err(error) => return ProjectionResult::NormalizeError(error.to_string()),
    };

    ProjectionResult::Normalized(CanonicalPotProjection::from_normalized(&normalized))
}

fn reorder_collect_lines(raw: &str) -> String {
    reorder_matching_lines(raw, |line| line.contains(" collected ") && line.ends_with("from pot"))
}

fn reorder_summary_seat_lines(raw: &str) -> String {
    reorder_lines_inside_summary(raw, |line| line.starts_with("Seat "))
}

fn reorder_action_lines_unsafely(raw: &str) -> String {
    reorder_exact_lines(raw, &["Hero: bets 200", "Villain: folds"])
}

fn reorder_matching_lines(raw: &str, predicate: impl Fn(&str) -> bool) -> String {
    let mut lines = raw.lines().map(ToString::to_string).collect::<Vec<_>>();
    let indices = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| predicate(line))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let reordered = indices
        .iter()
        .rev()
        .map(|index| lines[*index].clone())
        .collect::<Vec<_>>();

    for (index, replacement) in indices.into_iter().zip(reordered) {
        lines[index] = replacement;
    }

    let mut rebuilt = lines.join("\n");
    rebuilt.push('\n');
    rebuilt
}

fn reorder_lines_inside_summary(raw: &str, predicate: impl Fn(&str) -> bool) -> String {
    let mut lines = raw.lines().map(ToString::to_string).collect::<Vec<_>>();
    let Some(summary_index) = lines.iter().position(|line| line == "*** SUMMARY ***") else {
        panic!("missing summary section");
    };
    let indices = lines
        .iter()
        .enumerate()
        .skip(summary_index + 1)
        .filter(|(_, line)| predicate(line))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let reordered = indices
        .iter()
        .rev()
        .map(|index| lines[*index].clone())
        .collect::<Vec<_>>();

    for (index, replacement) in indices.into_iter().zip(reordered) {
        lines[index] = replacement;
    }

    let mut rebuilt = lines.join("\n");
    rebuilt.push('\n');
    rebuilt
}

fn reorder_exact_lines(raw: &str, targets: &[&str]) -> String {
    let mut lines = raw.lines().map(ToString::to_string).collect::<Vec<_>>();
    let indices = targets
        .iter()
        .map(|target| {
            lines.iter()
                .position(|line| line == target)
                .unwrap_or_else(|| panic!("missing target line `{target}`"))
        })
        .collect::<Vec<_>>();
    let replacements = indices
        .iter()
        .rev()
        .map(|index| lines[*index].clone())
        .collect::<Vec<_>>();

    for (index, replacement) in indices.into_iter().zip(replacements) {
        lines[index] = replacement;
    }

    let mut rebuilt = lines.join("\n");
    rebuilt.push('\n');
    rebuilt
}

fn invariant_issue_code(issue: &InvariantIssue) -> &'static str {
    match issue {
        InvariantIssue::ChipConservationMismatch { .. } => "chip_conservation_mismatch",
        InvariantIssue::PotConservationMismatch { .. } => "pot_conservation_mismatch",
        InvariantIssue::SummaryTotalPotMismatch { .. } => "summary_total_pot_mismatch",
        InvariantIssue::PrematureStreetClose { .. } => "premature_street_close",
        InvariantIssue::IllegalActorOrder { .. } => "illegal_actor_order",
        InvariantIssue::IllegalSmallBlindActor { .. } => "illegal_small_blind_actor",
        InvariantIssue::IllegalBigBlindActor { .. } => "illegal_big_blind_actor",
        InvariantIssue::UncalledReturnActorMismatch { .. } => "uncalled_return_actor_mismatch",
        InvariantIssue::UncalledReturnAmountMismatch { .. } => "uncalled_return_amount_mismatch",
        InvariantIssue::IllegalCheck { .. } => "illegal_check",
        InvariantIssue::IllegalCallAmount { .. } => "illegal_call_amount",
        InvariantIssue::UndercallInconsistency { .. } => "undercall_inconsistency",
        InvariantIssue::OvercallInconsistency { .. } => "overcall_inconsistency",
        InvariantIssue::IllegalBetFacingOpenBet { .. } => "illegal_bet_facing_open_bet",
        InvariantIssue::ActionNotReopenedAfterShortAllIn { .. } => {
            "action_not_reopened_after_short_all_in"
        }
        InvariantIssue::IncompleteRaiseToCall { .. } => "incomplete_raise_to_call",
        InvariantIssue::IncompleteRaiseSize { .. } => "incomplete_raise_size",
    }
}
