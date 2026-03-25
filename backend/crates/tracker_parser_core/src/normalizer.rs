use std::collections::BTreeMap;

use crate::{
    ParserError,
    models::{
        ActionType, CanonicalParsedHand, CertaintyState, FinalPot, HandElimination,
        HandOutcomeActual, HandReturn, NormalizationInvariants, NormalizedHand, ParsedHandSeat,
        PlayerNodeState, PlayerStatus, PotContribution, PotSlice, PotWinner,
        ResolutionNodeSnapshot, Street,
    },
};

#[derive(Debug, Clone)]
struct ReplayState {
    ordered_seats: Vec<ParsedHandSeat>,
    player_order: Vec<String>,
    starting_stack: BTreeMap<String, i64>,
    stack_current: BTreeMap<String, i64>,
    committed_total: BTreeMap<String, i64>,
    committed_by_street: BTreeMap<String, BTreeMap<String, i64>>,
    betting_round_contrib: BTreeMap<String, i64>,
    status: BTreeMap<String, PlayerStatus>,
    current_street: Street,
    snapshot: Option<ResolutionNodeSnapshot>,
}

#[derive(Debug, Clone)]
struct ResolvedPotState {
    pot_no: u8,
    amount: i64,
    is_main: bool,
    eligible_players: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PotAllocation {
    pot_no: u8,
    shares: Vec<(String, i64)>,
}

struct SnapshotBuildContext<'a> {
    ordered_seats: &'a [ParsedHandSeat],
    stack_current: &'a BTreeMap<String, i64>,
    committed_total: &'a BTreeMap<String, i64>,
    committed_by_street: &'a BTreeMap<String, BTreeMap<String, i64>>,
    status: &'a BTreeMap<String, PlayerStatus>,
}

pub fn normalize_hand(hand: &CanonicalParsedHand) -> Result<NormalizedHand, ParserError> {
    let hero_name = hand
        .hero_name
        .clone()
        .ok_or(ParserError::MissingLine("hero_name"))?;
    let warnings = hand.parse_warnings.clone();

    let ordered_seats = {
        let mut seats = hand.seats.clone();
        seats.sort_by_key(|seat| seat.seat_no);
        seats
    };
    let player_order = ordered_seats
        .iter()
        .map(|seat| seat.player_name.clone())
        .collect::<Vec<_>>();
    let seat_by_player = ordered_seats
        .iter()
        .map(|seat| (seat.player_name.clone(), seat.seat_no))
        .collect::<BTreeMap<_, _>>();

    let mut replay = ReplayState::new(ordered_seats.clone());
    for event in &hand.actions {
        replay.apply_event(hand, &hero_name, event)?;
    }

    let final_board_cards = if hand.board_final.is_empty() && !hand.summary_board.is_empty() {
        hand.summary_board.clone()
    } else {
        hand.board_final.clone()
    };
    let rake_amount = hand.summary_rake_amount.unwrap_or(0);
    let returns = build_returns(hand, &seat_by_player)?;
    let (resolved_pots, pot_contributions) =
        build_resolved_pots(&ordered_seats, &replay.committed_total, &replay.status);
    let final_pots = resolved_pots
        .iter()
        .map(|pot| FinalPot {
            pot_no: pot.pot_no,
            amount: pot.amount,
            is_main: pot.is_main,
        })
        .collect::<Vec<_>>();
    let (pot_winners, winner_mapping_state, winner_mapping_errors) =
        resolve_pot_winners(&resolved_pots, &hand.collected_amounts, &seat_by_player)?;

    let stacks_after_actual = player_order
        .iter()
        .map(|player| {
            let final_stack = replay.starting_stack[player] - replay.committed_total[player]
                + hand.collected_amounts.get(player).copied().unwrap_or(0);
            (player.clone(), final_stack)
        })
        .collect::<BTreeMap<_, _>>();

    let total_committed = replay.committed_total.values().sum::<i64>();
    let total_collected = hand.collected_amounts.values().sum::<i64>();
    let mut invariant_errors = winner_mapping_errors;

    let eliminations = ordered_seats
        .iter()
        .filter_map(|seat| {
            let final_stack = stacks_after_actual
                .get(&seat.player_name)
                .copied()
                .unwrap_or(0);
            (seat.starting_stack > 0 && final_stack == 0).then(|| {
                build_elimination(
                    hand,
                    seat,
                    &pot_contributions,
                    &final_pots,
                    &pot_winners,
                    winner_mapping_state,
                )
            })
        })
        .collect::<Vec<_>>();

    let starting_sum = replay.starting_stack.values().sum::<i64>();
    let final_sum = stacks_after_actual.values().sum::<i64>();
    let chip_conservation_ok = starting_sum == final_sum;
    if !chip_conservation_ok {
        invariant_errors.push(format!(
            "chip_conservation_mismatch: starting_sum={starting_sum}, final_sum={final_sum}"
        ));
    }

    let pot_conservation_ok = total_committed == total_collected + rake_amount;
    if !pot_conservation_ok {
        invariant_errors.push(format!(
            "pot_conservation_mismatch: committed_total={total_committed}, collected_total={total_collected}, rake_amount={rake_amount}"
        ));
    }
    if let Some(summary_total_pot) = hand.summary_total_pot
        && summary_total_pot != total_collected + rake_amount
    {
        invariant_errors.push(format!(
            "summary_total_pot_mismatch: summary_total_pot={summary_total_pot}, collected_plus_rake={}",
            total_collected + rake_amount
        ));
    }

    Ok(NormalizedHand {
        hand_id: hand.header.hand_id.clone(),
        player_order,
        snapshot: replay.snapshot,
        final_pots,
        pot_contributions,
        pot_winners,
        returns,
        actual: HandOutcomeActual {
            committed_total_by_player: replay.committed_total,
            stacks_after_actual,
            winner_collections: hand.collected_amounts.clone(),
            final_board_cards,
            rake_amount,
        },
        eliminations,
        invariants: NormalizationInvariants {
            chip_conservation_ok,
            pot_conservation_ok,
            invariant_errors,
        },
        warnings,
    })
}

impl ReplayState {
    fn new(ordered_seats: Vec<ParsedHandSeat>) -> Self {
        let player_order = ordered_seats
            .iter()
            .map(|seat| seat.player_name.clone())
            .collect::<Vec<_>>();
        let starting_stack = ordered_seats
            .iter()
            .map(|seat| (seat.player_name.clone(), seat.starting_stack))
            .collect::<BTreeMap<_, _>>();
        let committed_total = player_order
            .iter()
            .map(|player| (player.clone(), 0_i64))
            .collect::<BTreeMap<_, _>>();
        let committed_by_street = player_order
            .iter()
            .map(|player| (player.clone(), empty_committed_by_street()))
            .collect::<BTreeMap<_, _>>();
        let betting_round_contrib = player_order
            .iter()
            .map(|player| (player.clone(), 0_i64))
            .collect::<BTreeMap<_, _>>();
        let status = player_order
            .iter()
            .map(|player| {
                let player_status = if starting_stack[player] > 0 {
                    PlayerStatus::Live
                } else {
                    PlayerStatus::Eliminated
                };
                (player.clone(), player_status)
            })
            .collect::<BTreeMap<_, _>>();

        Self {
            ordered_seats,
            player_order,
            starting_stack: starting_stack.clone(),
            stack_current: starting_stack,
            committed_total,
            committed_by_street,
            betting_round_contrib,
            status,
            current_street: Street::Preflop,
            snapshot: None,
        }
    }

    fn apply_event(
        &mut self,
        hand: &CanonicalParsedHand,
        hero_name: &str,
        event: &crate::models::HandActionEvent,
    ) -> Result<(), ParserError> {
        self.advance_street_if_needed(event.street);

        let Some(player_name) = event.player_name.as_ref() else {
            return Ok(());
        };

        let mut delta = 0_i64;
        let mut contributes_to_betting_round = false;

        match event.action_type {
            ActionType::PostAnte => {
                delta = event.amount.unwrap_or(0);
            }
            ActionType::PostSb | ActionType::PostBb | ActionType::PostDead => {
                delta = event.amount.unwrap_or(0);
                contributes_to_betting_round = true;
            }
            ActionType::Fold => {
                if self.status[player_name] != PlayerStatus::Eliminated {
                    self.status
                        .insert(player_name.clone(), PlayerStatus::Folded);
                }
            }
            ActionType::Check => {}
            ActionType::Call => {
                delta = event.amount.unwrap_or(0);
                contributes_to_betting_round = true;
            }
            ActionType::Bet => {
                delta = event.amount.unwrap_or(0);
                contributes_to_betting_round = true;
            }
            ActionType::RaiseTo => {
                let to_amount = event.amount_from_to_amount()?;
                let already_contributed = self.betting_round_contrib[player_name];
                delta = (to_amount - already_contributed).max(0);
                contributes_to_betting_round = true;
            }
            ActionType::ReturnUncalled => {
                let refund = event.amount.unwrap_or(0);
                *self.stack_current.entry(player_name.clone()).or_default() += refund;
                *self.committed_total.entry(player_name.clone()).or_default() -= refund;
                *self
                    .committed_by_street
                    .entry(player_name.clone())
                    .or_insert_with(empty_committed_by_street)
                    .entry(street_key(self.current_street).to_string())
                    .or_default() -= refund;
                *self
                    .betting_round_contrib
                    .entry(player_name.clone())
                    .or_default() -= refund;
            }
            ActionType::Collect | ActionType::Show | ActionType::Muck => {}
        }

        if delta > 0 {
            *self.stack_current.entry(player_name.clone()).or_default() -= delta;
            *self.committed_total.entry(player_name.clone()).or_default() += delta;
            *self
                .committed_by_street
                .entry(player_name.clone())
                .or_insert_with(empty_committed_by_street)
                .entry(street_key(self.current_street).to_string())
                .or_default() += delta;
            if contributes_to_betting_round {
                *self
                    .betting_round_contrib
                    .entry(player_name.clone())
                    .or_default() += delta;
            }
        }

        self.update_player_status(player_name, event);

        if self.snapshot.is_none() && self.should_capture_snapshot(event) {
            self.snapshot = Some(build_snapshot(
                hand,
                hero_name,
                event.seq,
                self.current_street,
                SnapshotBuildContext {
                    ordered_seats: &self.ordered_seats,
                    stack_current: &self.stack_current,
                    committed_total: &self.committed_total,
                    committed_by_street: &self.committed_by_street,
                    status: &self.status,
                },
            ));
        }

        Ok(())
    }

    fn advance_street_if_needed(&mut self, event_street: Street) {
        if matches!(event_street, Street::Showdown | Street::Summary) {
            self.current_street = event_street;
            return;
        }

        if event_street != self.current_street {
            self.current_street = event_street;
            self.betting_round_contrib = self
                .player_order
                .iter()
                .map(|player| (player.clone(), 0_i64))
                .collect::<BTreeMap<_, _>>();
        }
    }

    fn update_player_status(&mut self, player_name: &str, event: &crate::models::HandActionEvent) {
        if self.status[player_name] == PlayerStatus::Folded
            || self.status[player_name] == PlayerStatus::Eliminated
        {
            return;
        }

        if event.is_all_in || self.stack_current[player_name] == 0 {
            self.status
                .insert(player_name.to_string(), PlayerStatus::AllIn);
        } else {
            self.status
                .insert(player_name.to_string(), PlayerStatus::Live);
        }
    }

    fn should_capture_snapshot(&self, event: &crate::models::HandActionEvent) -> bool {
        let contestants = self
            .player_order
            .iter()
            .filter(|player| {
                matches!(
                    self.status[player.as_str()],
                    PlayerStatus::Live | PlayerStatus::AllIn
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let all_in_count = contestants
            .iter()
            .filter(|player| self.status[player.as_str()] == PlayerStatus::AllIn)
            .count();
        let live_count = contestants
            .iter()
            .filter(|player| self.status[player.as_str()] == PlayerStatus::Live)
            .count();

        let betting_closed_with_single_live = all_in_count >= 1
            && live_count == 1
            && matches!(event.action_type, ActionType::Call | ActionType::Check);
        let all_contestants_all_in = contestants.len() >= 2 && all_in_count == contestants.len();

        contestants.len() >= 2 && (all_contestants_all_in || betting_closed_with_single_live)
    }
}

fn build_returns(
    hand: &CanonicalParsedHand,
    seat_by_player: &BTreeMap<String, u8>,
) -> Result<Vec<HandReturn>, ParserError> {
    hand.actions
        .iter()
        .filter(|event| event.action_type == ActionType::ReturnUncalled)
        .map(|event| {
            let player_name = event
                .player_name
                .clone()
                .ok_or(ParserError::MissingLine("return player_name"))?;
            let seat_no = seat_by_player.get(&player_name).copied().ok_or_else(|| {
                ParserError::InvalidField {
                    field: "return_player_name",
                    value: player_name.clone(),
                }
            })?;
            Ok(HandReturn {
                seat_no,
                player_name,
                amount: event.amount.unwrap_or(0),
                reason: "uncalled".to_string(),
            })
        })
        .collect()
}

fn build_resolved_pots(
    ordered_seats: &[ParsedHandSeat],
    committed_total: &BTreeMap<String, i64>,
    status: &BTreeMap<String, PlayerStatus>,
) -> (Vec<ResolvedPotState>, Vec<PotContribution>) {
    let mut levels = committed_total
        .values()
        .copied()
        .filter(|amount| *amount > 0)
        .collect::<Vec<_>>();
    levels.sort_unstable();
    levels.dedup();

    let mut pots = Vec::new();
    let mut contributions = Vec::new();
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
        let eligible_players = contributors
            .iter()
            .filter(|seat| status[seat.player_name.as_str()] != PlayerStatus::Folded)
            .map(|seat| seat.player_name.clone())
            .collect::<Vec<_>>();

        pots.push(ResolvedPotState {
            pot_no,
            amount,
            is_main: pots.is_empty(),
            eligible_players,
        });
        contributions.extend(contributors.into_iter().map(|seat| PotContribution {
            pot_no,
            seat_no: seat.seat_no,
            player_name: seat.player_name.clone(),
            amount: increment,
        }));
        previous_level = level;
    }

    (pots, contributions)
}

fn resolve_pot_winners(
    pots: &[ResolvedPotState],
    winner_collections: &BTreeMap<String, i64>,
    seat_by_player: &BTreeMap<String, u8>,
) -> Result<(Vec<PotWinner>, CertaintyState, Vec<String>), ParserError> {
    if pots.is_empty() {
        let has_collections = winner_collections.values().any(|amount| *amount > 0);
        return Ok((
            Vec::new(),
            if has_collections {
                CertaintyState::Inconsistent
            } else {
                CertaintyState::Exact
            },
            if has_collections {
                vec!["collect_events_without_pots".to_string()]
            } else {
                Vec::new()
            },
        ));
    }

    let mut remaining = winner_collections
        .iter()
        .filter(|(_, amount)| **amount > 0)
        .map(|(player, amount)| (player.clone(), *amount))
        .collect::<BTreeMap<_, _>>();

    if remaining.is_empty() {
        return Ok((
            Vec::new(),
            CertaintyState::Inconsistent,
            vec!["pot_winners_missing_collections".to_string()],
        ));
    }

    let mut pot_order = pots.to_vec();
    pot_order.sort_by(|left, right| {
        right
            .amount
            .cmp(&left.amount)
            .then(
                left.eligible_players
                    .len()
                    .cmp(&right.eligible_players.len()),
            )
            .then(left.pot_no.cmp(&right.pot_no))
    });

    let mut current = Vec::new();
    let mut solutions = Vec::new();
    search_pot_allocations(
        &pot_order,
        0,
        &mut remaining,
        &mut current,
        &mut solutions,
        2,
    );

    let Some(first_solution) = solutions.first().cloned() else {
        return Ok((
            Vec::new(),
            CertaintyState::Inconsistent,
            vec!["collect_mapping_unsatisfied".to_string()],
        ));
    };

    if solutions.len() == 1 {
        return Ok((
            allocations_to_pot_winners(&first_solution, seat_by_player)?,
            CertaintyState::Exact,
            Vec::new(),
        ));
    }

    // When multiple valid mappings exist we keep the hand uncertain and avoid
    // materializing guessed winners into exact downstream tables.
    Ok((Vec::new(), CertaintyState::Uncertain, Vec::new()))
}

fn search_pot_allocations(
    pots: &[ResolvedPotState],
    index: usize,
    remaining: &mut BTreeMap<String, i64>,
    current: &mut Vec<PotAllocation>,
    solutions: &mut Vec<Vec<PotAllocation>>,
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
    let candidates = candidate_allocations_for_pot(pot, remaining);
    for shares in candidates {
        if !apply_allocation(remaining, &shares) {
            continue;
        }

        current.push(PotAllocation {
            pot_no: pot.pot_no,
            shares: shares.clone(),
        });
        search_pot_allocations(pots, index + 1, remaining, current, solutions, limit);
        current.pop();
        revert_allocation(remaining, &shares);

        if solutions.len() >= limit {
            return;
        }
    }
}

fn candidate_allocations_for_pot(
    pot: &ResolvedPotState,
    remaining: &BTreeMap<String, i64>,
) -> Vec<Vec<(String, i64)>> {
    let eligible_positive = pot
        .eligible_players
        .iter()
        .filter(|player| remaining.get(player.as_str()).copied().unwrap_or(0) > 0)
        .cloned()
        .collect::<Vec<_>>();

    if eligible_positive.is_empty() {
        return Vec::new();
    }

    let positive_total = eligible_positive
        .iter()
        .map(|player| remaining.get(player.as_str()).copied().unwrap_or(0))
        .sum::<i64>();
    if positive_total < pot.amount {
        return Vec::new();
    }

    let mut candidates = Vec::new();

    for player in &eligible_positive {
        if remaining.get(player.as_str()).copied().unwrap_or(0) >= pot.amount {
            candidates.push(vec![(player.clone(), pot.amount)]);
        }
    }

    for winner_count in 2..=eligible_positive.len() {
        if pot.amount < winner_count as i64 {
            break;
        }

        let base_share = pot.amount / winner_count as i64;
        if base_share == 0 {
            continue;
        }

        let remainder = (pot.amount % winner_count as i64) as usize;
        for subset in combinations(&eligible_positive, winner_count) {
            if remainder == 0 {
                let shares = subset
                    .iter()
                    .map(|player| (player.clone(), base_share))
                    .collect::<Vec<_>>();
                if allocation_fits(remaining, &shares) {
                    candidates.push(shares);
                }
                continue;
            }

            for bonus_receivers in combinations(&subset, remainder) {
                let shares = subset
                    .iter()
                    .map(|player| {
                        let bonus = if bonus_receivers.contains(player) {
                            1
                        } else {
                            0
                        };
                        (player.clone(), base_share + bonus)
                    })
                    .collect::<Vec<_>>();
                if allocation_fits(remaining, &shares) {
                    candidates.push(shares);
                }
            }
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

fn allocation_fits(remaining: &BTreeMap<String, i64>, shares: &[(String, i64)]) -> bool {
    shares.iter().all(|(player, share)| {
        *share > 0 && remaining.get(player.as_str()).copied().unwrap_or(0) >= *share
    })
}

fn apply_allocation(remaining: &mut BTreeMap<String, i64>, shares: &[(String, i64)]) -> bool {
    if !allocation_fits(remaining, shares) {
        return false;
    }

    for (player, share) in shares {
        *remaining.entry(player.clone()).or_default() -= *share;
    }
    true
}

fn revert_allocation(remaining: &mut BTreeMap<String, i64>, shares: &[(String, i64)]) {
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

fn allocations_to_pot_winners(
    allocations: &[PotAllocation],
    seat_by_player: &BTreeMap<String, u8>,
) -> Result<Vec<PotWinner>, ParserError> {
    let mut winners = Vec::new();
    let mut ordered_allocations = allocations.to_vec();
    ordered_allocations.sort_by_key(|allocation| allocation.pot_no);

    for allocation in ordered_allocations {
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

fn build_elimination(
    hand: &CanonicalParsedHand,
    seat: &ParsedHandSeat,
    pot_contributions: &[PotContribution],
    final_pots: &[FinalPot],
    pot_winners: &[PotWinner],
    winner_mapping_state: CertaintyState,
) -> HandElimination {
    let resolved_by_pot_no = pot_contributions
        .iter()
        .filter(|contribution| contribution.seat_no == seat.seat_no)
        .map(|contribution| contribution.pot_no)
        .max();

    let Some(pot_no) = resolved_by_pot_no else {
        return HandElimination {
            eliminated_seat_no: seat.seat_no,
            eliminated_player_name: seat.player_name.clone(),
            resolved_by_pot_no: None,
            ko_involved_winner_count: 0,
            hero_involved: false,
            hero_share_fraction: None,
            is_split_ko: false,
            split_n: None,
            is_sidepot_based: false,
            certainty_state: CertaintyState::Uncertain,
        };
    };

    let resolved_winners = pot_winners
        .iter()
        .filter(|winner| winner.pot_no == pot_no)
        .collect::<Vec<_>>();
    let resolved_pot_amount = final_pots
        .iter()
        .find(|pot| pot.pot_no == pot_no)
        .map(|pot| pot.amount)
        .unwrap_or(0);
    let hero_share = resolved_winners
        .iter()
        .filter(|winner| winner.player_name == hand.hero_name.as_deref().unwrap_or_default())
        .map(|winner| winner.share_amount)
        .sum::<i64>();
    let share_fraction = if winner_mapping_state == CertaintyState::Exact && resolved_pot_amount > 0
    {
        Some(hero_share as f64 / resolved_pot_amount as f64)
    } else {
        None
    };
    let split_n = (!resolved_winners.is_empty()).then_some(resolved_winners.len() as u8);

    HandElimination {
        eliminated_seat_no: seat.seat_no,
        eliminated_player_name: seat.player_name.clone(),
        resolved_by_pot_no: Some(pot_no),
        ko_involved_winner_count: resolved_winners.len() as u8,
        hero_involved: hero_share > 0,
        hero_share_fraction: share_fraction,
        is_split_ko: resolved_winners.len() > 1,
        split_n,
        is_sidepot_based: pot_no > 1,
        certainty_state: if resolved_winners.is_empty() {
            CertaintyState::Uncertain
        } else {
            winner_mapping_state
        },
    }
}

fn build_snapshot(
    hand: &CanonicalParsedHand,
    hero_name: &str,
    snapshot_event_seq: usize,
    street: Street,
    context: SnapshotBuildContext<'_>,
) -> ResolutionNodeSnapshot {
    let players = context
        .ordered_seats
        .iter()
        .map(|seat| {
            let hole_cards = if seat.player_name == hero_name {
                hand.hero_hole_cards.clone()
            } else {
                hand.showdown_hands.get(&seat.player_name).cloned()
            };

            PlayerNodeState {
                seat_no: seat.seat_no,
                player_name: seat.player_name.clone(),
                stack_before_hand: seat.starting_stack,
                stack_at_snapshot: context.stack_current[&seat.player_name],
                committed_total: context.committed_total[&seat.player_name],
                committed_by_street: context.committed_by_street[&seat.player_name].clone(),
                status: context.status[&seat.player_name].clone(),
                is_hero: seat.player_name == hero_name,
                hole_cards_known: hole_cards.is_some(),
                hole_cards,
            }
        })
        .collect::<Vec<_>>();

    let pots = build_snapshot_pots(&players);
    let known_board_cards = match street {
        Street::Preflop => Vec::new(),
        Street::Flop => hand.board_final.iter().take(3).cloned().collect(),
        Street::Turn => hand.board_final.iter().take(4).cloned().collect(),
        Street::River | Street::Showdown | Street::Summary => hand.board_final.clone(),
    };

    ResolutionNodeSnapshot {
        hand_id: hand.header.hand_id.clone(),
        snapshot_street: street,
        snapshot_event_seq,
        known_board_cards: known_board_cards.clone(),
        future_board_cards_count: (5_usize.saturating_sub(known_board_cards.len())) as u8,
        players,
        pots,
        hero_name: hero_name.to_string(),
        terminal_allin_node: true,
    }
}

fn build_snapshot_pots(players: &[PlayerNodeState]) -> Vec<PotSlice> {
    let mut levels = players
        .iter()
        .map(|player| player.committed_total)
        .filter(|amount| *amount > 0)
        .collect::<Vec<_>>();
    levels.sort_unstable();
    levels.dedup();

    let mut pots = Vec::new();
    let mut previous_level = 0_i64;

    for level in levels {
        let contributors = players
            .iter()
            .filter(|player| player.committed_total >= level)
            .collect::<Vec<_>>();
        if contributors.is_empty() {
            continue;
        }

        let increment = level - previous_level;
        let amount = increment * contributors.len() as i64;
        if amount <= 0 {
            previous_level = level;
            continue;
        }

        let eligible_players = contributors
            .iter()
            .filter(|player| player.status != PlayerStatus::Folded)
            .map(|player| player.player_name.clone())
            .collect::<Vec<_>>();

        pots.push(PotSlice {
            pot_index: pots.len(),
            amount,
            eligible_players,
            is_main: pots.is_empty(),
        });

        previous_level = level;
    }

    pots
}

fn empty_committed_by_street() -> BTreeMap<String, i64> {
    BTreeMap::from([
        ("preflop".to_string(), 0),
        ("flop".to_string(), 0),
        ("turn".to_string(), 0),
        ("river".to_string(), 0),
    ])
}

fn street_key(street: Street) -> &'static str {
    match street {
        Street::Preflop => "preflop",
        Street::Flop => "flop",
        Street::Turn => "turn",
        Street::River => "river",
        Street::Showdown => "river",
        Street::Summary => "river",
    }
}

trait RaiseAmountExt {
    fn amount_from_to_amount(&self) -> Result<i64, ParserError>;
}

impl RaiseAmountExt for crate::models::HandActionEvent {
    fn amount_from_to_amount(&self) -> Result<i64, ParserError> {
        self.to_amount.ok_or(ParserError::InvalidField {
            field: "to_amount",
            value: self.raw_line.clone(),
        })
    }
}
