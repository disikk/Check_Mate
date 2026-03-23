use std::collections::BTreeMap;

use crate::{
    ParserError,
    models::{
        ActionType, CanonicalParsedHand, HandElimination, HandOutcomeActual,
        NormalizationInvariants, NormalizedHand, PlayerNodeState, PlayerStatus, PotSlice,
        ResolutionNodeSnapshot, Street,
    },
};

pub fn normalize_hand(hand: &CanonicalParsedHand) -> Result<NormalizedHand, ParserError> {
    let hero_name = hand
        .hero_name
        .clone()
        .ok_or(ParserError::MissingLine("hero_name"))?;

    let ordered_seats = {
        let mut seats = hand.seats.clone();
        seats.sort_by_key(|seat| seat.seat_no);
        seats
    };
    let player_order = ordered_seats
        .iter()
        .map(|seat| seat.player_name.clone())
        .collect::<Vec<_>>();
    let starting_stack = ordered_seats
        .iter()
        .map(|seat| (seat.player_name.clone(), seat.starting_stack))
        .collect::<BTreeMap<_, _>>();

    let mut stack_current = starting_stack.clone();
    let mut committed_total = player_order
        .iter()
        .map(|player| (player.clone(), 0_i64))
        .collect::<BTreeMap<_, _>>();
    let mut committed_by_street = player_order
        .iter()
        .map(|player| (player.clone(), empty_committed_by_street()))
        .collect::<BTreeMap<_, _>>();
    let mut betting_round_contrib = player_order
        .iter()
        .map(|player| (player.clone(), 0_i64))
        .collect::<BTreeMap<_, _>>();
    let mut status = player_order
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
    let warnings = hand.parse_warnings.clone();
    let mut invariant_errors = Vec::new();
    let mut snapshot = None;
    let mut current_street = Street::Preflop;

    for event in &hand.actions {
        if matches!(event.street, Street::Showdown | Street::Summary) {
            current_street = event.street;
        } else if event.street != current_street {
            current_street = event.street;
            betting_round_contrib = player_order
                .iter()
                .map(|player| (player.clone(), 0_i64))
                .collect::<BTreeMap<_, _>>();
        }

        let Some(player_name) = event.player_name.as_ref() else {
            continue;
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
                if status[player_name] != PlayerStatus::Eliminated {
                    status.insert(player_name.clone(), PlayerStatus::Folded);
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
                let already_contributed = betting_round_contrib[player_name];
                delta = (to_amount - already_contributed).max(0);
                contributes_to_betting_round = true;
            }
            ActionType::ReturnUncalled => {
                let refund = event.amount.unwrap_or(0);
                *stack_current.entry(player_name.clone()).or_default() += refund;
                *committed_total.entry(player_name.clone()).or_default() -= refund;
                *committed_by_street
                    .entry(player_name.clone())
                    .or_insert_with(empty_committed_by_street)
                    .entry(street_key(current_street).to_string())
                    .or_default() -= refund;
                *betting_round_contrib
                    .entry(player_name.clone())
                    .or_default() -= refund;
            }
            ActionType::Collect | ActionType::Show | ActionType::Muck => {}
        }

        if delta > 0 {
            *stack_current.entry(player_name.clone()).or_default() -= delta;
            *committed_total.entry(player_name.clone()).or_default() += delta;
            *committed_by_street
                .entry(player_name.clone())
                .or_insert_with(empty_committed_by_street)
                .entry(street_key(current_street).to_string())
                .or_default() += delta;
            if contributes_to_betting_round {
                *betting_round_contrib
                    .entry(player_name.clone())
                    .or_default() += delta;
            }
        }

        if status[player_name] != PlayerStatus::Folded
            && status[player_name] != PlayerStatus::Eliminated
        {
            if event.is_all_in || stack_current[player_name] == 0 {
                status.insert(player_name.clone(), PlayerStatus::AllIn);
            } else {
                status.insert(player_name.clone(), PlayerStatus::Live);
            }
        }

        if snapshot.is_none() {
            let contestants = player_order
                .iter()
                .filter(|player| {
                    matches!(
                        status[player.as_str()],
                        PlayerStatus::Live | PlayerStatus::AllIn
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            let all_in_count = contestants
                .iter()
                .filter(|player| status[player.as_str()] == PlayerStatus::AllIn)
                .count();
            let live_count = contestants
                .iter()
                .filter(|player| status[player.as_str()] == PlayerStatus::Live)
                .count();

            let betting_closed_with_single_live = all_in_count >= 1
                && live_count == 1
                && matches!(event.action_type, ActionType::Call | ActionType::Check);
            let all_contestants_all_in =
                contestants.len() >= 2 && all_in_count == contestants.len();

            if contestants.len() >= 2 && (all_contestants_all_in || betting_closed_with_single_live)
            {
                snapshot = Some(build_snapshot(
                    hand,
                    &hero_name,
                    event.seq,
                    current_street,
                    &ordered_seats,
                    &stack_current,
                    &committed_total,
                    &committed_by_street,
                    &status,
                ));
            }
        }
    }

    let stacks_after_actual = player_order
        .iter()
        .map(|player| {
            let final_stack = starting_stack[player] - committed_total[player]
                + hand.collected_amounts.get(player).copied().unwrap_or(0);
            (player.clone(), final_stack)
        })
        .collect::<BTreeMap<_, _>>();

    let total_committed = committed_total.values().sum::<i64>();
    let total_collected = hand.collected_amounts.values().sum::<i64>();
    let ko_involved_winner_count = hand
        .collected_amounts
        .values()
        .filter(|amount| **amount > 0)
        .count() as u8;
    let eliminations = ordered_seats
        .iter()
        .filter_map(|seat| {
            let final_stack = stacks_after_actual.get(&seat.player_name).copied().unwrap_or(0);
            (seat.starting_stack > 0 && final_stack == 0).then(|| HandElimination {
                eliminated_seat_no: seat.seat_no,
                eliminated_player_name: seat.player_name.clone(),
                resolved_by_pot_no: None,
                ko_involved_winner_count,
            })
        })
        .collect::<Vec<_>>();

    let starting_sum = starting_stack.values().sum::<i64>();
    let final_sum = stacks_after_actual.values().sum::<i64>();
    let chip_conservation_ok = starting_sum == final_sum;
    if !chip_conservation_ok {
        invariant_errors.push(format!(
            "chip_conservation_mismatch: starting_sum={starting_sum}, final_sum={final_sum}"
        ));
    }

    let pot_conservation_ok = total_committed == total_collected;
    if !pot_conservation_ok {
        invariant_errors.push(format!(
            "pot_conservation_mismatch: committed_total={total_committed}, collected_total={total_collected}"
        ));
    }

    Ok(NormalizedHand {
        hand_id: hand.header.hand_id.clone(),
        player_order,
        snapshot,
        actual: HandOutcomeActual {
            committed_total_by_player: committed_total,
            stacks_after_actual,
            winner_collections: hand.collected_amounts.clone(),
            final_board_cards: hand.board_final.clone(),
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

fn build_snapshot(
    hand: &CanonicalParsedHand,
    hero_name: &str,
    snapshot_event_seq: usize,
    street: Street,
    ordered_seats: &[crate::models::ParsedHandSeat],
    stack_current: &BTreeMap<String, i64>,
    committed_total: &BTreeMap<String, i64>,
    committed_by_street: &BTreeMap<String, BTreeMap<String, i64>>,
    status: &BTreeMap<String, PlayerStatus>,
) -> ResolutionNodeSnapshot {
    let players = ordered_seats
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
                stack_at_snapshot: stack_current[&seat.player_name],
                committed_total: committed_total[&seat.player_name],
                committed_by_street: committed_by_street[&seat.player_name].clone(),
                status: status[&seat.player_name].clone(),
                is_hero: seat.player_name == hero_name,
                hole_cards_known: hole_cards.is_some(),
                hole_cards,
            }
        })
        .collect::<Vec<_>>();

    let pots = build_pots(&players);
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

fn build_pots(players: &[PlayerNodeState]) -> Vec<PotSlice> {
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
