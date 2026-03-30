use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ParserError,
    models::{
        CanonicalParsedHand, CertaintyState, HandSettlement, HandSettlementEvidence,
        ParseIssueCode, ParsedHandSeat, PlayerStatus, PotContribution, PotEligibility,
        PotSettlementIssue, SettlementAllocation, SettlementAllocationSource,
        SettlementCollectEvent, SettlementIssue, SettlementPot, SettlementShare,
        SettlementShowHand, SettlementSummaryOutcome, SummarySeatOutcomeKind,
    },
    street_strength::evaluate_river_showdown_ranks,
};

#[derive(Debug, Clone)]
struct ConstructedPot {
    pot_no: u8,
    amount: i64,
    is_main: bool,
    eligible_players: Vec<String>,
    contributions: Vec<PotContribution>,
    eligibilities: Vec<PotEligibility>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PotSettlementOption {
    pot_no: u8,
    source: SettlementAllocationSource,
    shares: Vec<(String, i64)>,
}

#[derive(Debug, Clone)]
struct PotOptions {
    pot_no: u8,
    options: Vec<PotSettlementOption>,
}

enum PotOptionBuild {
    Ready(Vec<PotSettlementOption>),
    Uncertain(PotSettlementIssue),
}

pub(crate) enum ObservedPayouts {
    Missing,
    Ready(BTreeMap<String, i64>),
    Conflict {
        collect_payouts: BTreeMap<String, i64>,
        _summary_payouts: BTreeMap<String, i64>,
    },
}

impl ObservedPayouts {
    pub(crate) fn settlement_totals(&self) -> BTreeMap<String, i64> {
        match self {
            Self::Ready(payouts) => payouts.clone(),
            Self::Missing | Self::Conflict { .. } => BTreeMap::new(),
        }
    }

    pub(crate) fn best_effort_totals(&self) -> BTreeMap<String, i64> {
        match self {
            Self::Missing => BTreeMap::new(),
            Self::Ready(payouts) => payouts.clone(),
            Self::Conflict {
                collect_payouts, ..
            } => collect_payouts.clone(),
        }
    }
}

pub(crate) struct PotResolutionOutcome {
    pub(crate) settlement: HandSettlement,
}

pub(crate) fn resolve_hand_pots(
    hand: &CanonicalParsedHand,
    ordered_seats: &[ParsedHandSeat],
    committed_total: &BTreeMap<String, i64>,
    status: &BTreeMap<String, PlayerStatus>,
) -> Result<PotResolutionOutcome, ParserError> {
    let seat_by_player = ordered_seats
        .iter()
        .map(|seat| (seat.player_name.clone(), seat.seat_no))
        .collect::<BTreeMap<_, _>>();
    let pots = construct_pots(ordered_seats, committed_total, status);
    let settlement = settle_pots(hand, &pots, &seat_by_player)?;

    Ok(PotResolutionOutcome { settlement })
}

fn construct_pots(
    ordered_seats: &[ParsedHandSeat],
    committed_total: &BTreeMap<String, i64>,
    status: &BTreeMap<String, PlayerStatus>,
) -> Vec<ConstructedPot> {
    let mut levels = committed_total
        .values()
        .copied()
        .filter(|amount| *amount > 0)
        .collect::<Vec<_>>();
    levels.sort_unstable();
    levels.dedup();

    let mut pots = Vec::new();
    let mut previous_level = 0_i64;

    for level in levels {
        let contributors = ordered_seats
            .iter()
            .filter(|seat| committed_total.get(&seat.player_name).copied().unwrap_or(0) >= level)
            .collect::<Vec<_>>();
        if contributors.is_empty() {
            continue;
        }

        let increment = level - previous_level;
        if increment <= 0 {
            previous_level = level;
            continue;
        }

        let pot_no = (pots.len() + 1) as u8;
        let amount = increment * contributors.len() as i64;
        let eligible_seats = contributors
            .iter()
            .filter(|seat| status[seat.player_name.as_str()] != PlayerStatus::Folded)
            .collect::<Vec<_>>();

        pots.push(ConstructedPot {
            pot_no,
            amount,
            is_main: pots.is_empty(),
            eligible_players: eligible_seats
                .iter()
                .map(|seat| seat.player_name.clone())
                .collect(),
            contributions: contributors
                .iter()
                .map(|seat| PotContribution {
                    pot_no,
                    seat_no: seat.seat_no,
                    player_name: seat.player_name.clone(),
                    amount: increment,
                })
                .collect(),
            eligibilities: eligible_seats
                .iter()
                .map(|seat| PotEligibility {
                    pot_no,
                    seat_no: seat.seat_no,
                    player_name: seat.player_name.clone(),
                })
                .collect(),
        });
        previous_level = level;
    }

    pots
}

fn settle_pots(
    hand: &CanonicalParsedHand,
    pots: &[ConstructedPot],
    seat_by_player: &BTreeMap<String, u8>,
) -> Result<HandSettlement, ParserError> {
    let evidence = build_settlement_evidence(hand, seat_by_player)?;

    if pots.is_empty() {
        let has_collections = hand.collected_amounts.values().any(|amount| *amount > 0);
        return Ok(HandSettlement {
            certainty_state: if has_collections {
                CertaintyState::Inconsistent
            } else {
                CertaintyState::Exact
            },
            issues: if has_collections {
                vec![SettlementIssue::CollectEventsWithoutPots]
            } else {
                Vec::new()
            },
            evidence,
            pots: Vec::new(),
        });
    }

    let observed_payouts = observed_payouts(hand);
    let observed_payout_totals = observed_payouts.settlement_totals();
    let showdown_ranks = showdown_rank_map(hand)?;
    let single_collector = (observed_payout_totals.len() == 1)
        .then(|| observed_payout_totals.keys().next().cloned())
        .flatten();
    let total_pots = pots.len();

    let mut pot_builds = Vec::new();
    for pot in pots {
        let contenders = pot_contenders(hand, &pot.eligible_players);
        let build = build_pot_options(
            hand,
            pot,
            total_pots,
            &observed_payout_totals,
            single_collector.as_deref(),
            &showdown_ranks,
            &contenders,
        );
        pot_builds.push((pot, contenders, build));
    }

    let pot_uncertainty = pot_builds
        .iter()
        .any(|(_, _, build)| matches!(build, PotOptionBuild::Uncertain(_)));
    if pot_uncertainty {
        return Ok(HandSettlement {
            certainty_state: CertaintyState::Uncertain,
            issues: Vec::new(),
            evidence,
            pots: build_settlement_pots(seat_by_player, pot_builds, &BTreeMap::new(), false)?,
        });
    }

    match observed_payouts {
        ObservedPayouts::Conflict { .. } => {
            return Ok(HandSettlement {
                certainty_state: CertaintyState::Inconsistent,
                issues: vec![
                    SettlementIssue::CollectConflictNoExactSettlementMatchesCollectedAmounts,
                ],
                evidence,
                pots: build_settlement_pots(seat_by_player, pot_builds, &BTreeMap::new(), false)?,
            });
        }
        ObservedPayouts::Missing => {
            let (certainty_state, issues) = if has_multiple_candidate_allocations(&pot_builds) {
                (
                    CertaintyState::Uncertain,
                    vec![SettlementIssue::MultipleExactAllocations],
                )
            } else {
                (
                    CertaintyState::Inconsistent,
                    vec![SettlementIssue::MissingCollections],
                )
            };

            return Ok(HandSettlement {
                certainty_state,
                issues,
                evidence,
                pots: build_settlement_pots(seat_by_player, pot_builds, &BTreeMap::new(), false)?,
            });
        }
        ObservedPayouts::Ready(_) => {}
    }

    let pot_options = pot_builds
        .iter()
        .map(|(pot, _, build)| match build {
            PotOptionBuild::Ready(options) => PotOptions {
                pot_no: pot.pot_no,
                options: options.clone(),
            },
            PotOptionBuild::Uncertain(_) => unreachable!("pot uncertainty handled earlier"),
        })
        .collect::<Vec<_>>();

    let mut remaining = observed_payout_totals.clone();
    let mut current = Vec::new();
    let mut solutions = Vec::new();
    search_settlement_combinations(
        &pot_options,
        0,
        &mut remaining,
        &mut current,
        &mut solutions,
        2,
    );

    let selected_allocations = if let Some(solution) = solutions.first().cloned() {
        if solutions.len() > 1 {
            return Ok(HandSettlement {
                certainty_state: CertaintyState::Uncertain,
                issues: vec![SettlementIssue::MultipleExactAllocations],
                evidence,
                pots: build_settlement_pots(seat_by_player, pot_builds, &BTreeMap::new(), false)?,
            });
        }

        solution
            .into_iter()
            .map(|option| (option.pot_no, option))
            .collect::<BTreeMap<_, _>>()
    } else {
        return Ok(HandSettlement {
            certainty_state: CertaintyState::Inconsistent,
            issues: vec![SettlementIssue::CollectConflictNoExactSettlementMatchesCollectedAmounts],
            evidence,
            pots: build_settlement_pots(seat_by_player, pot_builds, &BTreeMap::new(), false)?,
        });
    };

    Ok(HandSettlement {
        certainty_state: CertaintyState::Exact,
        issues: Vec::new(),
        evidence,
        pots: build_settlement_pots(seat_by_player, pot_builds, &selected_allocations, true)?,
    })
}

pub(crate) fn observed_payouts(hand: &CanonicalParsedHand) -> ObservedPayouts {
    let collect_payouts = hand
        .collected_amounts
        .iter()
        .filter(|(_, amount)| **amount > 0)
        .map(|(player, amount)| (player.clone(), *amount))
        .collect::<BTreeMap<_, _>>();
    let summary_payouts = hand
        .summary_seat_outcomes
        .iter()
        .filter_map(|outcome| {
            outcome
                .won_amount
                .filter(|amount| *amount > 0)
                .map(|amount| (outcome.player_name.clone(), amount))
        })
        .fold(BTreeMap::new(), |mut payouts, (player, amount)| {
            *payouts.entry(player).or_default() += amount;
            payouts
        });

    match (collect_payouts.is_empty(), summary_payouts.is_empty()) {
        (true, true) => ObservedPayouts::Missing,
        (false, true) => ObservedPayouts::Ready(collect_payouts),
        (true, false) => ObservedPayouts::Ready(summary_payouts),
        (false, false) if collect_payouts == summary_payouts => {
            ObservedPayouts::Ready(collect_payouts)
        }
        (false, false) => ObservedPayouts::Conflict {
            collect_payouts,
            _summary_payouts: summary_payouts,
        },
    }
}

fn has_multiple_candidate_allocations(
    pot_builds: &[(&ConstructedPot, Vec<String>, PotOptionBuild)],
) -> bool {
    pot_builds.iter().any(|(_, _, build)| match build {
        PotOptionBuild::Ready(options) => options.len() > 1,
        PotOptionBuild::Uncertain(_) => false,
    })
}

fn build_settlement_pots(
    seat_by_player: &BTreeMap<String, u8>,
    pot_builds: Vec<(&ConstructedPot, Vec<String>, PotOptionBuild)>,
    selected_allocations: &BTreeMap<u8, PotSettlementOption>,
    allow_selected: bool,
) -> Result<Vec<SettlementPot>, ParserError> {
    let mut pots = Vec::new();

    for (pot, contenders, build) in pot_builds {
        let (candidate_allocations, issues) = match build {
            PotOptionBuild::Ready(options) => (
                options
                    .iter()
                    .map(|option| option_to_allocation(option, seat_by_player))
                    .collect::<Result<Vec<_>, _>>()?,
                Vec::new(),
            ),
            PotOptionBuild::Uncertain(issue) => (Vec::new(), vec![issue]),
        };

        let selected_allocation = if allow_selected {
            selected_allocations
                .get(&pot.pot_no)
                .map(|option| option_to_allocation(option, seat_by_player))
                .transpose()?
        } else {
            None
        };

        pots.push(SettlementPot {
            pot_no: pot.pot_no,
            amount: pot.amount,
            is_main: pot.is_main,
            contributions: pot.contributions.clone(),
            eligibilities: pot.eligibilities.clone(),
            contenders,
            candidate_allocations,
            selected_allocation,
            issues,
        });
    }

    Ok(pots)
}

fn option_to_allocation(
    option: &PotSettlementOption,
    seat_by_player: &BTreeMap<String, u8>,
) -> Result<SettlementAllocation, ParserError> {
    Ok(SettlementAllocation {
        source: option.source,
        shares: option
            .shares
            .iter()
            .map(|(player_name, share_amount)| {
                let seat_no = seat_by_player.get(player_name).copied().ok_or_else(|| {
                    ParserError::InvalidField {
                        field: "collect_player_missing_seat",
                        value: player_name.clone(),
                    }
                })?;
                Ok(SettlementShare {
                    seat_no,
                    player_name: player_name.clone(),
                    share_amount: *share_amount,
                })
            })
            .collect::<Result<Vec<_>, ParserError>>()?,
    })
}

fn build_settlement_evidence(
    hand: &CanonicalParsedHand,
    seat_by_player: &BTreeMap<String, u8>,
) -> Result<HandSettlementEvidence, ParserError> {
    let collect_events_seen = hand
        .actions
        .iter()
        .filter(|event| event.action_type == crate::models::ActionType::Collect)
        .filter_map(|event| {
            let player_name = event.player_name.as_ref()?;
            let seat_no = seat_by_player.get(player_name).copied()?;
            Some(SettlementCollectEvent {
                seq: event.seq,
                street: event.street,
                seat_no,
                player_name: player_name.clone(),
                amount: event.amount.unwrap_or(0),
            })
        })
        .collect::<Vec<_>>();

    let summary_outcomes_seen = hand
        .summary_seat_outcomes
        .iter()
        .map(|outcome| SettlementSummaryOutcome {
            seat_no: outcome.seat_no,
            player_name: outcome.player_name.clone(),
            position_marker: outcome.position_marker,
            outcome_kind: outcome.outcome_kind,
            folded_at: outcome.folded_at,
            shown_cards: outcome.shown_cards.clone(),
            won_amount: outcome.won_amount,
            hand_class: outcome.hand_class.clone(),
        })
        .collect::<Vec<_>>();

    let show_hands_seen = hand
        .actions
        .iter()
        .filter(|event| event.action_type == crate::models::ActionType::Show)
        .filter_map(|event| {
            let player_name = event.player_name.as_ref()?;
            let seat_no = seat_by_player.get(player_name).copied()?;
            Some(SettlementShowHand {
                seq: event.seq,
                street: event.street,
                seat_no,
                player_name: player_name.clone(),
                cards: event.cards.clone().unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();

    Ok(HandSettlementEvidence {
        collect_events_seen,
        summary_outcomes_seen,
        show_hands_seen,
    })
}

fn build_pot_options(
    hand: &CanonicalParsedHand,
    pot: &ConstructedPot,
    total_pots: usize,
    positive_collections: &BTreeMap<String, i64>,
    single_collector: Option<&str>,
    showdown_ranks: &BTreeMap<String, i64>,
    contenders: &[String],
) -> PotOptionBuild {
    if let Some(options) =
        build_showdown_options(pot.pot_no, pot.amount, contenders, showdown_ranks)
    {
        return PotOptionBuild::Ready(options);
    }

    if contenders.len() == 1 {
        return PotOptionBuild::Ready(vec![PotSettlementOption {
            pot_no: pot.pot_no,
            source: SettlementAllocationSource::SingleContender,
            shares: vec![(contenders[0].clone(), pot.amount)],
        }]);
    }

    if total_pots == 1 {
        let eligible_collects = positive_collections
            .iter()
            .filter(|(player, _)| contenders.contains(player))
            .map(|(player, amount)| (player.clone(), *amount))
            .collect::<Vec<_>>();
        let collected_total = eligible_collects
            .iter()
            .map(|(_, amount)| *amount)
            .sum::<i64>();
        if collected_total == pot.amount && !eligible_collects.is_empty() {
            return PotOptionBuild::Ready(vec![PotSettlementOption {
                pot_no: pot.pot_no,
                source: SettlementAllocationSource::SinglePotCollectedAmounts,
                shares: eligible_collects,
            }]);
        }
    }

    if let Some(player_name) = single_collector
        && contenders.iter().any(|player| player == player_name)
    {
        return PotOptionBuild::Ready(vec![PotSettlementOption {
            pot_no: pot.pot_no,
            source: SettlementAllocationSource::SingleCollectorFallback,
            shares: vec![(player_name.to_string(), pot.amount)],
        }]);
    }

    if hand.parse_issues.iter().any(|issue| {
        matches!(
            issue.code,
            ParseIssueCode::PartialRevealShowLine | ParseIssueCode::PartialRevealSummaryShowSurface
        )
    }) {
        PotOptionBuild::Uncertain(PotSettlementIssue::AmbiguousPartialReveal {
            eligible_players: contenders.to_vec(),
        })
    } else {
        PotOptionBuild::Uncertain(PotSettlementIssue::AmbiguousHiddenShowdown {
            eligible_players: contenders.to_vec(),
        })
    }
}

fn build_showdown_options(
    pot_no: u8,
    amount: i64,
    contenders: &[String],
    showdown_ranks: &BTreeMap<String, i64>,
) -> Option<Vec<PotSettlementOption>> {
    let eligible_ranks = contenders
        .iter()
        .map(|player| {
            showdown_ranks
                .get(player)
                .copied()
                .map(|rank| (player.clone(), rank))
        })
        .collect::<Option<Vec<_>>>()?;
    if eligible_ranks.is_empty() {
        return None;
    }

    let top_rank = eligible_ranks.iter().map(|(_, rank)| *rank).max()?;
    let top_winners = eligible_ranks
        .into_iter()
        .filter(|(_, rank)| *rank == top_rank)
        .map(|(player, _)| player)
        .collect::<Vec<_>>();
    let winner_count = top_winners.len() as i64;
    let base_share = amount / winner_count;
    let remainder = (amount % winner_count) as usize;

    if remainder == 0 {
        return Some(vec![PotSettlementOption {
            pot_no,
            source: SettlementAllocationSource::ShowdownRank,
            shares: top_winners
                .into_iter()
                .map(|player| (player, base_share))
                .collect(),
        }]);
    }

    let mut options = Vec::new();
    for bonus_receivers in combinations(&top_winners, remainder) {
        let receiver_set = bonus_receivers.into_iter().collect::<BTreeSet<_>>();
        options.push(PotSettlementOption {
            pot_no,
            source: SettlementAllocationSource::ShowdownRank,
            shares: top_winners
                .iter()
                .map(|player| {
                    let bonus = if receiver_set.contains(player) { 1 } else { 0 };
                    (player.clone(), base_share + bonus)
                })
                .collect(),
        });
    }
    options.sort();
    options.dedup();
    Some(options)
}

fn pot_contenders(hand: &CanonicalParsedHand, eligible_players: &[String]) -> Vec<String> {
    let definite_non_winners = hand
        .summary_seat_outcomes
        .iter()
        .filter(|outcome| {
            matches!(
                outcome.outcome_kind,
                SummarySeatOutcomeKind::Folded
                    | SummarySeatOutcomeKind::ShowedLost
                    | SummarySeatOutcomeKind::Lost
                    | SummarySeatOutcomeKind::Mucked
            )
        })
        .map(|outcome| outcome.player_name.as_str())
        .collect::<BTreeSet<_>>();

    let filtered = eligible_players
        .iter()
        .filter(|player| !definite_non_winners.contains(player.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        eligible_players.to_vec()
    } else {
        filtered
    }
}

fn showdown_rank_map(hand: &CanonicalParsedHand) -> Result<BTreeMap<String, i64>, ParserError> {
    evaluate_river_showdown_ranks(hand)
}

fn search_settlement_combinations(
    pots: &[PotOptions],
    index: usize,
    remaining: &mut BTreeMap<String, i64>,
    current: &mut Vec<PotSettlementOption>,
    solutions: &mut Vec<Vec<PotSettlementOption>>,
    limit: usize,
) {
    if solutions.len() >= limit {
        return;
    }

    if index == pots.len() {
        if remaining.values().all(|amount| *amount == 0) {
            solutions.push(current.clone());
        }
        return;
    }

    let pot = &pots[index];
    for option in &pot.options {
        if option.pot_no != pot.pot_no || !apply_shares(remaining, &option.shares) {
            continue;
        }

        current.push(option.clone());
        search_settlement_combinations(pots, index + 1, remaining, current, solutions, limit);
        current.pop();
        revert_shares(remaining, &option.shares);

        if solutions.len() >= limit {
            return;
        }
    }
}

fn apply_shares(remaining: &mut BTreeMap<String, i64>, shares: &[(String, i64)]) -> bool {
    if !shares.iter().all(|(player, share)| {
        *share > 0 && remaining.get(player.as_str()).copied().unwrap_or(0) >= *share
    }) {
        return false;
    }

    for (player, share) in shares {
        *remaining.entry(player.clone()).or_default() -= *share;
    }
    true
}

fn revert_shares(remaining: &mut BTreeMap<String, i64>, shares: &[(String, i64)]) {
    for (player, share) in shares {
        *remaining.entry(player.clone()).or_default() += *share;
    }
}

fn combinations(items: &[String], k: usize) -> Vec<Vec<String>> {
    if k == 0 {
        return vec![Vec::new()];
    }
    if items.len() < k {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut current = Vec::new();
    combinations_recursive(items, k, 0, &mut current, &mut result);
    result
}

fn combinations_recursive(
    items: &[String],
    k: usize,
    start: usize,
    current: &mut Vec<String>,
    result: &mut Vec<Vec<String>>,
) {
    if current.len() == k {
        result.push(current.clone());
        return;
    }

    for index in start..items.len() {
        current.push(items[index].clone());
        combinations_recursive(items, k, index + 1, current, result);
        current.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        parsers::hand_history::parse_canonical_hand, street_strength::evaluate_river_showdown_ranks,
    };

    #[test]
    fn showdown_rank_map_uses_river_showdown_kernel_and_keeps_summary_reveal_semantics() {
        let hand = parse_canonical_hand(&showdown_hand_with_summary_reveal()).unwrap();

        let actual = showdown_rank_map(&hand).unwrap();
        let expected = evaluate_river_showdown_ranks(&hand).unwrap();

        assert_eq!(actual, expected);
    }

    #[allow(clippy::too_many_arguments)]
    fn showdown_hand_with_summary_reveal() -> String {
        let flop = ["Kd", "7h", "2h"];
        let turn = Some("4c");
        let river = Some("3s");
        let turn_line = turn
            .map(|card| {
                format!(
                    "*** TURN *** [{} {} {}] [{}]\nVillain: checks\nHero: checks\n",
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

        format!(
            "Poker Hand #BRSTRPARTIALSUMMARY: Tournament #999001, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:00:00\n\
Table '1' 9-max Seat #1 is the button\n\
Seat 1: Villain (1,000 in chips)\n\
Seat 2: Hero (1,000 in chips)\n\
Villain: posts small blind 50\n\
Hero: posts big blind 100\n\
*** HOLE CARDS ***\n\
Dealt to Hero [Ah Kh]\n\
Villain: calls 50\n\
Hero: checks\n\
*** FLOP *** [{flop0} {flop1} {flop2}]\n\
Villain: checks\n\
Hero: checks\n\
{turn_line}\
{river_line}\
*** SHOWDOWN ***\n\
Villain: shows [Qc]\n\
Hero: shows [Ah Kh]\n\
Hero collected 200 from pot\n\
*** SUMMARY ***\n\
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0\n\
Board [Kd 7h 2h 4c 3s]\n\
Seat 1: Villain (small blind) showed [Qc Qd] and lost with a pair of Queens\n\
Seat 2: Hero (big blind) showed [Ah Kh] and collected (200) with a pair of Kings",
            flop0 = flop[0],
            flop1 = flop[1],
            flop2 = flop[2],
        )
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;

    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
    use std::{cell::RefCell, collections::BTreeMap};

    #[test]
    fn deterministic_generated_pot_scenarios_hold_constructed_pot_contract() {
        for scenario in generated_constructed_pot_scenarios(128) {
            assert_constructed_pot_contract(&scenario);
        }
    }

    proptest! {
        #[test]
        fn generated_pot_scenarios_keep_eligibility_inside_contributors(
            scenario in constructed_pot_scenario_strategy()
        ) {
            assert_constructed_pot_contract(&scenario);
        }
    }

    #[test]
    #[ignore = "stress suite for 10k+ generated pot math scenarios"]
    fn stress_generated_pot_scenarios_hold_constructed_pot_contract_for_10k_cases() {
        for scenario in generated_constructed_pot_scenarios(10_000) {
            assert_constructed_pot_contract(&scenario);
        }
    }

    #[derive(Debug, Clone)]
    struct ConstructedPotScenario {
        ordered_seats: Vec<ParsedHandSeat>,
        committed_total: BTreeMap<String, i64>,
        status: BTreeMap<String, PlayerStatus>,
    }

    fn constructed_pot_scenario_strategy() -> BoxedStrategy<ConstructedPotScenario> {
        (2_usize..=10)
            .prop_flat_map(|player_count| {
                (
                    prop::collection::vec(0_i64..=2_000, player_count),
                    prop::collection::vec(
                        prop_oneof![
                            Just(PlayerStatus::Live),
                            Just(PlayerStatus::Folded),
                            Just(PlayerStatus::AllIn),
                        ],
                        player_count,
                    ),
                )
                    .prop_map(move |(mut commitments, statuses)| {
                        if commitments.iter().all(|amount| *amount == 0) {
                            commitments[0] = 1;
                        }

                        let mut ordered_seats = Vec::with_capacity(player_count);
                        let mut committed_total = BTreeMap::new();
                        let mut status = BTreeMap::new();

                        for index in 0..player_count {
                            let player_name = format!("P{}", index + 1);
                            let committed = commitments[index];
                            ordered_seats.push(ParsedHandSeat {
                                seat_no: (index + 1) as u8,
                                player_name: player_name.clone(),
                                starting_stack: committed + 100,
                                is_sitting_out: false,
                            });
                            committed_total.insert(player_name.clone(), committed);
                            status.insert(player_name, statuses[index].clone());
                        }

                        ConstructedPotScenario {
                            ordered_seats,
                            committed_total,
                            status,
                        }
                    })
            })
            .boxed()
    }

    fn generated_constructed_pot_scenarios(case_count: u32) -> Vec<ConstructedPotScenario> {
        let strategy = constructed_pot_scenario_strategy();
        let captured = RefCell::new(Vec::new());
        let mut runner = TestRunner::new_with_rng(
            Config::with_cases(case_count),
            TestRng::deterministic_rng(RngAlgorithm::default()),
        );

        runner
            .run(&strategy, |scenario| {
                captured.borrow_mut().push(scenario);
                Ok(())
            })
            .unwrap();

        captured.into_inner()
    }

    fn assert_constructed_pot_contract(scenario: &ConstructedPotScenario) {
        let pots = construct_pots(
            &scenario.ordered_seats,
            &scenario.committed_total,
            &scenario.status,
        );

        let total_pot_amount = pots.iter().map(|pot| pot.amount).sum::<i64>();
        let total_committed = scenario.committed_total.values().sum::<i64>();
        assert_eq!(total_pot_amount, total_committed);

        let mut committed_by_player_from_pots = BTreeMap::<String, i64>::new();

        for (index, pot) in pots.iter().enumerate() {
            assert_eq!(pot.pot_no as usize, index + 1);
            assert_eq!(pot.is_main, index == 0);
            assert!(pot.amount > 0);

            let contribution_sum = pot
                .contributions
                .iter()
                .map(|entry| entry.amount)
                .sum::<i64>();
            assert_eq!(contribution_sum, pot.amount);

            let contributed_players = pot
                .contributions
                .iter()
                .map(|entry| entry.player_name.clone())
                .collect::<std::collections::BTreeSet<_>>();
            let eligible_players = pot
                .eligibilities
                .iter()
                .map(|entry| entry.player_name.clone())
                .collect::<std::collections::BTreeSet<_>>();

            assert!(eligible_players.is_subset(&contributed_players));

            for contribution in &pot.contributions {
                assert!(contribution.amount > 0);
                *committed_by_player_from_pots
                    .entry(contribution.player_name.clone())
                    .or_default() += contribution.amount;

                let player_status = scenario.status[&contribution.player_name].clone();
                let player_is_eligible = eligible_players.contains(&contribution.player_name);

                if player_status == PlayerStatus::Folded {
                    assert!(!player_is_eligible);
                } else {
                    assert!(player_is_eligible);
                }
            }
        }

        for seat in &scenario.ordered_seats {
            assert_eq!(
                committed_by_player_from_pots
                    .get(&seat.player_name)
                    .copied()
                    .unwrap_or(0),
                scenario
                    .committed_total
                    .get(&seat.player_name)
                    .copied()
                    .unwrap_or(0)
            );
        }
    }
}
