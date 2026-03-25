use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::models::{HandPosition, PositionCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionSeatInput {
    pub seat_no: u8,
    pub is_active: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PositionError {
    #[error("table must have between 2 and 9 active seats, found {active_seats}")]
    UnsupportedActiveSeatCount { active_seats: usize },
    #[error("button seat {button_seat} is out of range for a {max_seats}-max table")]
    ButtonSeatOutOfRange { button_seat: u8, max_seats: u8 },
    #[error("button seat {button_seat} must be active")]
    ButtonSeatInactive { button_seat: u8 },
    #[error("seat {seat_no} is out of range for a {max_seats}-max table")]
    SeatOutOfRange { seat_no: u8, max_seats: u8 },
    #[error("duplicate seat {seat_no} in position input")]
    DuplicateSeat { seat_no: u8 },
}

pub fn compute_position_facts(
    max_seats: u8,
    button_seat: u8,
    seats: &[PositionSeatInput],
) -> Result<Vec<HandPosition>, PositionError> {
    if max_seats < 2 {
        return Err(PositionError::ButtonSeatOutOfRange {
            button_seat,
            max_seats,
        });
    }
    if button_seat == 0 || button_seat > max_seats {
        return Err(PositionError::ButtonSeatOutOfRange {
            button_seat,
            max_seats,
        });
    }

    let mut seat_active = BTreeMap::new();
    for seat in seats {
        if seat.seat_no == 0 || seat.seat_no > max_seats {
            return Err(PositionError::SeatOutOfRange {
                seat_no: seat.seat_no,
                max_seats,
            });
        }
        if seat_active.insert(seat.seat_no, seat.is_active).is_some() {
            return Err(PositionError::DuplicateSeat {
                seat_no: seat.seat_no,
            });
        }
    }

    let active_seat_nos = seat_active
        .iter()
        .filter_map(|(seat_no, is_active)| is_active.then_some(*seat_no))
        .collect::<Vec<_>>();

    if !(2..=9).contains(&active_seat_nos.len()) {
        return Err(PositionError::UnsupportedActiveSeatCount {
            active_seats: active_seat_nos.len(),
        });
    }
    if !seat_active.get(&button_seat).copied().unwrap_or(false) {
        return Err(PositionError::ButtonSeatInactive { button_seat });
    }

    let active_order = clockwise_active_order(max_seats, button_seat, &seat_active);
    let position_codes = position_codes_for_active_count(active_order.len())?;
    let preflop_order = preflop_action_order(&active_order);
    let postflop_order = postflop_action_order(&active_order);

    let preflop_index_by_seat = order_index_map(&preflop_order);
    let postflop_index_by_seat = order_index_map(&postflop_order);

    let mut positions = active_order
        .into_iter()
        .zip(position_codes)
        .map(|(seat_no, position_code)| HandPosition {
            seat_no,
            position_code,
            preflop_act_order_index: preflop_index_by_seat[&seat_no],
            postflop_act_order_index: postflop_index_by_seat[&seat_no],
        })
        .collect::<Vec<_>>();

    positions.sort_by_key(|position| position.seat_no);
    Ok(positions)
}

fn clockwise_active_order(
    max_seats: u8,
    button_seat: u8,
    seat_active: &BTreeMap<u8, bool>,
) -> Vec<u8> {
    let mut ordered = Vec::new();
    let mut seat_no = button_seat;
    let mut seen = BTreeSet::new();

    loop {
        if seat_active.get(&seat_no).copied().unwrap_or(false) && seen.insert(seat_no) {
            ordered.push(seat_no);
        }

        seat_no = if seat_no == max_seats { 1 } else { seat_no + 1 };
        if seat_no == button_seat {
            break;
        }
    }

    ordered
}

fn preflop_action_order(active_order: &[u8]) -> Vec<u8> {
    if active_order.len() == 2 {
        return vec![active_order[0], active_order[1]];
    }

    rotate_left(active_order, 3)
}

fn postflop_action_order(active_order: &[u8]) -> Vec<u8> {
    if active_order.len() == 2 {
        return vec![active_order[1], active_order[0]];
    }

    rotate_left(active_order, 1)
}

fn rotate_left(active_order: &[u8], rotation: usize) -> Vec<u8> {
    let len = active_order.len();
    (0..len)
        .map(|offset| active_order[(rotation + offset) % len])
        .collect()
}

fn order_index_map(order: &[u8]) -> BTreeMap<u8, u8> {
    order
        .iter()
        .enumerate()
        .map(|(index, seat_no)| (*seat_no, (index + 1) as u8))
        .collect()
}

fn position_codes_for_active_count(
    active_count: usize,
) -> Result<Vec<PositionCode>, PositionError> {
    let codes = match active_count {
        2 => vec![PositionCode::Btn, PositionCode::Bb],
        3 => vec![PositionCode::Btn, PositionCode::Sb, PositionCode::Bb],
        4 => vec![
            PositionCode::Btn,
            PositionCode::Sb,
            PositionCode::Bb,
            PositionCode::Co,
        ],
        5 => vec![
            PositionCode::Btn,
            PositionCode::Sb,
            PositionCode::Bb,
            PositionCode::Hj,
            PositionCode::Co,
        ],
        6 => vec![
            PositionCode::Btn,
            PositionCode::Sb,
            PositionCode::Bb,
            PositionCode::Lj,
            PositionCode::Hj,
            PositionCode::Co,
        ],
        7 => vec![
            PositionCode::Btn,
            PositionCode::Sb,
            PositionCode::Bb,
            PositionCode::Mp,
            PositionCode::Lj,
            PositionCode::Hj,
            PositionCode::Co,
        ],
        8 => vec![
            PositionCode::Btn,
            PositionCode::Sb,
            PositionCode::Bb,
            PositionCode::UtgPlus1,
            PositionCode::Mp,
            PositionCode::Lj,
            PositionCode::Hj,
            PositionCode::Co,
        ],
        9 => vec![
            PositionCode::Btn,
            PositionCode::Sb,
            PositionCode::Bb,
            PositionCode::Utg,
            PositionCode::UtgPlus1,
            PositionCode::Mp,
            PositionCode::Lj,
            PositionCode::Hj,
            PositionCode::Co,
        ],
        _ => {
            return Err(PositionError::UnsupportedActiveSeatCount {
                active_seats: active_count,
            });
        }
    };

    Ok(codes)
}
