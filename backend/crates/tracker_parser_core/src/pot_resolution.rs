use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ParserError,
    models::{
        CanonicalParsedHand, CertaintyState, FinalPot, ParsedHandSeat, PlayerStatus,
        PotContribution, PotEligibility, PotWinner, Street, SummarySeatOutcomeKind,
    },
    street_strength::evaluate_street_hand_strength,
};

#[derive(Debug, Clone)]
struct ConstructedPot {
    pot_no: u8,
    amount: i64,
    is_main: bool,
    eligible_players: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PotSettlementOption {
    pot_no: u8,
    shares: Vec<(String, i64)>,
}

#[derive(Debug, Clone)]
struct PotOptions {
    pot_no: u8,
    options: Vec<PotSettlementOption>,
}

enum PotOptionBuild {
    Ready(Vec<PotSettlementOption>),
    Uncertain(String),
}

pub(crate) struct PotResolutionOutcome {
    pub(crate) final_pots: Vec<FinalPot>,
    pub(crate) pot_contributions: Vec<PotContribution>,
    pub(crate) pot_eligibilities: Vec<PotEligibility>,
    pub(crate) pot_winners: Vec<PotWinner>,
    pub(crate) certainty_state: CertaintyState,
    pub(crate) invariant_errors: Vec<String>,
    pub(crate) uncertain_reason_codes: Vec<String>,
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
    let (pots, contributions, eligibilities) =
        construct_pots(ordered_seats, committed_total, status);
    let final_pots = pots
        .iter()
        .map(|pot| FinalPot {
            pot_no: pot.pot_no,
            amount: pot.amount,
            is_main: pot.is_main,
        })
        .collect::<Vec<_>>();

    let settlement = settle_pots(hand, &pots, &seat_by_player)?;

    Ok(PotResolutionOutcome {
        final_pots,
        pot_contributions: contributions,
        pot_eligibilities: eligibilities,
        pot_winners: settlement.pot_winners,
        certainty_state: settlement.certainty_state,
        invariant_errors: settlement.invariant_errors,
        uncertain_reason_codes: settlement.uncertain_reason_codes,
    })
}

struct SettlementResult {
    pot_winners: Vec<PotWinner>,
    certainty_state: CertaintyState,
    invariant_errors: Vec<String>,
    uncertain_reason_codes: Vec<String>,
}

fn construct_pots(
    ordered_seats: &[ParsedHandSeat],
    committed_total: &BTreeMap<String, i64>,
    status: &BTreeMap<String, PlayerStatus>,
) -> (
    Vec<ConstructedPot>,
    Vec<PotContribution>,
    Vec<PotEligibility>,
) {
    let mut levels = committed_total
        .values()
        .copied()
        .filter(|amount| *amount > 0)
        .collect::<Vec<_>>();
    levels.sort_unstable();
    levels.dedup();

    let mut pots = Vec::new();
    let mut contributions = Vec::new();
    let mut eligibilities = Vec::new();
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
        let eligible_players = eligible_seats
            .iter()
            .map(|seat| seat.player_name.clone())
            .collect::<Vec<_>>();

        pots.push(ConstructedPot {
            pot_no,
            amount,
            is_main: pots.is_empty(),
            eligible_players,
        });
        contributions.extend(contributors.iter().map(|seat| PotContribution {
            pot_no,
            seat_no: seat.seat_no,
            player_name: seat.player_name.clone(),
            amount: increment,
        }));
        eligibilities.extend(eligible_seats.into_iter().map(|seat| PotEligibility {
            pot_no,
            seat_no: seat.seat_no,
            player_name: seat.player_name.clone(),
        }));
        previous_level = level;
    }

    (pots, contributions, eligibilities)
}

fn settle_pots(
    hand: &CanonicalParsedHand,
    pots: &[ConstructedPot],
    seat_by_player: &BTreeMap<String, u8>,
) -> Result<SettlementResult, ParserError> {
    if pots.is_empty() {
        let has_collections = hand.collected_amounts.values().any(|amount| *amount > 0);
        return Ok(SettlementResult {
            pot_winners: Vec::new(),
            certainty_state: if has_collections {
                CertaintyState::Inconsistent
            } else {
                CertaintyState::Exact
            },
            invariant_errors: if has_collections {
                vec!["collect_events_without_pots".to_string()]
            } else {
                Vec::new()
            },
            uncertain_reason_codes: Vec::new(),
        });
    }

    let positive_collections = hand
        .collected_amounts
        .iter()
        .filter(|(_, amount)| **amount > 0)
        .map(|(player, amount)| (player.clone(), *amount))
        .collect::<BTreeMap<_, _>>();
    if positive_collections.is_empty() {
        return Ok(SettlementResult {
            pot_winners: Vec::new(),
            certainty_state: CertaintyState::Inconsistent,
            invariant_errors: vec!["pot_winners_missing_collections".to_string()],
            uncertain_reason_codes: Vec::new(),
        });
    }

    let showdown_ranks = showdown_rank_map(hand)?;
    let single_collector = (positive_collections.len() == 1)
        .then(|| positive_collections.keys().next().cloned())
        .flatten();
    let total_pots = pots.len();
    let mut pot_options = Vec::new();
    let mut uncertain_reason_codes = Vec::new();

    for pot in pots {
        match build_pot_options(
            hand,
            pot,
            total_pots,
            &positive_collections,
            single_collector.as_deref(),
            &showdown_ranks,
        ) {
            PotOptionBuild::Ready(options) => pot_options.push(PotOptions {
                pot_no: pot.pot_no,
                options,
            }),
            PotOptionBuild::Uncertain(reason_code) => uncertain_reason_codes.push(reason_code),
        }
    }

    if !uncertain_reason_codes.is_empty() {
        uncertain_reason_codes.sort();
        uncertain_reason_codes.dedup();
        return Ok(SettlementResult {
            pot_winners: Vec::new(),
            certainty_state: CertaintyState::Uncertain,
            invariant_errors: Vec::new(),
            uncertain_reason_codes,
        });
    }

    let mut remaining = positive_collections.clone();
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

    let Some(solution) = solutions.first().cloned() else {
        return Ok(SettlementResult {
            pot_winners: Vec::new(),
            certainty_state: CertaintyState::Inconsistent,
            invariant_errors: vec![
                "pot_settlement_collect_conflict: no_exact_settlement_matches_collected_amounts"
                    .to_string(),
            ],
            uncertain_reason_codes: Vec::new(),
        });
    };

    if solutions.len() > 1 {
        return Ok(SettlementResult {
            pot_winners: Vec::new(),
            certainty_state: CertaintyState::Uncertain,
            invariant_errors: Vec::new(),
            uncertain_reason_codes: vec!["pot_settlement_multiple_exact_allocations".to_string()],
        });
    }

    Ok(SettlementResult {
        pot_winners: allocations_to_pot_winners(&solution, seat_by_player)?,
        certainty_state: CertaintyState::Exact,
        invariant_errors: Vec::new(),
        uncertain_reason_codes: Vec::new(),
    })
}

fn build_pot_options(
    hand: &CanonicalParsedHand,
    pot: &ConstructedPot,
    total_pots: usize,
    positive_collections: &BTreeMap<String, i64>,
    single_collector: Option<&str>,
    showdown_ranks: &BTreeMap<String, i64>,
) -> PotOptionBuild {
    let contenders = pot_contenders(hand, &pot.eligible_players);

    if contenders.len() == 1 {
        return PotOptionBuild::Ready(vec![PotSettlementOption {
            pot_no: pot.pot_no,
            shares: vec![(contenders[0].clone(), pot.amount)],
        }]);
    }

    if let Some(options) =
        build_showdown_options(pot.pot_no, pot.amount, &contenders, showdown_ranks)
    {
        return PotOptionBuild::Ready(options);
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
                shares: eligible_collects,
            }]);
        }
    }

    if let Some(player_name) = single_collector
        && contenders.iter().any(|player| player == player_name)
    {
        return PotOptionBuild::Ready(vec![PotSettlementOption {
            pot_no: pot.pot_no,
            shares: vec![(player_name.to_string(), pot.amount)],
        }]);
    }

    let reason_prefix = if hand
        .parse_warnings
        .iter()
        .any(|warning| warning.starts_with("partial_reveal_"))
    {
        "pot_settlement_ambiguous_partial_reveal"
    } else {
        "pot_settlement_ambiguous_hidden_showdown"
    };
    PotOptionBuild::Uncertain(format!(
        "{reason_prefix}: pot_no={}, eligible_players={}",
        pot.pot_no,
        contenders.join("|")
    ))
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
    let mut settlement_hand = hand.clone();
    for outcome in &hand.summary_seat_outcomes {
        let summary_shows_cards = matches!(
            outcome.outcome_kind,
            SummarySeatOutcomeKind::ShowedWon | SummarySeatOutcomeKind::ShowedLost
        );
        if summary_shows_cards
            && let Some(cards) = &outcome.shown_cards
            && cards.len() == 2
        {
            settlement_hand
                .showdown_hands
                .entry(outcome.player_name.clone())
                .or_insert_with(|| cards.clone());
        }
    }

    let player_by_seat = hand
        .seats
        .iter()
        .map(|seat| (seat.seat_no, seat.player_name.clone()))
        .collect::<BTreeMap<_, _>>();

    Ok(evaluate_street_hand_strength(&settlement_hand)?
        .into_iter()
        .filter(|row| row.street == Street::River)
        .filter_map(|row| {
            player_by_seat
                .get(&row.seat_no)
                .map(|player| (player.clone(), row.best_hand_rank_value))
        })
        .collect())
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

fn allocations_to_pot_winners(
    allocations: &[PotSettlementOption],
    seat_by_player: &BTreeMap<String, u8>,
) -> Result<Vec<PotWinner>, ParserError> {
    let mut winners = Vec::new();
    let mut ordered = allocations.to_vec();
    ordered.sort_by_key(|allocation| allocation.pot_no);

    for allocation in ordered {
        for (player_name, share_amount) in allocation.shares {
            let seat_no = seat_by_player.get(&player_name).copied().ok_or_else(|| {
                ParserError::InvalidField {
                    field: "collect_player_missing_seat",
                    value: player_name.clone(),
                }
            })?;
            winners.push(PotWinner {
                pot_no: allocation.pot_no,
                seat_no,
                player_name,
                share_amount,
            });
        }
    }

    Ok(winners)
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
