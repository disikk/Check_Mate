use std::collections::BTreeMap;

use crate::{
    ParserError,
    models::{CanonicalParsedHand, CertaintyState},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflopStartingHand {
    pub seat_no: u8,
    pub starter_hand_class: String,
    pub certainty_state: CertaintyState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedCard {
    rank: u8,
    suit: char,
}

pub fn canonical_starting_hand_class(card1: &str, card2: &str) -> Result<String, ParserError> {
    let first = parse_card(card1)?;
    let second = parse_card(card2)?;
    let (high, low) = if first.rank >= second.rank {
        (first, second)
    } else {
        (second, first)
    };

    if high.rank == low.rank {
        return Ok(format!("{}{}", rank_code(high.rank), rank_code(low.rank)));
    }

    let suitedness = if high.suit == low.suit { 's' } else { 'o' };
    Ok(format!(
        "{}{}{}",
        rank_code(high.rank),
        rank_code(low.rank),
        suitedness
    ))
}

pub fn evaluate_preflop_starting_hands(
    hand: &CanonicalParsedHand,
) -> Result<Vec<PreflopStartingHand>, ParserError> {
    let seat_by_player = hand
        .seats
        .iter()
        .map(|seat| (seat.player_name.as_str(), seat.seat_no))
        .collect::<BTreeMap<_, _>>();

    let mut rows = BTreeMap::<u8, PreflopStartingHand>::new();

    if let (Some(hero_name), Some(hero_hole_cards)) =
        (hand.hero_name.as_deref(), hand.hero_hole_cards.as_deref())
        && let Some((card1, card2)) = exact_two_cards(hero_hole_cards)
        && let Some(&seat_no) = seat_by_player.get(hero_name)
    {
        rows.insert(
            seat_no,
            PreflopStartingHand {
                seat_no,
                starter_hand_class: canonical_starting_hand_class(card1, card2)?,
                certainty_state: CertaintyState::Exact,
            },
        );
    }

    for (player_name, cards) in &hand.showdown_hands {
        let Some((card1, card2)) = exact_two_cards(cards) else {
            continue;
        };
        let Some(&seat_no) = seat_by_player.get(player_name.as_str()) else {
            continue;
        };
        rows.insert(
            seat_no,
            PreflopStartingHand {
                seat_no,
                starter_hand_class: canonical_starting_hand_class(card1, card2)?,
                certainty_state: CertaintyState::Exact,
            },
        );
    }

    Ok(rows.into_values().collect())
}

fn exact_two_cards(cards: &[String]) -> Option<(&str, &str)> {
    match cards {
        [card1, card2] => Some((card1.as_str(), card2.as_str())),
        _ => None,
    }
}

fn rank_code(rank: u8) -> char {
    match rank {
        2..=9 => char::from(b'0' + rank),
        10 => 'T',
        11 => 'J',
        12 => 'Q',
        13 => 'K',
        14 => 'A',
        _ => unreachable!("card ranks are validated in parse_card"),
    }
}

fn parse_card(raw: &str) -> Result<ParsedCard, ParserError> {
    let mut chars = raw.chars();
    let rank_char = chars.next().ok_or_else(|| ParserError::InvalidField {
        field: "preflop_starting_hand_card",
        value: raw.to_string(),
    })?;
    let suit_char = chars.next().ok_or_else(|| ParserError::InvalidField {
        field: "preflop_starting_hand_card",
        value: raw.to_string(),
    })?;
    if chars.next().is_some() {
        return Err(ParserError::InvalidField {
            field: "preflop_starting_hand_card",
            value: raw.to_string(),
        });
    }

    let rank = match rank_char {
        '2'..='9' => rank_char.to_digit(10).expect("digit rank") as u8,
        'T' => 10,
        'J' => 11,
        'Q' => 12,
        'K' => 13,
        'A' => 14,
        _ => {
            return Err(ParserError::InvalidField {
                field: "preflop_starting_hand_rank",
                value: raw.to_string(),
            });
        }
    };

    match suit_char {
        'c' | 'd' | 'h' | 's' => Ok(ParsedCard {
            rank,
            suit: suit_char,
        }),
        _ => Err(ParserError::InvalidField {
            field: "preflop_starting_hand_suit",
            value: raw.to_string(),
        }),
    }
}
