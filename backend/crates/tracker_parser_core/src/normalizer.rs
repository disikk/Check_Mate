use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ParserError,
    betting_rules::evaluate_action_legality,
    money_state::{MoneyMutationFailure, apply_debit, apply_refund, validate_refund},
    models::{
        ActionType, CanonicalParsedHand, CertaintyState, HandElimination, HandEliminationKoShare,
        HandInvariants, HandOutcomeActual, HandReturn, HandSettlement, InvariantIssue,
        NormalizedHand, ParsedHandSeat, PlayerNodeState, PlayerStatus, PotSlice,
        ResolutionNodeSnapshot, SettlementIssue, Street,
    },
    pot_resolution::{observed_payouts, resolve_hand_pots},
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
    pending_live_actors: BTreeSet<String>,
    status: BTreeMap<String, PlayerStatus>,
    current_street: Street,
    snapshot: Option<ResolutionNodeSnapshot>,
    issues: Vec<InvariantIssue>,
    replay_state_invalid: bool,
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
    let legality_errors = evaluate_action_legality(hand)?;

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
    let pot_resolution = resolve_hand_pots(
        hand,
        &ordered_seats,
        &replay.committed_total,
        &replay.status,
    )?;
    let mut invariant_issues = Vec::new();
    push_unique_invariant_issues(&mut invariant_issues, legality_errors);
    push_unique_invariant_issues(&mut invariant_issues, replay.issues.clone());

    let replay_state_invalid =
        replay.replay_state_invalid || invariant_issues.iter().any(issue_triggers_fail_safe);
    let mut settlement = pot_resolution.settlement;
    if replay_state_invalid {
        settlement = invalidate_settlement(settlement);
    }
    let observed_payout_totals = observed_payouts(hand).best_effort_totals();

    let stacks_after_actual = player_order
        .iter()
        .map(|player| {
            let final_stack = replay.starting_stack[player] - replay.committed_total[player]
                + observed_payout_totals.get(player).copied().unwrap_or(0);
            (player.clone(), final_stack)
        })
        .collect::<BTreeMap<_, _>>();

    let total_committed = replay.committed_total.values().sum::<i64>();
    let total_collected = observed_payout_totals.values().sum::<i64>();

    let eliminations = ordered_seats
        .iter()
        .filter_map(|seat| {
            let final_stack = stacks_after_actual
                .get(&seat.player_name)
                .copied()
                .unwrap_or(0);
            (seat.starting_stack > 0 && final_stack == 0)
                .then(|| build_elimination(seat, &settlement))
        })
        .collect::<Vec<_>>();

    let starting_sum = replay.starting_stack.values().sum::<i64>();
    let final_sum = stacks_after_actual.values().sum::<i64>();
    let chip_conservation_ok = starting_sum == final_sum;
    if !chip_conservation_ok {
        invariant_issues.push(InvariantIssue::ChipConservationMismatch {
            starting_sum,
            final_sum,
        });
    }

    let pot_conservation_ok = total_committed == total_collected + rake_amount;
    if !pot_conservation_ok {
        invariant_issues.push(InvariantIssue::PotConservationMismatch {
            committed_total: total_committed,
            collected_total: total_collected,
            rake_amount,
        });
    }
    if let Some(summary_total_pot) = hand.summary_total_pot
        && summary_total_pot != total_collected + rake_amount
    {
        invariant_issues.push(InvariantIssue::SummaryTotalPotMismatch {
            summary_total_pot,
            collected_plus_rake: total_collected + rake_amount,
        });
    }

    Ok(NormalizedHand {
        hand_id: hand.header.hand_id.clone(),
        player_order,
        snapshot: replay.snapshot,
        settlement,
        returns,
        actual: HandOutcomeActual {
            committed_total_by_player: replay.committed_total,
            stacks_after_actual,
            winner_collections: observed_payout_totals,
            final_board_cards,
            rake_amount,
        },
        eliminations,
        invariants: HandInvariants {
            chip_conservation_ok,
            pot_conservation_ok,
            issues: invariant_issues,
        },
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
            .zip(ordered_seats.iter())
            .map(|(player, seat)| {
                let player_status = if starting_stack[player] > 0 && !seat.is_sitting_out {
                    PlayerStatus::Live
                } else {
                    PlayerStatus::Eliminated
                };
                (player.clone(), player_status)
            })
            .collect::<BTreeMap<_, _>>();
        let pending_live_actors = status
            .iter()
            .filter(|(_, status)| **status == PlayerStatus::Live)
            .map(|(player, _)| player.clone())
            .collect::<BTreeSet<_>>();

        Self {
            ordered_seats,
            player_order,
            starting_stack: starting_stack.clone(),
            stack_current: starting_stack,
            committed_total,
            committed_by_street,
            betting_round_contrib,
            pending_live_actors,
            status,
            current_street: Street::Preflop,
            snapshot: None,
            issues: Vec::new(),
            replay_state_invalid: false,
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
                let refund_failures = validate_refund(
                    Some(self.committed_total[player_name]),
                    self.betting_round_contrib[player_name],
                    refund,
                );
                if !refund_failures.is_empty() {
                    self.replay_state_invalid = true;
                    for failure in refund_failures {
                        self.push_issue(invariant_issue_from_money_failure(
                            player_name,
                            event,
                            failure,
                        ));
                    }
                }

                if !self.refund_surface_allows_mutation(player_name, refund) {
                    self.replay_state_invalid = true;
                } else if self
                    .issues
                    .iter()
                    .all(|issue| !matches!(
                        issue,
                        InvariantIssue::RefundExceedsCommitted { seq, player_name: issue_player, .. }
                            | InvariantIssue::RefundExceedsBettingRoundContrib { seq, player_name: issue_player, .. }
                            if *seq == event.seq && issue_player == player_name
                    ))
                {
                    let street_key = street_key(self.current_street).to_string();
                    let stack_current = self.stack_current.get_mut(player_name).unwrap();
                    let committed_total = self.committed_total.get_mut(player_name).unwrap();
                    let committed_by_street = self
                        .committed_by_street
                        .get_mut(player_name)
                        .and_then(|by_street| by_street.get_mut(&street_key))
                        .unwrap();
                    let betting_round_contrib =
                        self.betting_round_contrib.get_mut(player_name).unwrap();

                    if let Err(failures) = apply_refund(
                        stack_current,
                        Some(committed_total),
                        Some(committed_by_street),
                        betting_round_contrib,
                        refund,
                    ) {
                        for failure in failures {
                            self.push_issue(invariant_issue_from_money_failure(
                                player_name,
                                event,
                                failure,
                            ));
                        }
                    }
                }
            }
            ActionType::Collect | ActionType::Show | ActionType::Muck => {}
        }

        if delta > 0 {
            let stack_current = self.stack_current.get_mut(player_name).unwrap();
            if let Err(failure) = apply_debit(stack_current, delta) {
                self.replay_state_invalid = true;
                self.push_issue(invariant_issue_from_money_failure(
                    player_name,
                    event,
                    failure,
                ));
            } else {
                *self.committed_total.get_mut(player_name).unwrap() += delta;
                *self
                    .committed_by_street
                    .get_mut(player_name)
                    .unwrap()
                    .get_mut(street_key(self.current_street))
                    .unwrap() += delta;
                if contributes_to_betting_round {
                    *self.betting_round_contrib.get_mut(player_name).unwrap() += delta;
                }
            }
        }

        self.update_player_status(player_name, event);
        self.update_pending_live_actors(player_name, event);

        if !self.replay_state_invalid && self.snapshot.is_none() && self.should_capture_snapshot() {
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

    fn refund_surface_allows_mutation(&self, player_name: &str, refund: i64) -> bool {
        let before_contrib = self.betting_round_contrib[player_name];
        let highest_other_contrib = self
            .betting_round_contrib
            .iter()
            .filter(|(candidate, _)| candidate.as_str() != player_name)
            .map(|(_, amount)| *amount)
            .max()
            .unwrap_or(0);
        let overage = (before_contrib - highest_other_contrib).max(0);

        overage > 0 && refund <= overage
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
            self.pending_live_actors = self
                .player_order
                .iter()
                .filter(|player| self.status[player.as_str()] == PlayerStatus::Live)
                .cloned()
                .collect::<BTreeSet<_>>();
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

    fn update_pending_live_actors(
        &mut self,
        player_name: &str,
        event: &crate::models::HandActionEvent,
    ) {
        self.pending_live_actors
            .retain(|player| self.status[player.as_str()] == PlayerStatus::Live);

        match event.action_type {
            ActionType::Bet | ActionType::RaiseTo => {
                self.pending_live_actors = self
                    .player_order
                    .iter()
                    .filter(|player| {
                        player.as_str() != player_name
                            && self.status[player.as_str()] == PlayerStatus::Live
                    })
                    .cloned()
                    .collect::<BTreeSet<_>>();
            }
            ActionType::Check | ActionType::Call | ActionType::Fold => {
                self.pending_live_actors.remove(player_name);
            }
            ActionType::ReturnUncalled => {
                self.pending_live_actors.clear();
            }
            ActionType::PostAnte
            | ActionType::PostSb
            | ActionType::PostBb
            | ActionType::PostDead
            | ActionType::Collect
            | ActionType::Show
            | ActionType::Muck => {}
        }
    }

    fn should_capture_snapshot(&self) -> bool {
        if matches!(self.current_street, Street::Showdown | Street::Summary) {
            return false;
        }
        if !self.pending_live_actors.is_empty() {
            return false;
        }

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
        let all_contestants_all_in = contestants.len() >= 2 && all_in_count == contestants.len();
        let betting_closed_with_single_live =
            contestants.len() >= 2 && all_in_count >= 1 && live_count == 1;

        all_contestants_all_in || betting_closed_with_single_live
    }

    fn push_issue(&mut self, issue: InvariantIssue) {
        if !self.issues.contains(&issue) {
            self.issues.push(issue);
        }
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

fn build_elimination(
    seat: &ParsedHandSeat,
    settlement: &crate::models::HandSettlement,
) -> HandElimination {
    let pots_participated_by_busted = settlement
        .pots
        .iter()
        .filter(|pot| {
            pot.contributions
                .iter()
                .any(|contribution| contribution.seat_no == seat.seat_no)
        })
        .map(|pot| pot.pot_no)
        .collect::<Vec<_>>();
    let mut pots_causing_bust = Vec::new();
    let mut stack_remaining = seat.starting_stack;

    for pot in &settlement.pots {
        let Some(contribution) = pot
            .contributions
            .iter()
            .find(|contribution| contribution.seat_no == seat.seat_no)
        else {
            continue;
        };

        stack_remaining -= contribution.amount;

        let player_is_eligible = pot
            .eligibilities
            .iter()
            .any(|eligibility| eligibility.seat_no == seat.seat_no);
        let player_won_share = pot.selected_allocation.as_ref().is_some_and(|allocation| {
            allocation
                .shares
                .iter()
                .any(|share| share.seat_no == seat.seat_no && share.share_amount > 0)
        });

        if player_is_eligible && !player_won_share && stack_remaining <= 0 {
            pots_causing_bust.push(pot.pot_no);
            break;
        }
    }

    let last_busting_pot_no = pots_causing_bust.last().copied();
    let busting_pot = last_busting_pot_no
        .and_then(|pot_no| settlement.pots.iter().find(|pot| pot.pot_no == pot_no));
    let (ko_winner_set, ko_share_fraction_by_winner, ko_certainty_state) =
        if let Some(pot) = busting_pot {
            if let Some(allocation) = pot.selected_allocation.as_ref() {
                let ko_winner_set = allocation
                    .shares
                    .iter()
                    .map(|share| share.player_name.clone())
                    .collect::<Vec<_>>();
                let ko_share_fraction_by_winner = if pot.amount > 0 {
                    allocation
                        .shares
                        .iter()
                        .map(|share| HandEliminationKoShare {
                            seat_no: share.seat_no,
                            player_name: share.player_name.clone(),
                            share_fraction: share.share_amount as f64 / pot.amount as f64,
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };

                (
                    ko_winner_set,
                    ko_share_fraction_by_winner,
                    CertaintyState::Exact,
                )
            } else {
                (
                    Vec::new(),
                    Vec::new(),
                    fallback_ko_certainty_state(settlement.certainty_state),
                )
            }
        } else {
            (
                Vec::new(),
                Vec::new(),
                fallback_ko_certainty_state(settlement.certainty_state),
            )
        };

    HandElimination {
        eliminated_seat_no: seat.seat_no,
        eliminated_player_name: seat.player_name.clone(),
        pots_participated_by_busted,
        pots_causing_bust,
        last_busting_pot_no,
        ko_winner_set,
        ko_share_fraction_by_winner,
        elimination_certainty_state: CertaintyState::Exact,
        ko_certainty_state,
    }
}

fn fallback_ko_certainty_state(settlement_certainty_state: CertaintyState) -> CertaintyState {
    match settlement_certainty_state {
        CertaintyState::Inconsistent => CertaintyState::Inconsistent,
        CertaintyState::Estimated => CertaintyState::Estimated,
        CertaintyState::Exact | CertaintyState::Uncertain => CertaintyState::Uncertain,
    }
}

fn push_unique_invariant_issues(target: &mut Vec<InvariantIssue>, issues: Vec<InvariantIssue>) {
    for issue in issues {
        if !target.contains(&issue) {
            target.push(issue);
        }
    }
}

fn issue_triggers_fail_safe(issue: &InvariantIssue) -> bool {
    matches!(
        issue,
        InvariantIssue::UncalledReturnActorMismatch { .. }
            | InvariantIssue::UncalledReturnAmountMismatch { .. }
            | InvariantIssue::ActionAmountExceedsStack { .. }
            | InvariantIssue::RefundExceedsCommitted { .. }
            | InvariantIssue::RefundExceedsBettingRoundContrib { .. }
    )
}

fn invalidate_settlement(mut settlement: HandSettlement) -> HandSettlement {
    settlement.certainty_state = CertaintyState::Inconsistent;
    if !settlement
        .issues
        .iter()
        .any(|issue| matches!(issue, SettlementIssue::ReplayStateInvalid))
    {
        settlement.issues.push(SettlementIssue::ReplayStateInvalid);
    }
    for pot in &mut settlement.pots {
        pot.selected_allocation = None;
    }
    settlement
}

fn invariant_issue_from_money_failure(
    player_name: &str,
    event: &crate::models::HandActionEvent,
    failure: MoneyMutationFailure,
) -> InvariantIssue {
    match failure {
        MoneyMutationFailure::ActionAmountExceedsStack {
            available_stack,
            attempted_amount,
        } => InvariantIssue::ActionAmountExceedsStack {
            street: event.street,
            seq: event.seq,
            player_name: player_name.to_string(),
            available_stack,
            attempted_amount,
        },
        MoneyMutationFailure::RefundExceedsCommitted {
            committed_total,
            attempted_refund,
        } => InvariantIssue::RefundExceedsCommitted {
            street: event.street,
            seq: event.seq,
            player_name: player_name.to_string(),
            committed_total,
            actual_refund: attempted_refund,
        },
        MoneyMutationFailure::RefundExceedsBettingRoundContrib {
            betting_round_contrib,
            attempted_refund,
        } => InvariantIssue::RefundExceedsBettingRoundContrib {
            street: event.street,
            seq: event.seq,
            player_name: player_name.to_string(),
            betting_round_contrib,
            actual_refund: attempted_refund,
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
