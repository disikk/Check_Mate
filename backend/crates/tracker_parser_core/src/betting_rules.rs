use std::collections::BTreeMap;

use crate::{
    ParserError,
    models::{
        ActionType, CanonicalParsedHand, HandActionEvent, InvariantIssue, PlayerStatus, Street,
    },
    money_state::{MoneyMutationFailure, apply_debit, apply_refund, validate_refund},
    positions::{PositionSeatInput, compute_position_facts},
};

pub fn evaluate_action_legality(
    hand: &CanonicalParsedHand,
) -> Result<Vec<InvariantIssue>, ParserError> {
    let mut engine = LegalityEngine::new(hand)?;
    for event in &hand.actions {
        engine.apply_event(event)?;
    }
    Ok(engine.issues)
}

#[derive(Debug, Clone)]
struct LegalityEngine {
    big_blind: i64,
    button_order: Vec<String>,
    blindless_preflop_order: Vec<String>,
    preflop_order: Vec<String>,
    postflop_order: Vec<String>,
    stack_current: BTreeMap<String, i64>,
    betting_round_contrib: BTreeMap<String, i64>,
    status: BTreeMap<String, PlayerStatus>,
    current_street: Street,
    current_to_call: i64,
    last_full_raise_size: i64,
    last_aggressor: Option<String>,
    eligible_to_act: Vec<String>,
    street_opener: Option<String>,
    street_closer: Option<String>,
    action_reopened: bool,
    street_initialized: bool,
    raise_reopened_for: BTreeMap<String, bool>,
    issues: Vec<InvariantIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AggressionKind {
    None,
    FullRaise { new_to_call: i64, raise_size: i64 },
    ShortAllInRaise { new_to_call: i64 },
}

impl LegalityEngine {
    fn new(hand: &CanonicalParsedHand) -> Result<Self, ParserError> {
        let mut ordered_seats = hand.seats.clone();
        ordered_seats.sort_by_key(|seat| seat.seat_no);

        let position_inputs = ordered_seats
            .iter()
            .map(|seat| PositionSeatInput {
                seat_no: seat.seat_no,
                is_active: seat.starting_stack > 0 && !seat.is_sitting_out,
            })
            .collect::<Vec<_>>();
        let positions = compute_position_facts(
            hand.header.max_players,
            hand.header.button_seat,
            &position_inputs,
        )
        .map_err(|error| ParserError::InvalidField {
            field: "position_facts",
            value: error.to_string(),
        })?;

        let player_by_seat = ordered_seats
            .iter()
            .map(|seat| (seat.seat_no, seat.player_name.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut preflop_positions = positions.clone();
        let mut button_positions = positions.clone();
        button_positions.sort_by_key(|position| position.position_index);
        let button_order = button_positions
            .into_iter()
            .filter_map(|position| player_by_seat.get(&position.seat_no).cloned())
            .collect::<Vec<_>>();
        preflop_positions.sort_by_key(|position| position.preflop_act_order_index);
        let preflop_order = preflop_positions
            .into_iter()
            .filter_map(|position| player_by_seat.get(&position.seat_no).cloned())
            .collect::<Vec<_>>();
        let mut postflop_positions = positions;
        postflop_positions.sort_by_key(|position| position.postflop_act_order_index);
        let postflop_order = postflop_positions
            .into_iter()
            .filter_map(|position| player_by_seat.get(&position.seat_no).cloned())
            .collect::<Vec<_>>();

        let stack_current = ordered_seats
            .iter()
            .map(|seat| (seat.player_name.clone(), seat.starting_stack))
            .collect::<BTreeMap<_, _>>();
        let betting_round_contrib = ordered_seats
            .iter()
            .map(|seat| (seat.player_name.clone(), 0_i64))
            .collect::<BTreeMap<_, _>>();
        let status = ordered_seats
            .iter()
            .map(|seat| {
                (
                    seat.player_name.clone(),
                    if seat.starting_stack > 0 && !seat.is_sitting_out {
                        PlayerStatus::Live
                    } else {
                        PlayerStatus::Eliminated
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let raise_reopened_for = ordered_seats
            .iter()
            .map(|seat| {
                (
                    seat.player_name.clone(),
                    seat.starting_stack > 0 && !seat.is_sitting_out,
                )
            })
            .collect::<BTreeMap<_, _>>();

        Ok(Self {
            big_blind: i64::from(hand.header.big_blind),
            button_order: button_order.clone(),
            blindless_preflop_order: rotate_left(&button_order, 1),
            preflop_order,
            postflop_order,
            stack_current,
            betting_round_contrib,
            status,
            current_street: Street::Preflop,
            current_to_call: 0,
            last_full_raise_size: i64::from(hand.header.big_blind),
            last_aggressor: None,
            eligible_to_act: Vec::new(),
            street_opener: None,
            street_closer: None,
            action_reopened: false,
            street_initialized: false,
            raise_reopened_for,
            issues: Vec::new(),
        })
    }

    fn apply_event(&mut self, event: &HandActionEvent) -> Result<(), ParserError> {
        self.advance_street_if_needed(event.street);

        let Some(player_name) = event.player_name.as_ref() else {
            return Ok(());
        };

        let before_contrib = self.betting_round_contrib[player_name];
        let before_stack = self.stack_current[player_name];
        let allow_return_mutation = self.validate_actor_surface(player_name, event, before_contrib);

        let aggression = if is_voluntary_action(event.action_type) {
            self.ensure_street_initialized();
            self.validate_actor_order(player_name, event);
            self.classify_aggression(player_name, event, before_contrib, before_stack)?
        } else {
            AggressionKind::None
        };

        self.apply_stack_and_status(player_name, event, allow_return_mutation)?;

        if is_forced_betting_post(event.action_type)
            && matches!(self.current_street, Street::Preflop)
        {
            self.current_to_call = self
                .current_to_call
                .max(self.betting_round_contrib[player_name]);
        }

        if is_voluntary_action(event.action_type) {
            self.advance_round(player_name, event, aggression);
        }

        Ok(())
    }

    fn advance_street_if_needed(&mut self, next_street: Street) {
        if next_street == self.current_street {
            return;
        }

        if is_betting_street(self.current_street)
            && !matches!(next_street, Street::Preflop)
            && !self.eligible_to_act.is_empty()
        {
            self.push_issue(InvariantIssue::PrematureStreetClose {
                street: self.current_street,
                pending_players: self.eligible_to_act.clone(),
            });
        }

        self.current_street = next_street;
        self.current_to_call = 0;
        self.last_full_raise_size = self.big_blind;
        self.last_aggressor = None;
        self.eligible_to_act.clear();
        self.street_opener = None;
        self.street_closer = None;
        self.action_reopened = false;
        self.street_initialized = false;

        if is_betting_street(next_street) {
            for amount in self.betting_round_contrib.values_mut() {
                *amount = 0;
            }
            for (player_name, status) in &self.status {
                self.raise_reopened_for
                    .insert(player_name.clone(), *status == PlayerStatus::Live);
            }
        }
    }

    fn ensure_street_initialized(&mut self) {
        if self.street_initialized || !is_betting_street(self.current_street) {
            return;
        }

        let order = self
            .street_order()
            .iter()
            .filter(|player_name| self.status[player_name.as_str()] == PlayerStatus::Live)
            .cloned()
            .collect::<Vec<_>>();
        self.street_opener = order.first().cloned();
        self.street_closer = order.last().cloned();
        self.eligible_to_act = order;
        self.street_initialized = true;
    }

    fn validate_actor_order(&mut self, player_name: &str, event: &HandActionEvent) {
        let Some(expected_actor) = self.eligible_to_act.first() else {
            return;
        };
        if expected_actor != player_name {
            self.push_issue(InvariantIssue::IllegalActorOrder {
                street: event.street,
                seq: event.seq,
                expected_actor: expected_actor.clone(),
                actual_actor: player_name.to_string(),
            });
        }
    }

    fn validate_actor_surface(
        &mut self,
        player_name: &str,
        event: &HandActionEvent,
        before_contrib: i64,
    ) -> bool {
        match event.action_type {
            ActionType::PostSb => {
                if let Some(expected_actor) = self.expected_small_blind_actor()
                    && expected_actor != player_name
                {
                    self.push_issue(InvariantIssue::IllegalSmallBlindActor {
                        seq: event.seq,
                        expected_actor: expected_actor.to_string(),
                        actual_actor: player_name.to_string(),
                    });
                }
                true
            }
            ActionType::PostBb => {
                if let Some(expected_actor) = self.expected_big_blind_actor()
                    && expected_actor != player_name
                {
                    self.push_issue(InvariantIssue::IllegalBigBlindActor {
                        seq: event.seq,
                        expected_actor: expected_actor.to_string(),
                        actual_actor: player_name.to_string(),
                    });
                }
                true
            }
            ActionType::ReturnUncalled => {
                let highest_other_contrib = self
                    .betting_round_contrib
                    .iter()
                    .filter(|(candidate, _)| candidate.as_str() != player_name)
                    .map(|(_, amount)| *amount)
                    .max()
                    .unwrap_or(0);
                let refund = event.amount.unwrap_or(0);
                let overage = (before_contrib - highest_other_contrib).max(0);

                if overage == 0 {
                    self.push_issue(InvariantIssue::UncalledReturnActorMismatch {
                        seq: event.seq,
                        player_name: player_name.to_string(),
                    });
                    false
                } else if refund > overage {
                    self.push_issue(InvariantIssue::UncalledReturnAmountMismatch {
                        seq: event.seq,
                        player_name: player_name.to_string(),
                        allowed_refund: overage,
                        actual_refund: refund,
                    });
                    false
                } else {
                    true
                }
            }
            ActionType::PostAnte
            | ActionType::PostDead
            | ActionType::Fold
            | ActionType::Check
            | ActionType::Call
            | ActionType::Bet
            | ActionType::RaiseTo
            | ActionType::Collect
            | ActionType::Show
            | ActionType::Muck => true,
        }
    }

    fn classify_aggression(
        &mut self,
        player_name: &str,
        event: &HandActionEvent,
        before_contrib: i64,
        before_stack: i64,
    ) -> Result<AggressionKind, ParserError> {
        let required_call = (self.current_to_call - before_contrib).max(0);

        match event.action_type {
            ActionType::Fold => Ok(AggressionKind::None),
            ActionType::Check => {
                if required_call != 0 {
                    self.push_issue(InvariantIssue::IllegalCheck {
                        street: event.street,
                        seq: event.seq,
                        player_name: player_name.to_string(),
                        required_call,
                    });
                }
                Ok(AggressionKind::None)
            }
            ActionType::Call => {
                let amount = event.amount.unwrap_or(0);
                let expected_call = required_call.min(before_stack);
                if required_call == 0 {
                    self.push_issue(InvariantIssue::IllegalCallAmount {
                        street: event.street,
                        seq: event.seq,
                        player_name: player_name.to_string(),
                        expected_call: 0,
                        actual_amount: amount,
                    });
                } else if amount < expected_call {
                    self.push_issue(InvariantIssue::UndercallInconsistency {
                        street: event.street,
                        seq: event.seq,
                        player_name: player_name.to_string(),
                        expected_call,
                        actual_amount: amount,
                    });
                } else if amount > expected_call {
                    self.push_issue(InvariantIssue::OvercallInconsistency {
                        street: event.street,
                        seq: event.seq,
                        player_name: player_name.to_string(),
                        expected_call,
                        actual_amount: amount,
                    });
                }
                Ok(AggressionKind::None)
            }
            ActionType::Bet => {
                if required_call != 0 {
                    self.push_issue(InvariantIssue::IllegalBetFacingOpenBet {
                        street: event.street,
                        seq: event.seq,
                        player_name: player_name.to_string(),
                        required_call,
                    });
                }

                let amount = event.amount.unwrap_or(0);
                let new_to_call = before_contrib + amount;
                self.classify_raise(
                    player_name,
                    event,
                    required_call,
                    new_to_call,
                    new_to_call - self.current_to_call,
                )
            }
            ActionType::RaiseTo => {
                let to_amount = event.to_amount.ok_or(ParserError::InvalidField {
                    field: "to_amount",
                    value: event.raw_line.clone(),
                })?;
                self.classify_raise(
                    player_name,
                    event,
                    required_call,
                    to_amount,
                    to_amount - self.current_to_call,
                )
            }
            ActionType::PostAnte
            | ActionType::PostSb
            | ActionType::PostBb
            | ActionType::PostDead
            | ActionType::ReturnUncalled
            | ActionType::Collect
            | ActionType::Show
            | ActionType::Muck => Ok(AggressionKind::None),
        }
    }

    fn classify_raise(
        &mut self,
        player_name: &str,
        event: &HandActionEvent,
        required_call: i64,
        new_to_call: i64,
        raise_size: i64,
    ) -> Result<AggressionKind, ParserError> {
        if required_call > 0
            && !self
                .raise_reopened_for
                .get(player_name)
                .copied()
                .unwrap_or(true)
        {
            self.push_issue(InvariantIssue::ActionNotReopenedAfterShortAllIn {
                street: event.street,
                seq: event.seq,
                player_name: player_name.to_string(),
            });
        }

        if new_to_call <= self.current_to_call {
            self.push_issue(InvariantIssue::IncompleteRaiseToCall {
                street: event.street,
                seq: event.seq,
                player_name: player_name.to_string(),
                current_to_call: self.current_to_call,
                attempted_to: new_to_call,
            });
            return Ok(AggressionKind::None);
        }

        if raise_size < self.last_full_raise_size {
            if event.is_all_in {
                return Ok(AggressionKind::ShortAllInRaise { new_to_call });
            }

            self.push_issue(InvariantIssue::IncompleteRaiseSize {
                street: event.street,
                seq: event.seq,
                player_name: player_name.to_string(),
                min_raise: self.last_full_raise_size,
                actual_raise: raise_size,
            });
            return Ok(AggressionKind::ShortAllInRaise { new_to_call });
        }

        Ok(AggressionKind::FullRaise {
            new_to_call,
            raise_size,
        })
    }

    fn apply_stack_and_status(
        &mut self,
        player_name: &str,
        event: &HandActionEvent,
        allow_return_mutation: bool,
    ) -> Result<(), ParserError> {
        let mut delta = 0_i64;
        let before_contrib = self.betting_round_contrib[player_name];
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
                        .insert(player_name.to_string(), PlayerStatus::Folded);
                }
            }
            ActionType::Check => {}
            ActionType::Call | ActionType::Bet => {
                delta = event.amount.unwrap_or(0);
                contributes_to_betting_round = true;
            }
            ActionType::RaiseTo => {
                let to_amount = event.to_amount.ok_or(ParserError::InvalidField {
                    field: "to_amount",
                    value: event.raw_line.clone(),
                })?;
                delta = (to_amount - before_contrib).max(0);
                contributes_to_betting_round = true;
            }
            ActionType::ReturnUncalled => {
                let refund = event.amount.unwrap_or(0);
                let refund_failures =
                    validate_refund(None, self.betting_round_contrib[player_name], refund);
                for failure in refund_failures {
                    self.push_issue(invariant_issue_from_money_failure(
                        player_name,
                        event,
                        failure,
                    ));
                }

                let has_refund_money_issue = self.issues.iter().any(|issue| {
                    matches!(
                        issue,
                        InvariantIssue::RefundExceedsCommitted { seq, player_name: issue_player, .. }
                            | InvariantIssue::RefundExceedsBettingRoundContrib { seq, player_name: issue_player, .. }
                            if *seq == event.seq && issue_player == player_name
                    )
                });

                if allow_return_mutation && !has_refund_money_issue {
                    let stack_current = self.stack_current.get_mut(player_name).unwrap();
                    let betting_round_contrib =
                        self.betting_round_contrib.get_mut(player_name).unwrap();
                    if let Err(failures) =
                        apply_refund(stack_current, None, None, betting_round_contrib, refund)
                    {
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
                self.push_issue(invariant_issue_from_money_failure(
                    player_name,
                    event,
                    failure,
                ));
            } else if contributes_to_betting_round {
                *self.betting_round_contrib.get_mut(player_name).unwrap() += delta;
            }
        }

        if !matches!(event.action_type, ActionType::Fold)
            && self.status[player_name] != PlayerStatus::Eliminated
            && self.status[player_name] != PlayerStatus::Folded
        {
            if event.is_all_in || self.stack_current[player_name] == 0 {
                self.status
                    .insert(player_name.to_string(), PlayerStatus::AllIn);
            } else {
                self.status
                    .insert(player_name.to_string(), PlayerStatus::Live);
            }
        }

        Ok(())
    }

    fn advance_round(
        &mut self,
        player_name: &str,
        event: &HandActionEvent,
        aggression: AggressionKind,
    ) {
        self.eligible_to_act
            .retain(|candidate| candidate != player_name);
        self.raise_reopened_for
            .insert(player_name.to_string(), false);

        match aggression {
            AggressionKind::None => {}
            AggressionKind::FullRaise {
                new_to_call,
                raise_size,
            } => {
                self.current_to_call = new_to_call;
                self.last_full_raise_size = raise_size;
                self.last_aggressor = Some(player_name.to_string());
                self.action_reopened = true;
                for (candidate, status) in &self.status {
                    self.raise_reopened_for
                        .insert(candidate.clone(), *status == PlayerStatus::Live);
                }
                self.raise_reopened_for
                    .insert(player_name.to_string(), false);
                self.eligible_to_act = self
                    .actors_after(player_name)
                    .into_iter()
                    .filter(|candidate| self.status[candidate.as_str()] == PlayerStatus::Live)
                    .filter(|candidate| {
                        self.betting_round_contrib[candidate.as_str()] < self.current_to_call
                    })
                    .collect::<Vec<_>>();
            }
            AggressionKind::ShortAllInRaise { new_to_call } => {
                self.current_to_call = new_to_call;
                self.last_aggressor = Some(player_name.to_string());
                self.action_reopened = false;
                self.eligible_to_act = self
                    .actors_after(player_name)
                    .into_iter()
                    .filter(|candidate| self.status[candidate.as_str()] == PlayerStatus::Live)
                    .filter(|candidate| {
                        self.betting_round_contrib[candidate.as_str()] < self.current_to_call
                    })
                    .collect::<Vec<_>>();
            }
        }

        self.street_closer = self
            .eligible_to_act
            .last()
            .cloned()
            .or_else(|| self.last_aggressor.clone())
            .or_else(|| self.street_closer.clone());

        if self.contesting_player_count() <= 1
            || self.live_player_count() == 0
            || (self.live_player_count() <= 1 && self.pending_live_players_are_matched())
        {
            self.eligible_to_act.clear();
        }

        if !is_betting_street(event.street) {
            self.eligible_to_act.clear();
        }
    }

    fn street_order(&self) -> &[String] {
        match self.current_street {
            Street::Preflop => {
                if self.big_blind == 0 {
                    &self.blindless_preflop_order
                } else {
                    &self.preflop_order
                }
            }
            Street::Flop | Street::Turn | Street::River => &self.postflop_order,
            Street::Showdown | Street::Summary => &self.preflop_order[..0],
        }
    }

    fn actors_after(&self, player_name: &str) -> Vec<String> {
        let order = self.street_order();
        if order.is_empty() {
            return Vec::new();
        }

        let Some(index) = order.iter().position(|candidate| candidate == player_name) else {
            return order.to_vec();
        };

        (1..order.len())
            .map(|offset| order[(index + offset) % order.len()].clone())
            .collect()
    }

    fn push_issue(&mut self, issue: InvariantIssue) {
        self.issues.push(issue);
    }

    fn contesting_player_count(&self) -> usize {
        self.status
            .values()
            .filter(|status| !matches!(status, PlayerStatus::Folded | PlayerStatus::Eliminated))
            .count()
    }

    fn live_player_count(&self) -> usize {
        self.status
            .values()
            .filter(|status| **status == PlayerStatus::Live)
            .count()
    }

    fn pending_live_players_are_matched(&self) -> bool {
        self.eligible_to_act
            .iter()
            .filter(|player_name| self.status[player_name.as_str()] == PlayerStatus::Live)
            .all(|player_name| {
                self.betting_round_contrib[player_name.as_str()] >= self.current_to_call
            })
    }

    fn expected_small_blind_actor(&self) -> Option<&str> {
        if self.button_order.len() < 2 || self.big_blind == 0 {
            return None;
        }

        if self.button_order.len() == 2 {
            return self.button_order.first().map(String::as_str);
        }

        self.button_order.get(1).map(String::as_str)
    }

    fn expected_big_blind_actor(&self) -> Option<&str> {
        if self.button_order.len() < 2 || self.big_blind == 0 {
            return None;
        }

        if self.button_order.len() == 2 {
            return self.button_order.get(1).map(String::as_str);
        }

        self.button_order.get(2).map(String::as_str)
    }
}

fn is_betting_street(street: Street) -> bool {
    matches!(
        street,
        Street::Preflop | Street::Flop | Street::Turn | Street::River
    )
}

fn invariant_issue_from_money_failure(
    player_name: &str,
    event: &HandActionEvent,
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

fn is_forced_betting_post(action_type: ActionType) -> bool {
    matches!(
        action_type,
        ActionType::PostSb | ActionType::PostBb | ActionType::PostDead
    )
}

fn is_voluntary_action(action_type: ActionType) -> bool {
    matches!(
        action_type,
        ActionType::Fold
            | ActionType::Check
            | ActionType::Call
            | ActionType::Bet
            | ActionType::RaiseTo
    )
}

fn rotate_left(order: &[String], rotation: usize) -> Vec<String> {
    if order.is_empty() {
        return Vec::new();
    }

    let len = order.len();
    (0..len)
        .map(|offset| order[(rotation + offset) % len].clone())
        .collect()
}
