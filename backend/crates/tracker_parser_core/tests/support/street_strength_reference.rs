use std::collections::{BTreeMap, BTreeSet};

use tracker_parser_core::{
    models::{CanonicalParsedHand, Street},
    street_strength::{BestHandClass, DrawCategory},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TestCard {
    rank: u8,
    suit: TestSuit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TestSuit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EvaluatedHand {
    best_hand_class: BestHandClass,
    best_hand_rank_value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceStreetDrawRow {
    pub street: Street,
    pub draw_category: DrawCategory,
    pub missed_flush_draw: bool,
    pub missed_straight_draw: bool,
}

pub fn reference_draw_rows_for_seat(
    hand: &CanonicalParsedHand,
    seat_no: u8,
) -> Vec<ReferenceStreetDrawRow> {
    let Some(hole_cards) = known_hole_cards(hand, seat_no) else {
        return Vec::new();
    };
    let Ok(board_cards) = final_board_cards(hand) else {
        return Vec::new();
    };
    if board_cards.len() < 3 {
        return Vec::new();
    }

    let streets = reached_streets(board_cards.len());
    let mut rows = Vec::new();
    let mut had_frontdoor_flush_draw = false;
    let mut had_player_specific_straight_draw = false;

    for street in &streets {
        let street_board = street_board_cards(&board_cards, *street);
        let current_cards = all_cards(&hole_cards, &street_board);
        let evaluated = evaluate_best_hand(&current_cards);
        let draw_category = reference_draw_category(
            &hole_cards,
            &street_board,
            evaluated.best_hand_class,
            *street,
        );
        let has_frontdoor_flush_draw = matches!(
            draw_category,
            DrawCategory::FlushDraw | DrawCategory::ComboDraw
        );
        let has_player_specific_straight_draw = matches!(
            draw_category,
            DrawCategory::Gutshot
                | DrawCategory::OpenEnded
                | DrawCategory::DoubleGutshot
                | DrawCategory::ComboDraw
        );

        if *street != Street::River {
            had_frontdoor_flush_draw |= has_frontdoor_flush_draw;
            had_player_specific_straight_draw |= has_player_specific_straight_draw;
        }

        rows.push(ReferenceStreetDrawRow {
            street: *street,
            draw_category,
            missed_flush_draw: false,
            missed_straight_draw: false,
        });
    }

    if let Some(river_row) = rows.iter_mut().find(|row| row.street == Street::River) {
        let river_board = street_board_cards(&board_cards, Street::River);
        let river_evaluated = evaluate_best_hand(&all_cards(&hole_cards, &river_board));
        let flush_completed = matches!(
            river_evaluated.best_hand_class,
            BestHandClass::Flush | BestHandClass::StraightFlush
        );
        let straight_completed = matches!(
            river_evaluated.best_hand_class,
            BestHandClass::Straight | BestHandClass::StraightFlush
        );

        river_row.missed_flush_draw = had_frontdoor_flush_draw && !flush_completed;
        river_row.missed_straight_draw = had_player_specific_straight_draw && !straight_completed;
    }

    rows
}

fn reference_draw_category(
    hole_cards: &[TestCard; 2],
    board: &[TestCard],
    best_hand_class: BestHandClass,
    street: Street,
) -> DrawCategory {
    let improving_outs = improving_next_card_outs(hole_cards, board, best_hand_class, street);
    let open_ended = !is_straight_family(best_hand_class)
        && improving_outs.straight_completion_ranks.len() >= 2
        && has_player_specific_four_consecutive_run(hole_cards, board);
    let double_gutshot = !is_straight_family(best_hand_class)
        && improving_outs.straight_completion_ranks.len() >= 2
        && !open_ended;
    let gutshot =
        !is_straight_family(best_hand_class) && improving_outs.straight_completion_ranks.len() == 1;
    let has_straight_draw = open_ended || gutshot || double_gutshot;
    let backdoor_flush_only = has_backdoor_flush_only(
        hole_cards,
        board,
        best_hand_class,
        street,
        improving_outs.has_flush_out,
    );

    if improving_outs.has_flush_out && has_straight_draw {
        DrawCategory::ComboDraw
    } else if improving_outs.has_flush_out {
        DrawCategory::FlushDraw
    } else if double_gutshot {
        DrawCategory::DoubleGutshot
    } else if open_ended {
        DrawCategory::OpenEnded
    } else if gutshot {
        DrawCategory::Gutshot
    } else if backdoor_flush_only {
        DrawCategory::BackdoorFlushOnly
    } else {
        DrawCategory::None
    }
}

#[derive(Debug, Default)]
struct ImprovingNextCardOuts {
    straight_completion_ranks: BTreeSet<u8>,
    has_flush_out: bool,
}

fn improving_next_card_outs(
    hole_cards: &[TestCard; 2],
    board: &[TestCard],
    best_hand_class: BestHandClass,
    street: Street,
) -> ImprovingNextCardOuts {
    if matches!(street, Street::River) {
        return ImprovingNextCardOuts::default();
    }

    let current_cards = all_cards(hole_cards, board);
    let mut straight_completion_ranks = BTreeSet::new();
    let mut has_flush_out = false;

    for next_card in legal_unseen_next_cards(&current_cards) {
        let mut next_state = current_cards.clone();
        next_state.push(next_card);
        let next_evaluated = evaluate_best_hand(&next_state);
        if class_code(next_evaluated.best_hand_class) <= class_code(best_hand_class) {
            continue;
        }

        if next_evaluated.best_hand_class == BestHandClass::Straight
            && best_combo_uses_hole_card(
                &next_state,
                hole_cards,
                BestHandClass::Straight,
                next_evaluated.best_hand_rank_value,
            )
        {
            straight_completion_ranks.insert(next_card.rank);
        }

        if next_evaluated.best_hand_class == BestHandClass::Flush
            && best_combo_uses_hole_card(
                &next_state,
                hole_cards,
                BestHandClass::Flush,
                next_evaluated.best_hand_rank_value,
            )
        {
            has_flush_out = true;
        }
    }

    ImprovingNextCardOuts {
        straight_completion_ranks,
        has_flush_out,
    }
}

fn has_backdoor_flush_only(
    hole_cards: &[TestCard; 2],
    board: &[TestCard],
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
    let candidate_suits = suited_hole_counts(hole_cards, board)
        .into_iter()
        .filter_map(|(suit, (hole_count, total_count))| {
            (hole_count > 0 && total_count == 3).then_some(suit)
        })
        .collect::<Vec<_>>();

    candidate_suits.into_iter().any(|target_suit| {
        let suited_unseen_cards = legal_unseen_next_cards(&current_cards)
            .into_iter()
            .filter(|card| card.suit == target_suit)
            .collect::<Vec<_>>();

        for turn_index in 0..suited_unseen_cards.len() {
            for river_index in turn_index + 1..suited_unseen_cards.len() {
                let mut river_state = current_cards.clone();
                river_state.push(suited_unseen_cards[turn_index]);
                river_state.push(suited_unseen_cards[river_index]);
                let river_evaluated = evaluate_best_hand(&river_state);
                if class_code(river_evaluated.best_hand_class) <= class_code(best_hand_class) {
                    continue;
                }

                if matches!(
                    river_evaluated.best_hand_class,
                    BestHandClass::Flush | BestHandClass::StraightFlush
                ) && best_combo_uses_hole_card(
                    &river_state,
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

fn known_hole_cards(hand: &CanonicalParsedHand, seat_no: u8) -> Option<[TestCard; 2]> {
    let seat_by_name = hand
        .seats
        .iter()
        .map(|seat| (seat.player_name.as_str(), seat.seat_no))
        .collect::<BTreeMap<_, _>>();

    let raw_cards =
        if let (Some(hero_name), Some(hero_cards)) = (&hand.hero_name, &hand.hero_hole_cards) {
            if seat_by_name.get(hero_name.as_str()).copied() == Some(seat_no) {
                Some(hero_cards.as_slice())
            } else {
                hand.showdown_hands.iter().find_map(|(player_name, cards)| {
                    (seat_by_name.get(player_name.as_str()).copied() == Some(seat_no))
                        .then_some(cards.as_slice())
                })
            }
        } else {
            hand.showdown_hands.iter().find_map(|(player_name, cards)| {
                (seat_by_name.get(player_name.as_str()).copied() == Some(seat_no))
                    .then_some(cards.as_slice())
            })
        }?;

    let parsed = raw_cards
        .iter()
        .map(|card| parse_card(card))
        .collect::<Result<Vec<_>, String>>()
        .ok()?;
    let [card_1, card_2] = parsed.as_slice() else {
        return None;
    };

    Some([*card_1, *card_2])
}

fn final_board_cards(hand: &CanonicalParsedHand) -> Result<Vec<TestCard>, String> {
    let source = if !hand.board_final.is_empty() {
        &hand.board_final
    } else {
        &hand.summary_board
    };

    source.iter().map(|card| parse_card(card)).collect()
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

fn street_board_cards(board: &[TestCard], street: Street) -> Vec<TestCard> {
    match street {
        Street::Flop => board.iter().take(3).copied().collect(),
        Street::Turn => board.iter().take(4).copied().collect(),
        Street::River => board.iter().take(5).copied().collect(),
        _ => Vec::new(),
    }
}

fn all_cards(hole_cards: &[TestCard; 2], board: &[TestCard]) -> Vec<TestCard> {
    let mut cards = hole_cards.to_vec();
    cards.extend(board.iter().copied());
    cards
}

fn evaluate_best_hand(cards: &[TestCard]) -> EvaluatedHand {
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

    best.expect("reference evaluator requires at least five cards")
}

fn five_card_combinations(cards: &[TestCard]) -> Vec<[TestCard; 5]> {
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

fn evaluate_five_card_hand(cards: &[TestCard; 5]) -> EvaluatedHand {
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
        (BestHandClass::Quads, vec![counts[0].0, counts[1].0])
    } else if counts[0].1 == 3 && counts[1].1 == 2 {
        (BestHandClass::FullHouse, vec![counts[0].0, counts[1].0])
    } else if is_flush {
        (BestHandClass::Flush, ranks.clone())
    } else if let Some(straight_high) = straight_high {
        (BestHandClass::Straight, vec![straight_high])
    } else if counts[0].1 == 3 {
        let mut ordered = vec![counts[0].0];
        ordered.extend(counts.iter().skip(1).map(|(rank, _)| *rank));
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
        ordered.extend(counts.iter().skip(1).map(|(rank, _)| *rank));
        (BestHandClass::Pair, ordered)
    } else {
        (BestHandClass::HighCard, ranks.clone())
    };

    EvaluatedHand {
        best_hand_class,
        best_hand_rank_value: encode_rank_value(class_code(best_hand_class), &kickers),
    }
}

fn legal_unseen_next_cards(current_cards: &[TestCard]) -> Vec<TestCard> {
    full_deck()
        .into_iter()
        .filter(|card| !current_cards.contains(card))
        .collect()
}

fn full_deck() -> Vec<TestCard> {
    let mut deck = Vec::with_capacity(52);
    for suit in [
        TestSuit::Clubs,
        TestSuit::Diamonds,
        TestSuit::Hearts,
        TestSuit::Spades,
    ] {
        for rank in 2_u8..=14_u8 {
            deck.push(TestCard { rank, suit });
        }
    }
    deck
}

fn best_combo_uses_hole_card(
    cards: &[TestCard],
    hole_cards: &[TestCard; 2],
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

fn has_player_specific_four_consecutive_run(
    hole_cards: &[TestCard; 2],
    board: &[TestCard],
) -> bool {
    let rank_set = mirrored_rank_set(
        &all_cards(hole_cards, board)
            .iter()
            .map(|card| card.rank)
            .collect::<Vec<_>>(),
    );
    let hole_rank_set =
        mirrored_rank_set(&hole_cards.iter().map(|card| card.rank).collect::<Vec<_>>());

    (1_u8..=11_u8).any(|start| {
        let mut window = start..=start + 3;
        window.clone().all(|rank| rank_set.contains(&rank))
            && window.any(|rank| hole_rank_set.contains(&rank))
    })
}

fn suited_hole_counts(
    hole_cards: &[TestCard; 2],
    board: &[TestCard],
) -> BTreeMap<TestSuit, (usize, usize)> {
    let mut counts = BTreeMap::new();
    for suit in [
        TestSuit::Clubs,
        TestSuit::Diamonds,
        TestSuit::Hearts,
        TestSuit::Spades,
    ] {
        let hole_count = hole_cards.iter().filter(|card| card.suit == suit).count();
        let total_count = hole_count + board.iter().filter(|card| card.suit == suit).count();
        counts.insert(suit, (hole_count, total_count));
    }
    counts
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

fn class_code(best_hand_class: BestHandClass) -> i64 {
    match best_hand_class {
        BestHandClass::HighCard => 0,
        BestHandClass::Pair => 1,
        BestHandClass::TwoPair => 2,
        BestHandClass::Trips => 3,
        BestHandClass::Straight => 4,
        BestHandClass::Flush => 5,
        BestHandClass::FullHouse => 6,
        BestHandClass::Quads => 7,
        BestHandClass::StraightFlush => 8,
    }
}

fn parse_card(raw: &str) -> Result<TestCard, String> {
    let mut chars = raw.chars();
    let rank_char = chars.next().ok_or_else(|| format!("invalid card: {raw}"))?;
    let suit_char = chars.next().ok_or_else(|| format!("invalid card: {raw}"))?;
    if chars.next().is_some() {
        return Err(format!("invalid card: {raw}"));
    }

    let rank = match rank_char {
        '2'..='9' => rank_char.to_digit(10).expect("digit rank") as u8,
        'T' => 10,
        'J' => 11,
        'Q' => 12,
        'K' => 13,
        'A' => 14,
        _ => return Err(format!("invalid rank card: {raw}")),
    };
    let suit = match suit_char {
        'c' => TestSuit::Clubs,
        'd' => TestSuit::Diamonds,
        'h' => TestSuit::Hearts,
        's' => TestSuit::Spades,
        _ => return Err(format!("invalid suit card: {raw}")),
    };

    Ok(TestCard { rank, suit })
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
