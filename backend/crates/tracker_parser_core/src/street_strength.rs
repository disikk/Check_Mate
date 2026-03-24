use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ParserError,
    models::{CanonicalParsedHand, CertaintyState, Street},
};

pub const STREET_HAND_STRENGTH_VERSION: &str = "gg_mbr_street_strength_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BestHandClass {
    HighCard,
    Pair,
    TwoPair,
    Trips,
    Straight,
    Flush,
    FullHouse,
    Quads,
    StraightFlush,
}

impl BestHandClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HighCard => "high_card",
            Self::Pair => "pair",
            Self::TwoPair => "two_pair",
            Self::Trips => "trips",
            Self::Straight => "straight",
            Self::Flush => "flush",
            Self::FullHouse => "full_house",
            Self::Quads => "quads",
            Self::StraightFlush => "straight_flush",
        }
    }

    fn rank_code(self) -> i64 {
        match self {
            Self::HighCard => 0,
            Self::Pair => 1,
            Self::TwoPair => 2,
            Self::Trips => 3,
            Self::Straight => 4,
            Self::Flush => 5,
            Self::FullHouse => 6,
            Self::Quads => 7,
            Self::StraightFlush => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairStrength {
    None,
    BoardPair,
    Underpair,
    BottomPair,
    MiddlePair,
    TopPairAceKicker,
    TopPairBroadwayKicker,
    TopPairWeakKicker,
    Overpair,
    Trips,
    Set,
}

impl PairStrength {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::BoardPair => "board_pair",
            Self::Underpair => "underpair",
            Self::BottomPair => "bottom_pair",
            Self::MiddlePair => "middle_pair",
            Self::TopPairAceKicker => "top_pair_ace_kicker",
            Self::TopPairBroadwayKicker => "top_pair_broadway_kicker",
            Self::TopPairWeakKicker => "top_pair_weak_kicker",
            Self::Overpair => "overpair",
            Self::Trips => "trips",
            Self::Set => "set",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreetHandStrength {
    pub seat_no: u8,
    pub street: Street,
    pub best_hand_class: BestHandClass,
    pub best_hand_rank_value: i64,
    pub pair_strength: PairStrength,
    pub is_nut_hand: Option<bool>,
    pub is_nut_draw: Option<bool>,
    pub has_flush_draw: bool,
    pub has_backdoor_flush_draw: bool,
    pub has_open_ended: bool,
    pub has_gutshot: bool,
    pub has_double_gutshot: bool,
    pub has_pair_plus_draw: bool,
    pub has_overcards: bool,
    pub has_air: bool,
    pub has_missed_draw_by_river: bool,
    pub descriptor_version: &'static str,
    pub certainty_state: CertaintyState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Card {
    rank: u8,
    suit: Suit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeatDescriptorContext {
    seat_no: u8,
    cards: [Card; 2],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreetFlags {
    has_flush_draw: bool,
    has_backdoor_flush_draw: bool,
    has_open_ended: bool,
    has_gutshot: bool,
    has_double_gutshot: bool,
    has_pair_plus_draw: bool,
    has_overcards: bool,
    has_air: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EvaluatedHand {
    best_hand_class: BestHandClass,
    best_hand_rank_value: i64,
}

pub fn evaluate_street_hand_strength(
    hand: &CanonicalParsedHand,
) -> Result<Vec<StreetHandStrength>, ParserError> {
    let board_cards = final_board_cards(hand)?;
    if board_cards.len() < 3 {
        return Ok(Vec::new());
    }

    let known_seats = known_seat_cards(hand)?;
    let streets = reached_streets(board_cards.len());
    let mut rows = Vec::new();

    for seat in known_seats.values() {
        let mut seat_rows = Vec::new();
        for street in &streets {
            let street_board = street_board_cards(&board_cards, *street);
            let all_cards = all_cards(&seat.cards, &street_board);
            let evaluated = evaluate_best_hand(&all_cards);
            let pair_strength =
                pair_strength_for_street(&seat.cards, &street_board, evaluated.best_hand_class);
            let draw_flags = build_draw_flags(&seat.cards, &street_board, &evaluated, *street);

            seat_rows.push(StreetHandStrength {
                seat_no: seat.seat_no,
                street: *street,
                best_hand_class: evaluated.best_hand_class,
                best_hand_rank_value: evaluated.best_hand_rank_value,
                pair_strength,
                is_nut_hand: None,
                is_nut_draw: None,
                has_flush_draw: draw_flags.has_flush_draw,
                has_backdoor_flush_draw: draw_flags.has_backdoor_flush_draw,
                has_open_ended: draw_flags.has_open_ended,
                has_gutshot: draw_flags.has_gutshot,
                has_double_gutshot: draw_flags.has_double_gutshot,
                has_pair_plus_draw: draw_flags.has_pair_plus_draw,
                has_overcards: draw_flags.has_overcards,
                has_air: draw_flags.has_air,
                has_missed_draw_by_river: false,
                descriptor_version: STREET_HAND_STRENGTH_VERSION,
                certainty_state: CertaintyState::Exact,
            });
        }

        let had_flush_family_draw = seat_rows
            .iter()
            .take_while(|row| row.street != Street::River)
            .any(|row| row.has_flush_draw || row.has_backdoor_flush_draw);
        let had_straight_family_draw = seat_rows
            .iter()
            .take_while(|row| row.street != Street::River)
            .any(|row| row.has_open_ended || row.has_gutshot || row.has_double_gutshot);

        if let Some(river_row) = seat_rows.iter_mut().find(|row| row.street == Street::River) {
            let flush_completed = matches!(
                river_row.best_hand_class,
                BestHandClass::Flush | BestHandClass::StraightFlush
            );
            let straight_completed = matches!(
                river_row.best_hand_class,
                BestHandClass::Straight | BestHandClass::StraightFlush
            );
            river_row.has_missed_draw_by_river =
                (had_flush_family_draw && !flush_completed)
                    || (had_straight_family_draw && !straight_completed);
        }

        rows.extend(seat_rows);
    }

    rows.sort_by_key(|row| (row.seat_no, street_order(row.street)));
    Ok(rows)
}

fn final_board_cards(hand: &CanonicalParsedHand) -> Result<Vec<Card>, ParserError> {
    let source = if !hand.board_final.is_empty() {
        &hand.board_final
    } else {
        &hand.summary_board
    };

    source.iter().map(|card| parse_card(card)).collect()
}

fn known_seat_cards(
    hand: &CanonicalParsedHand,
) -> Result<BTreeMap<u8, SeatDescriptorContext>, ParserError> {
    let seat_by_player = hand
        .seats
        .iter()
        .map(|seat| (seat.player_name.as_str(), seat.seat_no))
        .collect::<BTreeMap<_, _>>();
    let mut known = BTreeMap::new();

    if let (Some(hero_name), Some(hero_cards)) = (&hand.hero_name, &hand.hero_hole_cards)
        && let Some(seat_no) = seat_by_player.get(hero_name.as_str())
        && let Some(context) = build_seat_context(*seat_no, hero_cards)?
    {
        known.insert(*seat_no, context);
    }

    for (player_name, cards) in &hand.showdown_hands {
        if let Some(seat_no) = seat_by_player.get(player_name.as_str())
            && let Some(context) = build_seat_context(*seat_no, cards)?
        {
            known.insert(*seat_no, context);
        }
    }

    Ok(known)
}

fn build_seat_context(
    seat_no: u8,
    cards: &[String],
) -> Result<Option<SeatDescriptorContext>, ParserError> {
    if cards.len() != 2 {
        return Ok(None);
    }

    let parsed = cards
        .iter()
        .map(|card| parse_card(card))
        .collect::<Result<Vec<_>, _>>()?;
    let [card1, card2] = parsed.as_slice() else {
        unreachable!("two hole cards were required above");
    };

    Ok(Some(SeatDescriptorContext {
        seat_no,
        cards: [*card1, *card2],
    }))
}

fn reached_streets(board_len: usize) -> Vec<Street> {
    let mut streets = vec![Street::Flop];
    if board_len >= 4 {
        streets.push(Street::Turn);
    }
    if board_len >= 5 {
        streets.push(Street::River);
    }
    streets
}

fn street_board_cards(board: &[Card], street: Street) -> Vec<Card> {
    match street {
        Street::Flop => board.iter().take(3).copied().collect(),
        Street::Turn => board.iter().take(4).copied().collect(),
        Street::River => board.iter().take(5).copied().collect(),
        _ => Vec::new(),
    }
}

fn all_cards(hole_cards: &[Card; 2], board: &[Card]) -> Vec<Card> {
    let mut cards = hole_cards.to_vec();
    cards.extend(board.iter().copied());
    cards
}

fn evaluate_best_hand(cards: &[Card]) -> EvaluatedHand {
    let mut best = None;

    for combo in five_card_combinations(cards) {
        let evaluated = evaluate_five_card_hand(&combo);
        if best
            .as_ref()
            .map(|current: &EvaluatedHand| {
                evaluated.best_hand_rank_value > current.best_hand_rank_value
            })
            .unwrap_or(true)
        {
            best = Some(evaluated);
        }
    }

    best.expect("street-strength evaluator requires at least five cards")
}

fn five_card_combinations(cards: &[Card]) -> Vec<[Card; 5]> {
    let mut combinations = Vec::new();
    for a in 0..cards.len() - 4 {
        for b in a + 1..cards.len() - 3 {
            for c in b + 1..cards.len() - 2 {
                for d in c + 1..cards.len() - 1 {
                    for e in d + 1..cards.len() {
                        combinations.push([cards[a], cards[b], cards[c], cards[d], cards[e]]);
                    }
                }
            }
        }
    }
    combinations
}

fn evaluate_five_card_hand(cards: &[Card; 5]) -> EvaluatedHand {
    let mut ranks = cards.iter().map(|card| card.rank).collect::<Vec<_>>();
    ranks.sort_unstable_by(|left, right| right.cmp(left));

    let is_flush = cards
        .iter()
        .all(|card| card.suit == cards.first().expect("cards").suit);
    let straight_high = highest_straight_high(&ranks);
    let mut counts = rank_counts(&ranks)
        .into_iter()
        .collect::<Vec<(u8, u8)>>();
    counts.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| right.0.cmp(&left.0))
    });

    let (best_hand_class, kickers) = if is_flush && straight_high.is_some() {
        (
            BestHandClass::StraightFlush,
            vec![straight_high.expect("straight flush high card")],
        )
    } else if counts[0].1 == 4 {
        let kicker = counts[1].0;
        (BestHandClass::Quads, vec![counts[0].0, kicker])
    } else if counts[0].1 == 3 && counts[1].1 == 2 {
        (BestHandClass::FullHouse, vec![counts[0].0, counts[1].0])
    } else if is_flush {
        (BestHandClass::Flush, ranks.clone())
    } else if let Some(high_card) = straight_high {
        (BestHandClass::Straight, vec![high_card])
    } else if counts[0].1 == 3 {
        let mut ordered = vec![counts[0].0];
        ordered.extend(
            counts
                .iter()
                .skip(1)
                .map(|(rank, _)| *rank)
                .collect::<Vec<_>>(),
        );
        (BestHandClass::Trips, ordered)
    } else if counts[0].1 == 2 && counts[1].1 == 2 {
        let high_pair = counts[0].0.max(counts[1].0);
        let low_pair = counts[0].0.min(counts[1].0);
        (BestHandClass::TwoPair, vec![high_pair, low_pair, counts[2].0])
    } else if counts[0].1 == 2 {
        let mut ordered = vec![counts[0].0];
        ordered.extend(
            counts
                .iter()
                .skip(1)
                .map(|(rank, _)| *rank)
                .collect::<Vec<_>>(),
        );
        (BestHandClass::Pair, ordered)
    } else {
        (BestHandClass::HighCard, ranks.clone())
    };

    let best_hand_rank_value = encode_rank_value(best_hand_class.rank_code(), &kickers);

    EvaluatedHand {
        best_hand_class,
        best_hand_rank_value,
    }
}

fn build_draw_flags(
    hole_cards: &[Card; 2],
    board: &[Card],
    evaluated: &EvaluatedHand,
    street: Street,
) -> StreetFlags {
    let all_cards = all_cards(hole_cards, board);
    let flush_draw = has_flush_draw(hole_cards, board, evaluated.best_hand_class, street);
    let backdoor_flush_draw =
        has_backdoor_flush_draw(hole_cards, board, evaluated.best_hand_class, street);
    let completion_ranks = straight_completion_ranks(&all_cards);
    let open_ended = !is_straight_family(evaluated.best_hand_class)
        && completion_ranks.len() >= 2
        && has_four_consecutive_run(&all_cards);
    let double_gutshot = !is_straight_family(evaluated.best_hand_class)
        && completion_ranks.len() >= 2
        && !open_ended;
    let gutshot = !is_straight_family(evaluated.best_hand_class)
        && completion_ranks.len() == 1;
    let has_active_draw = flush_draw || backdoor_flush_draw || open_ended || gutshot || double_gutshot;
    let has_pair_family_value = matches!(
        evaluated.best_hand_class,
        BestHandClass::Pair | BestHandClass::TwoPair | BestHandClass::Trips | BestHandClass::FullHouse | BestHandClass::Quads
    );
    let overcards = evaluated.best_hand_class == BestHandClass::HighCard
        && hole_cards
            .iter()
            .all(|card| card.rank > board.iter().map(|board_card| board_card.rank).max().unwrap_or(0));
    let air = evaluated.best_hand_class == BestHandClass::HighCard && !has_active_draw && !overcards;

    StreetFlags {
        has_flush_draw: flush_draw,
        has_backdoor_flush_draw: backdoor_flush_draw,
        has_open_ended: open_ended,
        has_gutshot: gutshot,
        has_double_gutshot: double_gutshot,
        has_pair_plus_draw: has_pair_family_value && has_active_draw,
        has_overcards: overcards,
        has_air: air,
    }
}

fn has_flush_draw(
    hole_cards: &[Card; 2],
    board: &[Card],
    best_hand_class: BestHandClass,
    street: Street,
) -> bool {
    if matches!(street, Street::River) || is_flush_family(best_hand_class) {
        return false;
    }

    suited_hole_counts(hole_cards, board)
        .into_iter()
        .any(|(_, (hole_count, total_count))| hole_count > 0 && total_count == 4)
}

fn has_backdoor_flush_draw(
    hole_cards: &[Card; 2],
    board: &[Card],
    best_hand_class: BestHandClass,
    street: Street,
) -> bool {
    if !matches!(street, Street::Flop) || is_flush_family(best_hand_class) {
        return false;
    }

    let suited_counts = suited_hole_counts(hole_cards, board);
    let has_live_flush_draw = suited_counts
        .values()
        .any(|(hole_count, total_count)| *hole_count > 0 && *total_count == 4);

    !has_live_flush_draw
        && suited_counts
            .values()
            .any(|(hole_count, total_count)| *hole_count > 0 && *total_count == 3)
}

fn suited_hole_counts(
    hole_cards: &[Card; 2],
    board: &[Card],
) -> BTreeMap<Suit, (usize, usize)> {
    let mut counts = BTreeMap::new();
    for suit in [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades] {
        let hole_count = hole_cards.iter().filter(|card| card.suit == suit).count();
        let total_count = hole_count + board.iter().filter(|card| card.suit == suit).count();
        counts.insert(suit, (hole_count, total_count));
    }
    counts
}

fn straight_completion_ranks(cards: &[Card]) -> BTreeSet<u8> {
    if highest_straight_high(&cards.iter().map(|card| card.rank).collect::<Vec<_>>()).is_some() {
        return BTreeSet::new();
    }

    (2_u8..=14_u8)
        .filter(|candidate_rank| {
            let mut ranks = cards.iter().map(|card| card.rank).collect::<Vec<_>>();
            ranks.push(*candidate_rank);
            highest_straight_high(&ranks).is_some()
        })
        .collect()
}

fn has_four_consecutive_run(cards: &[Card]) -> bool {
    let rank_set = mirrored_rank_set(&cards.iter().map(|card| card.rank).collect::<Vec<_>>());
    (1_u8..=11_u8).any(|start| (start..=start + 3).all(|rank| rank_set.contains(&rank)))
}

fn pair_strength_for_street(
    hole_cards: &[Card; 2],
    board: &[Card],
    best_hand_class: BestHandClass,
) -> PairStrength {
    let board_ranks = board.iter().map(|card| card.rank).collect::<Vec<_>>();
    let hole_ranks = [hole_cards[0].rank, hole_cards[1].rank];
    let board_counts = rank_counts(&board_ranks);
    let mut board_unique_desc = board_ranks.clone();
    board_unique_desc.sort_unstable_by(|left, right| right.cmp(left));
    board_unique_desc.dedup();
    let top_board_rank = *board_unique_desc.first().unwrap_or(&0);
    let low_board_rank = *board_unique_desc.last().unwrap_or(&0);

    match best_hand_class {
        BestHandClass::Trips => {
            if hole_ranks[0] == hole_ranks[1] && board_counts.contains_key(&hole_ranks[0]) {
                PairStrength::Set
            } else {
                PairStrength::Trips
            }
        }
        BestHandClass::Pair => {
            if hole_ranks[0] == hole_ranks[1] {
                if hole_ranks[0] > top_board_rank {
                    PairStrength::Overpair
                } else {
                    PairStrength::Underpair
                }
            } else {
                let paired_rank = hole_ranks
                    .into_iter()
                    .find(|rank| board_counts.contains_key(rank));

                match paired_rank {
                    Some(rank) if rank == top_board_rank => {
                        let kicker = hole_ranks
                            .into_iter()
                            .find(|kicker| *kicker != rank)
                            .unwrap_or(rank);
                        match kicker {
                            14 => PairStrength::TopPairAceKicker,
                            10..=13 => PairStrength::TopPairBroadwayKicker,
                            _ => PairStrength::TopPairWeakKicker,
                        }
                    }
                    Some(rank) if rank == low_board_rank => PairStrength::BottomPair,
                    Some(_) => PairStrength::MiddlePair,
                    None => PairStrength::BoardPair,
                }
            }
        }
        _ => PairStrength::None,
    }
}

fn rank_counts(ranks: &[u8]) -> BTreeMap<u8, u8> {
    let mut counts = BTreeMap::new();
    for rank in ranks {
        *counts.entry(*rank).or_insert(0) += 1;
    }
    counts
}

fn highest_straight_high(ranks: &[u8]) -> Option<u8> {
    let rank_set = mirrored_rank_set(ranks);
    (1_u8..=10_u8)
        .filter(|start| (0_u8..5_u8).all(|offset| rank_set.contains(&(start + offset))))
        .map(|start| if start == 1 { 5 } else { start + 4 })
        .max()
}

fn mirrored_rank_set(ranks: &[u8]) -> BTreeSet<u8> {
    let mut rank_set = ranks.iter().copied().collect::<BTreeSet<_>>();
    if rank_set.contains(&14) {
        rank_set.insert(1);
    }
    rank_set
}

fn encode_rank_value(class_code: i64, kickers: &[u8]) -> i64 {
    let mut value = class_code;
    for index in 0..5 {
        value *= 15;
        value += kickers.get(index).copied().unwrap_or(0) as i64;
    }
    value
}

fn parse_card(raw: &str) -> Result<Card, ParserError> {
    let mut chars = raw.chars();
    let rank_char = chars.next().ok_or_else(|| ParserError::InvalidField {
        field: "street_strength_card",
        value: raw.to_string(),
    })?;
    let suit_char = chars.next().ok_or_else(|| ParserError::InvalidField {
        field: "street_strength_card",
        value: raw.to_string(),
    })?;
    if chars.next().is_some() {
        return Err(ParserError::InvalidField {
            field: "street_strength_card",
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
                field: "street_strength_rank",
                value: raw.to_string(),
            });
        }
    };
    let suit = match suit_char {
        'c' => Suit::Clubs,
        'd' => Suit::Diamonds,
        'h' => Suit::Hearts,
        's' => Suit::Spades,
        _ => {
            return Err(ParserError::InvalidField {
                field: "street_strength_suit",
                value: raw.to_string(),
            });
        }
    };

    Ok(Card { rank, suit })
}

fn street_order(street: Street) -> u8 {
    match street {
        Street::Flop => 0,
        Street::Turn => 1,
        Street::River => 2,
        Street::Preflop => 3,
        Street::Showdown => 4,
        Street::Summary => 5,
    }
}

fn is_straight_family(best_hand_class: BestHandClass) -> bool {
    matches!(
        best_hand_class,
        BestHandClass::Straight | BestHandClass::StraightFlush
    )
}

fn is_flush_family(best_hand_class: BestHandClass) -> bool {
    matches!(
        best_hand_class,
        BestHandClass::Flush | BestHandClass::StraightFlush
    )
}
