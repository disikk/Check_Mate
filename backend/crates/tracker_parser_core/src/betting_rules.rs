use std::collections::BTreeMap;

use crate::{
    ParserError,
    models::{ActionType, CanonicalParsedHand, HandActionEvent, PlayerStatus, Street},
    positions::{PositionSeatInput, compute_position_facts},
};

pub fn evaluate_action_legality(hand: &CanonicalParsedHand) -> Result<Vec<String>, ParserError> {
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
    issues: Vec<String>,
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
        button_positions.sort_by_key(|position| button_order_rank(position.position_code.as_str()));
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
        self.validate_actor_surface(player_name, event, before_contrib);

        let aggression = if is_voluntary_action(event.action_type) {
            self.ensure_street_initialized();
            self.validate_actor_order(player_name, event);
            self.classify_aggression(player_name, event, before_contrib, before_stack)?
        } else {
            AggressionKind::None
        };

        self.apply_stack_and_status(player_name, event)?;

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
            self.push_issue(
                "premature_street_close",
                format!(
                    "street={} pending={}",
                    street_code(self.current_street),
                    self.eligible_to_act.join(",")
                ),
            );
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
            self.push_issue(
                "illegal_actor_order",
                format!(
                    "street={} seq={} expected={} actual={} raw_line={}",
                    street_code(event.street),
                    event.seq,
                    expected_actor,
                    player_name,
                    event.raw_line
                ),
            );
        }
    }

    fn validate_actor_surface(
        &mut self,
        player_name: &str,
        event: &HandActionEvent,
        before_contrib: i64,
    ) {
        match event.action_type {
            ActionType::PostSb => {
                if let Some(expected_actor) = self.expected_small_blind_actor()
                    && expected_actor != player_name
                {
                    self.push_issue(
                        "illegal_small_blind_actor",
                        format!(
                            "seq={} expected={} actual={} raw_line={}",
                            event.seq, expected_actor, player_name, event.raw_line
                        ),
                    );
                }
            }
            ActionType::PostBb => {
                if let Some(expected_actor) = self.expected_big_blind_actor()
                    && expected_actor != player_name
                {
                    self.push_issue(
                        "illegal_big_blind_actor",
                        format!(
                            "seq={} expected={} actual={} raw_line={}",
                            event.seq, expected_actor, player_name, event.raw_line
                        ),
                    );
                }
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
                    self.push_issue(
                        "uncalled_return_actor_mismatch",
                        format!(
                            "seq={} player={} raw_line={}",
                            event.seq, player_name, event.raw_line
                        ),
                    );
                } else if refund > overage {
                    self.push_issue(
                        "uncalled_return_amount_mismatch",
                        format!(
                            "seq={} player={} allowed_refund={} actual_refund={} raw_line={}",
                            event.seq, player_name, overage, refund, event.raw_line
                        ),
                    );
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
            | ActionType::Muck => {}
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
                    self.push_issue(
                        "illegal_check",
                        format!(
                            "street={} seq={} player={} required_call={} raw_line={}",
                            street_code(event.street),
                            event.seq,
                            player_name,
                            required_call,
                            event.raw_line
                        ),
                    );
                }
                Ok(AggressionKind::None)
            }
            ActionType::Call => {
                let amount = event.amount.unwrap_or(0);
                let expected_call = required_call.min(before_stack);
                if required_call == 0 {
                    self.push_issue(
                        "illegal_call_amount",
                        format!(
                            "street={} seq={} player={} expected_call=0 actual={} raw_line={}",
                            street_code(event.street),
                            event.seq,
                            player_name,
                            amount,
                            event.raw_line
                        ),
                    );
                } else if amount < expected_call {
                    self.push_issue(
                        "undercall_inconsistency",
                        format!(
                            "street={} seq={} player={} expected_call={} actual={} raw_line={}",
                            street_code(event.street),
                            event.seq,
                            player_name,
                            expected_call,
                            amount,
                            event.raw_line
                        ),
                    );
                } else if amount > expected_call {
                    self.push_issue(
                        "overcall_inconsistency",
                        format!(
                            "street={} seq={} player={} expected_call={} actual={} raw_line={}",
                            street_code(event.street),
                            event.seq,
                            player_name,
                            expected_call,
                            amount,
                            event.raw_line
                        ),
                    );
                }
                Ok(AggressionKind::None)
            }
            ActionType::Bet => {
                if required_call != 0 {
                    self.push_issue(
                        "illegal_bet_facing_open_bet",
                        format!(
                            "street={} seq={} player={} required_call={} raw_line={}",
                            street_code(event.street),
                            event.seq,
                            player_name,
                            required_call,
                            event.raw_line
                        ),
                    );
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
            self.push_issue(
                "action_not_reopened_after_short_all_in",
                format!(
                    "street={} seq={} player={} raw_line={}",
                    street_code(event.street),
                    event.seq,
                    player_name,
                    event.raw_line
                ),
            );
        }

        if new_to_call <= self.current_to_call {
            self.push_issue(
                "incomplete_raise",
                format!(
                    "street={} seq={} player={} current_to_call={} attempted_to={} raw_line={}",
                    street_code(event.street),
                    event.seq,
                    player_name,
                    self.current_to_call,
                    new_to_call,
                    event.raw_line
                ),
            );
            return Ok(AggressionKind::None);
        }

        if raise_size < self.last_full_raise_size {
            if event.is_all_in {
                return Ok(AggressionKind::ShortAllInRaise { new_to_call });
            }

            self.push_issue(
                "incomplete_raise",
                format!(
                    "street={} seq={} player={} min_raise={} actual_raise={} raw_line={}",
                    street_code(event.street),
                    event.seq,
                    player_name,
                    self.last_full_raise_size,
                    raise_size,
                    event.raw_line
                ),
            );
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
                *self
                    .stack_current
                    .entry(player_name.to_string())
                    .or_default() += refund;
                *self
                    .betting_round_contrib
                    .entry(player_name.to_string())
                    .or_default() -= refund;
            }
            ActionType::Collect | ActionType::Show | ActionType::Muck => {}
        }

        if delta > 0 {
            *self
                .stack_current
                .entry(player_name.to_string())
                .or_default() -= delta;
            if contributes_to_betting_round {
                *self
                    .betting_round_contrib
                    .entry(player_name.to_string())
                    .or_default() += delta;
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

    fn push_issue(&mut self, code: &str, detail: String) {
        self.issues.push(format!("{code}: {detail}"));
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

fn street_code(street: Street) -> &'static str {
    match street {
        Street::Preflop => "preflop",
        Street::Flop => "flop",
        Street::Turn => "turn",
        Street::River => "river",
        Street::Showdown => "showdown",
        Street::Summary => "summary",
    }
}

fn button_order_rank(position_code: &str) -> u8 {
    match position_code {
        "BTN" => 0,
        "SB" => 1,
        "BB" => 2,
        "UTG" => 3,
        "UTG+1" => 4,
        "MP" => 5,
        "LJ" => 6,
        "HJ" => 7,
        "CO" => 8,
        _ => 255,
    }
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
