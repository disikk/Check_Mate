use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ParserError,
    models::{CanonicalParsedHand, CertaintyState, Street},
};

pub const STREET_HAND_STRENGTH_NUT_POLICY: &str = "hand_and_draw";

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
pub enum MadeHandCategory {
    HighCard,
    BoardPairOnly,
    Underpair,
    ThirdPair,
    SecondPair,
    TopPairWeak,
    TopPairGood,
    TopPairTop,
    Overpair,
    TwoPair,
    Set,
    Trips,
    Straight,
    Flush,
    FullHouse,
    Quads,
    StraightFlush,
}

impl MadeHandCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HighCard => "high_card",
            Self::BoardPairOnly => "board_pair_only",
            Self::Underpair => "underpair",
            Self::ThirdPair => "third_pair",
            Self::SecondPair => "second_pair",
            Self::TopPairWeak => "top_pair_weak",
            Self::TopPairGood => "top_pair_good",
            Self::TopPairTop => "top_pair_top",
            Self::Overpair => "overpair",
            Self::TwoPair => "two_pair",
            Self::Set => "set",
            Self::Trips => "trips",
            Self::Straight => "straight",
            Self::Flush => "flush",
            Self::FullHouse => "full_house",
            Self::Quads => "quads",
            Self::StraightFlush => "straight_flush",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawCategory {
    None,
    BackdoorFlushOnly,
    Gutshot,
    OpenEnded,
    DoubleGutshot,
    FlushDraw,
    ComboDraw,
}

impl DrawCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::BackdoorFlushOnly => "backdoor_flush_only",
            Self::Gutshot => "gutshot",
            Self::OpenEnded => "open_ended",
            Self::DoubleGutshot => "double_gutshot",
            Self::FlushDraw => "flush_draw",
            Self::ComboDraw => "combo_draw",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreetHandStrength {
    pub seat_no: u8,
    pub street: Street,
    pub best_hand_class: BestHandClass,
    pub best_hand_rank_value: i64,
    pub made_hand_category: MadeHandCategory,
    pub draw_category: DrawCategory,
    pub overcards_count: u8,
    pub has_air: bool,
    pub missed_flush_draw: bool,
    pub missed_straight_draw: bool,
    pub is_nut_hand: Option<bool>,
    pub is_nut_draw: Option<bool>,
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
struct StreetSignals {
    draw_category: DrawCategory,
    overcards_count: u8,
    has_air: bool,
    is_nut_draw: bool,
    has_frontdoor_flush_draw: bool,
    has_player_specific_straight_draw: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ImprovingNextCardOuts {
    straight_out_cards: Vec<Card>,
    flush_out_cards: Vec<Card>,
}

impl ImprovingNextCardOuts {
    fn straight_completion_ranks(&self) -> BTreeSet<u8> {
        self.straight_out_cards
            .iter()
            .map(|card| card.rank)
            .collect::<BTreeSet<_>>()
    }

    fn has_flush_out(&self) -> bool {
        !self.flush_out_cards.is_empty()
    }
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
        let mut had_frontdoor_flush_draw = false;
        let mut had_player_specific_straight_draw = false;
        for street in &streets {
            let street_board = street_board_cards(&board_cards, *street);
            let all_cards = all_cards(&seat.cards, &street_board);
            let evaluated = evaluate_best_hand(&all_cards);
            let made_hand_category = made_hand_category_for_street(
                &seat.cards,
                &street_board,
                evaluated.best_hand_class,
            );
            let street_signals =
                build_street_signals(&seat.cards, &street_board, &evaluated, *street);

            if *street != Street::River {
                had_frontdoor_flush_draw |= street_signals.has_frontdoor_flush_draw;
                had_player_specific_straight_draw |=
                    street_signals.has_player_specific_straight_draw;
            }

            seat_rows.push(StreetHandStrength {
                seat_no: seat.seat_no,
                street: *street,
                best_hand_class: evaluated.best_hand_class,
                best_hand_rank_value: evaluated.best_hand_rank_value,
                made_hand_category,
                draw_category: street_signals.draw_category,
                overcards_count: street_signals.overcards_count,
                has_air: street_signals.has_air,
                missed_flush_draw: false,
                missed_straight_draw: false,
                is_nut_hand: Some(is_nut_hand_on_board(&seat.cards, &street_board, &evaluated)),
                is_nut_draw: Some(street_signals.is_nut_draw),
                certainty_state: CertaintyState::Exact,
            });
        }

        if let Some(river_row) = seat_rows.iter_mut().find(|row| row.street == Street::River) {
            let flush_completed = matches!(
                river_row.best_hand_class,
                BestHandClass::Flush | BestHandClass::StraightFlush
            );
            let straight_completed = matches!(
                river_row.best_hand_class,
                BestHandClass::Straight | BestHandClass::StraightFlush
            );
            river_row.missed_flush_draw = had_frontdoor_flush_draw && !flush_completed;
            river_row.missed_straight_draw =
                had_player_specific_straight_draw && !straight_completed;
        }

        rows.extend(seat_rows);
    }

    rows.sort_by_key(|row| (row.seat_no, street_order(row.street)));
    Ok(rows)
}

pub(crate) fn evaluate_river_showdown_ranks(
    hand: &CanonicalParsedHand,
) -> Result<BTreeMap<String, i64>, ParserError> {
    let settlement_hand = settlement_hand_with_summary_showdowns(hand);
    let board_cards = final_board_cards(&settlement_hand)?;
    if board_cards.len() < 5 {
        return Ok(BTreeMap::new());
    }

    let known_seats = known_seat_cards(&settlement_hand)?;
    let player_by_seat = settlement_hand
        .seats
        .iter()
        .map(|seat| (seat.seat_no, seat.player_name.clone()))
        .collect::<BTreeMap<_, _>>();
    let river_board = street_board_cards(&board_cards, Street::River);
    let mut showdown_ranks = BTreeMap::new();

    for (seat_no, seat_context) in known_seats {
        if let Some(player_name) = player_by_seat.get(&seat_no) {
            let all_cards = all_cards(&seat_context.cards, &river_board);
            let evaluated = evaluate_best_hand(&all_cards);
            showdown_ranks.insert(player_name.clone(), evaluated.best_hand_rank_value);
        }
    }

    Ok(showdown_ranks)
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

fn settlement_hand_with_summary_showdowns(hand: &CanonicalParsedHand) -> CanonicalParsedHand {
    let mut settlement_hand = hand.clone();
    for outcome in &hand.summary_seat_outcomes {
        let summary_shows_cards = matches!(
            outcome.outcome_kind,
            crate::models::SummarySeatOutcomeKind::ShowedWon
                | crate::models::SummarySeatOutcomeKind::ShowedLost
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

    settlement_hand
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
    let mut counts = rank_counts(&ranks).into_iter().collect::<Vec<(u8, u8)>>();
    counts.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| right.0.cmp(&left.0)));

    let (best_hand_class, kickers) = if let Some(straight_high) = straight_high.filter(|_| is_flush)
    {
        (BestHandClass::StraightFlush, vec![straight_high])
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
        (
            BestHandClass::TwoPair,
            vec![high_pair, low_pair, counts[2].0],
        )
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

fn build_street_signals(
    hole_cards: &[Card; 2],
    board: &[Card],
    evaluated: &EvaluatedHand,
    street: Street,
) -> StreetSignals {
    let improving_outs =
        improving_next_card_outs(hole_cards, board, evaluated.best_hand_class, street);
    let flush_draw = improving_outs.has_flush_out();
    let backdoor_flush_draw = has_backdoor_flush_draw(
        hole_cards,
        board,
        evaluated.best_hand_class,
        street,
        flush_draw,
    );
    let completion_ranks = improving_outs.straight_completion_ranks();
    let open_ended = !is_straight_family(evaluated.best_hand_class)
        && completion_ranks.len() >= 2
        && has_player_specific_four_consecutive_run(hole_cards, board);
    let double_gutshot = !is_straight_family(evaluated.best_hand_class)
        && completion_ranks.len() >= 2
        && !open_ended;
    let gutshot = !is_straight_family(evaluated.best_hand_class) && completion_ranks.len() == 1;
    let has_player_specific_straight_draw = open_ended || gutshot || double_gutshot;
    let draw_category = if flush_draw && has_player_specific_straight_draw {
        DrawCategory::ComboDraw
    } else if flush_draw {
        DrawCategory::FlushDraw
    } else if double_gutshot {
        DrawCategory::DoubleGutshot
    } else if open_ended {
        DrawCategory::OpenEnded
    } else if gutshot {
        DrawCategory::Gutshot
    } else if backdoor_flush_draw {
        DrawCategory::BackdoorFlushOnly
    } else {
        DrawCategory::None
    };
    let nut_flush_family = is_nut_out_family(hole_cards, board, &improving_outs.flush_out_cards);
    let nut_straight_family =
        is_nut_out_family(hole_cards, board, &improving_outs.straight_out_cards);
    let is_nut_draw = match draw_category {
        DrawCategory::FlushDraw => nut_flush_family,
        DrawCategory::Gutshot | DrawCategory::OpenEnded | DrawCategory::DoubleGutshot => {
            nut_straight_family
        }
        DrawCategory::ComboDraw => nut_flush_family || nut_straight_family,
        DrawCategory::None | DrawCategory::BackdoorFlushOnly => false,
    };
    let max_board_rank = board
        .iter()
        .map(|board_card| board_card.rank)
        .max()
        .unwrap_or(0);
    let overcards_count = if evaluated.best_hand_class == BestHandClass::HighCard {
        hole_cards
            .iter()
            .filter(|card| card.rank > max_board_rank)
            .count() as u8
    } else {
        0
    };
    let has_air = evaluated.best_hand_class == BestHandClass::HighCard
        && !flush_draw
        && !has_player_specific_straight_draw
        && overcards_count == 0;

    StreetSignals {
        draw_category,
        overcards_count,
        has_air,
        is_nut_draw,
        has_frontdoor_flush_draw: flush_draw,
        has_player_specific_straight_draw,
    }
}

fn improving_next_card_outs(
    hole_cards: &[Card; 2],
    board: &[Card],
    best_hand_class: BestHandClass,
    street: Street,
) -> ImprovingNextCardOuts {
    if matches!(street, Street::River) {
        return ImprovingNextCardOuts::default();
    }

    let current_cards = all_cards(hole_cards, board);
    let mut straight_out_cards = Vec::new();
    let mut flush_out_cards = Vec::new();

    for next_card in legal_unseen_next_cards(&current_cards) {
        let mut next_state_cards = current_cards.clone();
        next_state_cards.push(next_card);
        let next_evaluated = evaluate_best_hand(&next_state_cards);

        if next_evaluated.best_hand_class.rank_code() <= best_hand_class.rank_code() {
            continue;
        }

        if next_evaluated.best_hand_class == BestHandClass::Straight
            && best_combo_uses_hole_card(
                &next_state_cards,
                hole_cards,
                BestHandClass::Straight,
                next_evaluated.best_hand_rank_value,
            )
        {
            straight_out_cards.push(next_card);
        }

        if next_evaluated.best_hand_class == BestHandClass::Flush
            && best_combo_uses_hole_card(
                &next_state_cards,
                hole_cards,
                BestHandClass::Flush,
                next_evaluated.best_hand_rank_value,
            )
        {
            flush_out_cards.push(next_card);
        }
    }

    ImprovingNextCardOuts {
        straight_out_cards,
        flush_out_cards,
    }
}

fn has_backdoor_flush_draw(
    hole_cards: &[Card; 2],
    board: &[Card],
    best_hand_class: BestHandClass,
    street: Street,
    has_frontdoor_flush_draw: bool,
) -> bool {
    if !matches!(street, Street::Flop)
        || is_flush_family(best_hand_class)
        || has_frontdoor_flush_draw
    {
        return false;
    }

    let current_cards = all_cards(hole_cards, board);
    let suited_counts = suited_hole_counts(hole_cards, board)
        .into_iter()
        .filter_map(|(suit, (hole_count, total_count))| {
            (hole_count > 0 && total_count == 3).then_some(suit)
        })
        .collect::<Vec<_>>();

    suited_counts.into_iter().any(|target_suit| {
        let suited_unseen_cards = legal_unseen_next_cards(&current_cards)
            .into_iter()
            .filter(|card| card.suit == target_suit)
            .collect::<Vec<_>>();

        for turn_index in 0..suited_unseen_cards.len() {
            for river_index in turn_index + 1..suited_unseen_cards.len() {
                let mut river_state_cards = current_cards.clone();
                river_state_cards.push(suited_unseen_cards[turn_index]);
                river_state_cards.push(suited_unseen_cards[river_index]);

                let river_evaluated = evaluate_best_hand(&river_state_cards);
                if river_evaluated.best_hand_class.rank_code() <= best_hand_class.rank_code() {
                    continue;
                }

                if matches!(
                    river_evaluated.best_hand_class,
                    BestHandClass::Flush | BestHandClass::StraightFlush
                ) && best_combo_uses_hole_card(
                    &river_state_cards,
                    hole_cards,
                    river_evaluated.best_hand_class,
                    river_evaluated.best_hand_rank_value,
                ) {
                    return true;
                }
            }
        }

        false
    })
}

fn suited_hole_counts(hole_cards: &[Card; 2], board: &[Card]) -> BTreeMap<Suit, (usize, usize)> {
    let mut counts = BTreeMap::new();
    for suit in [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades] {
        let hole_count = hole_cards.iter().filter(|card| card.suit == suit).count();
        let total_count = hole_count + board.iter().filter(|card| card.suit == suit).count();
        counts.insert(suit, (hole_count, total_count));
    }
    counts
}

fn legal_unseen_next_cards(current_cards: &[Card]) -> Vec<Card> {
    full_deck()
        .into_iter()
        .filter(|candidate| !current_cards.contains(candidate))
        .collect()
}

fn legal_opponent_hole_card_combinations(board: &[Card], hole_cards: &[Card; 2]) -> Vec<[Card; 2]> {
    let mut dead_cards = board.to_vec();
    dead_cards.extend(hole_cards.iter().copied());
    let available_cards = full_deck()
        .into_iter()
        .filter(|candidate| !dead_cards.contains(candidate))
        .collect::<Vec<_>>();
    let mut combinations = Vec::new();

    for first_index in 0..available_cards.len().saturating_sub(1) {
        for second_index in first_index + 1..available_cards.len() {
            combinations.push([available_cards[first_index], available_cards[second_index]]);
        }
    }

    combinations
}

fn is_nut_hand_on_board(
    hole_cards: &[Card; 2],
    board: &[Card],
    our_evaluated: &EvaluatedHand,
) -> bool {
    for opponent_hole_cards in legal_opponent_hole_card_combinations(board, hole_cards) {
        let opponent_cards = all_cards(&opponent_hole_cards, board);
        let opponent_evaluated = evaluate_best_hand(&opponent_cards);

        if opponent_evaluated.best_hand_rank_value > our_evaluated.best_hand_rank_value {
            return false;
        }
    }

    true
}

fn is_nut_out_family(hole_cards: &[Card; 2], board: &[Card], out_cards: &[Card]) -> bool {
    !out_cards.is_empty()
        && out_cards
            .iter()
            .all(|out_card| out_results_in_nut_hand(hole_cards, board, *out_card))
}

fn out_results_in_nut_hand(hole_cards: &[Card; 2], board: &[Card], out_card: Card) -> bool {
    let mut resulting_board = board.to_vec();
    resulting_board.push(out_card);
    let resulting_cards = all_cards(hole_cards, &resulting_board);
    let resulting_evaluated = evaluate_best_hand(&resulting_cards);

    is_nut_hand_on_board(hole_cards, &resulting_board, &resulting_evaluated)
}

fn full_deck() -> Vec<Card> {
    let mut deck = Vec::with_capacity(52);
    for suit in [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades] {
        for rank in 2_u8..=14_u8 {
            deck.push(Card { rank, suit });
        }
    }
    deck
}

fn best_combo_uses_hole_card(
    cards: &[Card],
    hole_cards: &[Card; 2],
    expected_class: BestHandClass,
    expected_rank_value: i64,
) -> bool {
    five_card_combinations(cards).into_iter().any(|combo| {
        let evaluated = evaluate_five_card_hand(&combo);
        evaluated.best_hand_class == expected_class
            && evaluated.best_hand_rank_value == expected_rank_value
            && combo.iter().any(|card| hole_cards.contains(card))
    })
}

fn has_player_specific_four_consecutive_run(hole_cards: &[Card; 2], board: &[Card]) -> bool {
    let rank_set = mirrored_rank_set(
        &all_cards(hole_cards, board)
            .iter()
            .map(|card| card.rank)
            .collect::<Vec<_>>(),
    );
    let hole_rank_set =
        mirrored_rank_set(&hole_cards.iter().map(|card| card.rank).collect::<Vec<_>>());
    (1_u8..=11_u8).any(|start| (start..=start + 3).all(|rank| rank_set.contains(&rank)))
        && (1_u8..=11_u8).any(|start| {
            let mut window = start..=start + 3;
            window.clone().all(|rank| rank_set.contains(&rank))
                && window.any(|rank| hole_rank_set.contains(&rank))
        })
}

fn made_hand_category_for_street(
    hole_cards: &[Card; 2],
    board: &[Card],
    best_hand_class: BestHandClass,
) -> MadeHandCategory {
    let board_ranks = board.iter().map(|card| card.rank).collect::<Vec<_>>();
    let hole_ranks = [hole_cards[0].rank, hole_cards[1].rank];
    let board_counts = rank_counts(&board_ranks);
    let mut board_unique_desc = board_ranks.clone();
    board_unique_desc.sort_unstable_by(|left, right| right.cmp(left));
    board_unique_desc.dedup();
    let top_board_rank = *board_unique_desc.first().unwrap_or(&0);

    match best_hand_class {
        BestHandClass::HighCard => MadeHandCategory::HighCard,
        BestHandClass::Straight => MadeHandCategory::Straight,
        BestHandClass::Flush => MadeHandCategory::Flush,
        BestHandClass::FullHouse => MadeHandCategory::FullHouse,
        BestHandClass::Quads => MadeHandCategory::Quads,
        BestHandClass::StraightFlush => MadeHandCategory::StraightFlush,
        BestHandClass::TwoPair => MadeHandCategory::TwoPair,
        BestHandClass::Trips => {
            if hole_ranks[0] == hole_ranks[1] && board_counts.contains_key(&hole_ranks[0]) {
                MadeHandCategory::Set
            } else {
                MadeHandCategory::Trips
            }
        }
        BestHandClass::Pair => {
            if hole_ranks[0] == hole_ranks[1] {
                if hole_ranks[0] > top_board_rank {
                    MadeHandCategory::Overpair
                } else {
                    MadeHandCategory::Underpair
                }
            } else {
                let paired_rank = hole_ranks
                    .into_iter()
                    .find(|rank| board_counts.contains_key(rank));

                match paired_rank {
                    Some(rank) if rank == top_board_rank => top_pair_category(hole_ranks, rank),
                    Some(rank) if board_unique_desc.get(1).copied() == Some(rank) => {
                        MadeHandCategory::SecondPair
                    }
                    Some(_) => MadeHandCategory::ThirdPair,
                    None => MadeHandCategory::BoardPairOnly,
                }
            }
        }
    }
}

fn top_pair_category(hole_ranks: [u8; 2], paired_rank: u8) -> MadeHandCategory {
    let kicker = hole_ranks
        .into_iter()
        .find(|rank| *rank != paired_rank)
        .unwrap_or(paired_rank);
    let top_possible_kicker = if paired_rank == 14 { 13 } else { 14 };

    if kicker == top_possible_kicker {
        MadeHandCategory::TopPairTop
    } else if kicker >= 10 {
        MadeHandCategory::TopPairGood
    } else {
        MadeHandCategory::TopPairWeak
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::parsers::hand_history::parse_canonical_hand;

    #[test]
    fn evaluate_river_showdown_ranks_returns_only_river_showdown_player_ranks() {
        let hand = parse_canonical_hand(&showdown_hand_with_summary_reveal()).unwrap();

        let ranks = evaluate_river_showdown_ranks(&hand).unwrap();
        let expected = expected_river_showdown_ranks(&hand).unwrap();
        assert_eq!(ranks, expected);
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

    fn expected_river_showdown_ranks(
        hand: &CanonicalParsedHand,
    ) -> Result<BTreeMap<String, i64>, ParserError> {
        let settlement_hand = settlement_hand_with_summary_showdowns(hand);
        let board_cards = final_board_cards(&settlement_hand)?;
        let known_seats = known_seat_cards(&settlement_hand)?;
        let player_by_seat = settlement_hand
            .seats
            .iter()
            .map(|seat| (seat.seat_no, seat.player_name.clone()))
            .collect::<BTreeMap<_, _>>();
        let river_board = street_board_cards(&board_cards, Street::River);
        let mut ranks = BTreeMap::new();

        for (seat_no, seat_context) in known_seats {
            if let Some(player_name) = player_by_seat.get(&seat_no) {
                let all_cards = all_cards(&seat_context.cards, &river_board);
                let evaluated = evaluate_best_hand(&all_cards);
                ranks.insert(player_name.clone(), evaluated.best_hand_rank_value);
            }
        }

        Ok(ranks)
    }
}
