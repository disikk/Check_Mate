use std::collections::BTreeMap;

use anyhow::Result;
use tracker_parser_core::{
    models::{
        ActionType, CanonicalParsedHand, ParseIssueCode, ParseIssuePayload, Street,
    },
    normalizer::normalize_hand,
    parsers::{
        hand_history::{parse_canonical_hand, split_hand_history},
        tournament_summary::parse_tournament_summary,
    },
    positions::{PositionSeatInput, compute_position_facts},
    preflop_starting_hands::evaluate_preflop_starting_hands,
    street_strength::evaluate_street_hand_strength,
};
use uuid::Uuid;

use super::row_models::*;
use super::util::*;
use super::HAND_RESOLUTION_VERSION;

pub(crate) fn build_canonical_persistence(hand: &CanonicalParsedHand) -> Result<CanonicalHandPersistence> {
    let mut seat_lookup = BTreeMap::new();
    let mut seat_lookup_by_no = BTreeMap::new();
    let mut seats = Vec::new();
    for seat in &hand.seats {
        seat_lookup.insert(seat.player_name.clone(), seat.seat_no);
        seat_lookup_by_no.insert(seat.seat_no, seat.player_name.clone());
        seats.push(HandSeatRow {
            seat_no: i32::from(seat.seat_no),
            player_name: seat.player_name.clone(),
            starting_stack: seat.starting_stack,
            is_hero: hand.hero_name.as_deref() == Some(seat.player_name.as_str()),
            is_button: seat.seat_no == hand.header.button_seat,
            is_sitting_out: seat.is_sitting_out,
        });
    }

    let positions = build_position_rows(hand)?;

    let mut parse_issues = hand
        .parse_issues
        .iter()
        .map(parse_issue_row)
        .collect::<Vec<_>>();

    let mut hole_cards_by_seat = BTreeMap::new();
    if let (Some(hero_name), Some(hero_cards)) = (&hand.hero_name, &hand.hero_hole_cards) {
        match seat_lookup.get(hero_name) {
            Some(seat_no) => upsert_hole_cards(
                &mut hole_cards_by_seat,
                *seat_no,
                hero_cards,
                true,
                hand.showdown_hands.contains_key(hero_name),
            ),
            None => parse_issues.push(error_issue_row(
                ParseIssueCode::HeroCardsMissingSeat,
                format!("hero hole cards exist but hero `{hero_name}` has no seat row"),
                None,
                Some(ParseIssuePayload::HeroCardsMissingSeat {
                    hero_name: hero_name.clone(),
                }),
            )),
        }
    }

    let mut showdowns = Vec::new();
    for (player_name, shown_cards) in &hand.showdown_hands {
        match seat_lookup.get(player_name) {
            Some(seat_no) => {
                upsert_hole_cards(&mut hole_cards_by_seat, *seat_no, shown_cards, false, true);
                showdowns.push(HandShowdownRow {
                    seat_no: i32::from(*seat_no),
                    shown_cards: shown_cards.clone(),
                });
            }
            None => parse_issues.push(error_issue_row(
                ParseIssueCode::ShowdownPlayerMissingSeat,
                format!("showdown hand exists for `{player_name}` without seat row"),
                None,
                Some(ParseIssuePayload::ShowdownPlayerMissingSeat {
                    player_name: player_name.clone(),
                }),
            )),
        }
    }

    let mut summary_seat_outcomes = Vec::new();
    for outcome in &hand.summary_seat_outcomes {
        match seat_lookup_by_no.get(&outcome.seat_no) {
            Some(canonical_player_name) if canonical_player_name == &outcome.player_name => {
                summary_seat_outcomes.push(HandSummarySeatOutcomeRow {
                    seat_no: i32::from(outcome.seat_no),
                    player_name: outcome.player_name.clone(),
                    position_marker: outcome.position_marker.map(|marker| match marker {
                        tracker_parser_core::models::SummarySeatMarker::Button => {
                            "button".to_string()
                        }
                        tracker_parser_core::models::SummarySeatMarker::SmallBlind => {
                            "small blind".to_string()
                        }
                        tracker_parser_core::models::SummarySeatMarker::BigBlind => {
                            "big blind".to_string()
                        }
                    }),
                    outcome_kind: match outcome.outcome_kind {
                        tracker_parser_core::models::SummarySeatOutcomeKind::Folded => {
                            "folded".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::ShowedWon => {
                            "showed_won".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::ShowedLost => {
                            "showed_lost".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::Lost => {
                            "lost".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::Mucked => {
                            "mucked".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::Won => {
                            "won".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::Collected => {
                            "collected".to_string()
                        }
                    },
                    folded_street: outcome.folded_at.map(street_code).map(str::to_string),
                    shown_cards: outcome.shown_cards.clone(),
                    won_amount: outcome.won_amount,
                    hand_class: outcome.hand_class.clone(),
                    raw_line: outcome.raw_line.clone(),
                })
            }
            Some(canonical_player_name) => parse_issues.push(error_issue_row(
                ParseIssueCode::SummarySeatOutcomeSeatMismatch,
                format!(
                    "summary seat {} references `{}` but canonical seat belongs to `{}`",
                    outcome.seat_no, outcome.player_name, canonical_player_name
                ),
                Some(outcome.raw_line.clone()),
                Some(ParseIssuePayload::SummarySeatOutcomeSeatMismatch {
                    seat_no: outcome.seat_no,
                    player_name: outcome.player_name.clone(),
                    canonical_player_name: canonical_player_name.clone(),
                }),
            )),
            None => parse_issues.push(error_issue_row(
                ParseIssueCode::SummarySeatOutcomeMissingSeat,
                format!(
                    "summary seat {} references `{}` without seat row",
                    outcome.seat_no, outcome.player_name
                ),
                Some(outcome.raw_line.clone()),
                Some(ParseIssuePayload::SummarySeatOutcomeMissingSeat {
                    seat_no: outcome.seat_no,
                    player_name: outcome.player_name.clone(),
                }),
            )),
        }
    }

    let mut actions = Vec::new();
    for event in &hand.actions {
        let seat_no = event
            .player_name
            .as_ref()
            .and_then(|player_name| seat_lookup.get(player_name).copied());

        if let Some(player_name) = &event.player_name
            && seat_no.is_none()
        {
            parse_issues.push(error_issue_row(
                ParseIssueCode::ActionPlayerMissingSeat,
                format!("action references `{player_name}` without seat row"),
                Some(event.raw_line.clone()),
                Some(ParseIssuePayload::ActionPlayerMissingSeat {
                    player_name: player_name.clone(),
                    raw_line: event.raw_line.clone(),
                }),
            ));
        }

        actions.push(HandActionRow {
            sequence_no: event.seq as i32,
            street: street_code(event.street).to_string(),
            seat_no: seat_no.map(i32::from),
            action_type: action_code(event.action_type).to_string(),
            raw_amount: event.amount,
            to_amount: event.to_amount,
            is_all_in: event.is_all_in,
            all_in_reason: event
                .all_in_reason
                .map(|reason| reason.as_str().to_string()),
            forced_all_in_preflop: event.forced_all_in_preflop,
            references_previous_bet: matches!(
                event.action_type,
                ActionType::Call | ActionType::RaiseTo
            ),
            raw_line: event.raw_line.clone(),
        });
    }

    Ok(CanonicalHandPersistence {
        seats,
        positions,
        hole_cards: hole_cards_by_seat.into_values().collect(),
        actions,
        board: build_board_row(&hand.board_final),
        showdowns,
        summary_seat_outcomes,
        parse_issues,
    })
}

pub(crate) fn build_normalized_persistence(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> NormalizedHandPersistence {
    NormalizedHandPersistence {
        state_resolution: build_hand_state_resolution(normalized_hand),
        pot_rows: build_hand_pot_rows(normalized_hand),
        eligibility_rows: build_hand_pot_eligibility_rows(normalized_hand),
        contribution_rows: build_hand_pot_contribution_rows(normalized_hand),
        winner_rows: build_hand_pot_winner_rows(normalized_hand),
        return_rows: build_hand_return_rows(normalized_hand),
        elimination_rows: build_hand_elimination_rows(normalized_hand),
    }
}

pub(crate) fn build_hand_state_resolution(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> HandStateResolutionRow {
    HandStateResolutionRow {
        resolution_version: HAND_RESOLUTION_VERSION.to_string(),
        chip_conservation_ok: normalized_hand.invariants.chip_conservation_ok,
        pot_conservation_ok: normalized_hand.invariants.pot_conservation_ok,
        settlement_state: certainty_state_code(normalized_hand.settlement.certainty_state)
            .to_string(),
        rake_amount: normalized_hand.actual.rake_amount,
        final_stacks: normalized_hand.actual.stacks_after_observed.clone(),
        settlement: normalized_hand.settlement.clone(),
        invariant_issues: normalized_hand.invariants.issues.clone(),
    }
}

pub(crate) fn build_hand_pot_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandPotRow> {
    normalized_hand
        .settlement
        .pots
        .iter()
        .map(|pot| HandPotRow {
            pot_no: i32::from(pot.pot_no),
            pot_type: if pot.is_main {
                "main".to_string()
            } else {
                "side".to_string()
            },
            amount: pot.amount,
        })
        .collect()
}

pub(crate) fn build_hand_pot_eligibility_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandPotEligibilityRow> {
    normalized_hand
        .settlement
        .pots
        .iter()
        .flat_map(|pot| pot.eligibilities.iter())
        .map(|eligibility| HandPotEligibilityRow {
            pot_no: i32::from(eligibility.pot_no),
            seat_no: i32::from(eligibility.seat_no),
        })
        .collect()
}

pub(crate) fn build_hand_pot_contribution_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandPotContributionRow> {
    normalized_hand
        .settlement
        .pots
        .iter()
        .flat_map(|pot| pot.contributions.iter())
        .map(|contribution| HandPotContributionRow {
            pot_no: i32::from(contribution.pot_no),
            seat_no: i32::from(contribution.seat_no),
            amount: contribution.amount,
        })
        .collect()
}

pub(crate) fn build_hand_pot_winner_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandPotWinnerRow> {
    normalized_hand
        .settlement
        .pots
        .iter()
        .flat_map(|pot| {
            pot.selected_allocation.iter().flat_map(move |allocation| {
                allocation.shares.iter().map(move |share| HandPotWinnerRow {
                    pot_no: i32::from(pot.pot_no),
                    seat_no: i32::from(share.seat_no),
                    share_amount: share.share_amount,
                })
            })
        })
        .collect()
}

pub(crate) fn build_hand_return_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandReturnRow> {
    normalized_hand
        .returns
        .iter()
        .map(|hand_return| HandReturnRow {
            seat_no: i32::from(hand_return.seat_no),
            amount: hand_return.amount,
            reason: hand_return.reason.clone(),
        })
        .collect()
}

pub(crate) fn build_hand_elimination_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandEliminationRow> {
    normalized_hand
        .eliminations
        .iter()
        .map(|elimination| HandEliminationRow {
            eliminated_seat_no: elimination.eliminated_seat_no as i32,
            eliminated_player_name: elimination.eliminated_player_name.clone(),
            pots_participated_by_busted: elimination
                .pots_participated_by_busted
                .iter()
                .copied()
                .map(i32::from)
                .collect(),
            pots_causing_bust: elimination
                .pots_causing_bust
                .iter()
                .copied()
                .map(i32::from)
                .collect(),
            last_busting_pot_no: elimination.last_busting_pot_no.map(i32::from),
            ko_winner_set: elimination.ko_winner_set.clone(),
            ko_share_fraction_by_winner: elimination
                .ko_share_fraction_by_winner
                .iter()
                .map(|share| HandEliminationKoShareRow {
                    seat_no: i32::from(share.seat_no),
                    player_name: share.player_name.clone(),
                    share_fraction: format_fraction_value(share.share_fraction),
                })
                .collect(),
            elimination_certainty_state: certainty_state_code(
                elimination.elimination_certainty_state,
            )
            .to_string(),
            ko_certainty_state: certainty_state_code(elimination.ko_certainty_state).to_string(),
        })
        .collect()
}

pub(crate) fn build_street_hand_strength_rows(
    hand: &CanonicalParsedHand,
) -> Result<Vec<StreetHandStrengthRow>> {
    Ok(evaluate_street_hand_strength(hand)?
        .into_iter()
        .map(|descriptor| StreetHandStrengthRow {
            seat_no: descriptor.seat_no as i32,
            street: street_code(descriptor.street).to_string(),
            best_hand_class: descriptor.best_hand_class.as_str().to_string(),
            best_hand_rank_value: descriptor.best_hand_rank_value,
            made_hand_category: descriptor.made_hand_category.as_str().to_string(),
            draw_category: descriptor.draw_category.as_str().to_string(),
            overcards_count: i32::from(descriptor.overcards_count),
            has_air: descriptor.has_air,
            missed_flush_draw: descriptor.missed_flush_draw,
            missed_straight_draw: descriptor.missed_straight_draw,
            is_nut_hand: descriptor.is_nut_hand,
            is_nut_draw: descriptor.is_nut_draw,
            certainty_state: certainty_state_code(descriptor.certainty_state).to_string(),
        })
        .collect())
}

pub(crate) fn build_preflop_starting_hand_rows(
    hand: &CanonicalParsedHand,
) -> Result<Vec<PreflopStartingHandRow>> {
    Ok(evaluate_preflop_starting_hands(hand)?
        .into_iter()
        .map(|descriptor| PreflopStartingHandRow {
            seat_no: descriptor.seat_no as i32,
            starter_hand_class: descriptor.starter_hand_class,
            certainty_state: certainty_state_code(descriptor.certainty_state).to_string(),
        })
        .collect())
}

pub(crate) fn build_hand_local_compute_output(
    hand: &CanonicalParsedHand,
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Result<HandLocalComputeOutput> {
    Ok(HandLocalComputeOutput {
        canonical_persistence: build_canonical_persistence(hand)?,
        normalized_persistence: build_normalized_persistence(normalized_hand),
        ko_attempt_rows: build_hand_ko_attempt_rows(hand),
        ko_opportunity_rows: build_hand_ko_opportunity_rows(hand),
        preflop_starting_hand_rows: build_preflop_starting_hand_rows(hand)?,
        street_strength_rows: build_street_hand_strength_rows(hand)?,
        stage_fact: StageHandFact {
            hand_id: hand.header.hand_id.clone(),
            played_at: hand.header.played_at.clone(),
            max_players: hand.header.max_players,
            seat_count: hand.seats.len(),
            exact_hero_boundary_ko_share: exact_hero_boundary_ko_share(hand, normalized_hand),
        },
    })
}

#[derive(Debug, Clone, Copy)]
struct KoAttemptTrigger {
    sequence_no: i32,
    street: Street,
    is_forced_all_in: bool,
}

#[derive(Debug, Clone, Copy)]
struct KoTargetActionFacts {
    fold_sequence_no: Option<i32>,
    all_in_sequence_no: Option<i32>,
    all_in_street: Option<Street>,
    forced_auto_all_in_sequence_no: Option<i32>,
    forced_auto_all_in_street: Option<Street>,
}

pub(crate) fn build_hand_ko_attempt_rows(hand: &CanonicalParsedHand) -> Vec<HandKoAttemptRow> {
    let Some(hero_name) = hand.hero_name.as_deref() else {
        return Vec::new();
    };
    let Some((hero_seat_no, hero_stack)) = hero_identity(hand, hero_name) else {
        return Vec::new();
    };
    let hero_actions = player_actions(hand, hero_name);
    let hero_fold_sequence_no = hero_actions
        .iter()
        .find(|action| action.action_type == ActionType::Fold)
        .map(|action| action.seq as i32);

    if hero_fold_sequence_no.is_some() {
        return Vec::new();
    }

    let hero_push_trigger = first_hero_push_trigger(&hero_actions, hero_stack);

    target_seat_rows(hand, hero_name, hero_stack)
        .into_iter()
        .filter_map(|(target_seat_no, target_player_name, target_stack)| {
            let target_actions = player_actions(hand, target_player_name.as_str());
            let target_facts = target_action_facts(&target_actions, target_stack);
            let trigger = if let Some(hero_push_trigger) = hero_push_trigger {
                target_can_still_confront_after_push(hero_push_trigger.sequence_no, &target_facts)
                    .then_some(KoAttemptTrigger {
                        sequence_no: hero_push_trigger.sequence_no,
                        street: hero_push_trigger.street,
                        is_forced_all_in: target_facts.forced_auto_all_in_sequence_no.is_some(),
                    })
            } else if let (Some(target_all_in_sequence_no), Some(target_all_in_street)) =
                (target_facts.all_in_sequence_no, target_facts.all_in_street)
            {
                hero_responded_after_all_in(&hero_actions, target_all_in_sequence_no).then_some(
                    KoAttemptTrigger {
                        sequence_no: target_all_in_sequence_no,
                        street: target_all_in_street,
                        is_forced_all_in: false,
                    },
                )
            } else {
                target_facts
                    .forced_auto_all_in_sequence_no
                    .zip(target_facts.forced_auto_all_in_street)
                    .map(|(sequence_no, street)| KoAttemptTrigger {
                        sequence_no,
                        street,
                        is_forced_all_in: true,
                    })
            }?;

            Some(HandKoAttemptRow {
                hero_seat_no,
                target_seat_no,
                target_player_name,
                attempt_kind: if trigger.is_forced_all_in {
                    "forced_auto_all_in".to_string()
                } else if hero_push_trigger.is_some() {
                    "hero_push".to_string()
                } else {
                    "hero_response".to_string()
                },
                street: street_code(trigger.street).to_string(),
                source_sequence_no: trigger.sequence_no,
                is_forced_all_in: trigger.is_forced_all_in,
            })
        })
        .collect()
}

pub(crate) fn build_hand_ko_opportunity_rows(hand: &CanonicalParsedHand) -> Vec<HandKoOpportunityRow> {
    let Some(hero_name) = hand.hero_name.as_deref() else {
        return Vec::new();
    };
    let Some((hero_seat_no, hero_stack)) = hero_identity(hand, hero_name) else {
        return Vec::new();
    };
    let hero_actions = player_actions(hand, hero_name);
    let hero_fold_sequence_no = hero_actions
        .iter()
        .find(|action| action.action_type == ActionType::Fold)
        .map(|action| action.seq as i32);

    target_seat_rows(hand, hero_name, hero_stack)
        .into_iter()
        .filter_map(|(target_seat_no, target_player_name, target_stack)| {
            let target_actions = player_actions(hand, target_player_name.as_str());
            let target_facts = target_action_facts(&target_actions, target_stack);

            if let (Some(sequence_no), Some(street)) =
                (target_facts.all_in_sequence_no, target_facts.all_in_street)
            {
                if hero_fold_sequence_no.is_some_and(|fold_seq| fold_seq < sequence_no) {
                    return None;
                }

                return Some(HandKoOpportunityRow {
                    hero_seat_no,
                    target_seat_no,
                    target_player_name,
                    opportunity_kind: "all_in".to_string(),
                    street: street_code(street).to_string(),
                    source_sequence_no: sequence_no,
                    is_forced_all_in: false,
                });
            }

            target_facts
                .forced_auto_all_in_sequence_no
                .zip(target_facts.forced_auto_all_in_street)
                .map(|(sequence_no, street)| HandKoOpportunityRow {
                    hero_seat_no,
                    target_seat_no,
                    target_player_name,
                    opportunity_kind: "forced_auto_all_in".to_string(),
                    street: street_code(street).to_string(),
                    source_sequence_no: sequence_no,
                    is_forced_all_in: true,
                })
        })
        .collect()
}

pub(crate) fn hero_identity(hand: &CanonicalParsedHand, hero_name: &str) -> Option<(i32, i64)> {
    hand.seats
        .iter()
        .find(|seat| seat.player_name == hero_name)
        .map(|seat| (i32::from(seat.seat_no), seat.starting_stack))
}

pub(crate) fn player_actions<'a>(
    hand: &'a CanonicalParsedHand,
    player_name: &'a str,
) -> Vec<&'a tracker_parser_core::models::HandActionEvent> {
    hand.actions
        .iter()
        .filter(|action| action.player_name.as_deref() == Some(player_name))
        .collect()
}

fn first_hero_push_trigger(
    hero_actions: &[&tracker_parser_core::models::HandActionEvent],
    hero_stack: i64,
) -> Option<KoAttemptTrigger> {
    hero_actions.iter().find_map(|action| {
        let all_in_raise = matches!(action.action_type, ActionType::Bet | ActionType::RaiseTo)
            && action
                .to_amount
                .unwrap_or(action.amount.unwrap_or_default()) as f64
                >= hero_stack as f64 * 0.9;
        (action.is_all_in || all_in_raise).then_some(KoAttemptTrigger {
            sequence_no: action.seq as i32,
            street: action.street,
            is_forced_all_in: false,
        })
    })
}

pub(crate) fn target_seat_rows(
    hand: &CanonicalParsedHand,
    hero_name: &str,
    hero_stack: i64,
) -> Vec<(i32, String, i64)> {
    hand.seats
        .iter()
        .filter(|seat| seat.player_name != hero_name)
        .filter(|seat| seat.starting_stack > 0)
        .filter(|seat| hero_stack >= seat.starting_stack)
        .map(|seat| {
            (
                i32::from(seat.seat_no),
                seat.player_name.clone(),
                seat.starting_stack,
            )
        })
        .collect()
}

fn target_action_facts(
    target_actions: &[&tracker_parser_core::models::HandActionEvent],
    target_stack: i64,
) -> KoTargetActionFacts {
    let fold_sequence_no = target_actions
        .iter()
        .find(|action| action.action_type == ActionType::Fold)
        .map(|action| action.seq as i32);
    let all_in_action = target_actions.iter().find(|action| {
        action.is_all_in
            && matches!(
                action.action_type,
                ActionType::Call | ActionType::Bet | ActionType::RaiseTo
            )
            && !action.forced_all_in_preflop
            && !matches!(
                action.all_in_reason,
                Some(tracker_parser_core::models::AllInReason::BlindExhausted)
                    | Some(tracker_parser_core::models::AllInReason::AnteExhausted)
            )
    });

    let mut forced_commit_total = 0_i64;
    let mut forced_auto_all_in_sequence_no = None;
    let mut forced_auto_all_in_street = None;
    for action in target_actions.iter().filter(|action| {
        matches!(
            action.action_type,
            ActionType::PostAnte | ActionType::PostSb | ActionType::PostBb | ActionType::PostDead
        )
    }) {
        forced_commit_total += action
            .amount
            .unwrap_or(action.to_amount.unwrap_or_default());
        if forced_auto_all_in_sequence_no.is_none() && forced_commit_total >= target_stack {
            forced_auto_all_in_sequence_no = Some(action.seq as i32);
            forced_auto_all_in_street = Some(action.street);
        }
    }

    KoTargetActionFacts {
        fold_sequence_no,
        all_in_sequence_no: all_in_action.map(|action| action.seq as i32),
        all_in_street: all_in_action.map(|action| action.street),
        forced_auto_all_in_sequence_no: all_in_action
            .is_none()
            .then_some(forced_auto_all_in_sequence_no)
            .flatten(),
        forced_auto_all_in_street: all_in_action
            .is_none()
            .then_some(forced_auto_all_in_street)
            .flatten(),
    }
}

fn target_can_still_confront_after_push(
    hero_push_sequence_no: i32,
    target_facts: &KoTargetActionFacts,
) -> bool {
    let target_folded_before_push = target_facts
        .fold_sequence_no
        .is_some_and(|fold_seq| fold_seq < hero_push_sequence_no);

    !target_folded_before_push
        && (target_facts.all_in_sequence_no.is_some()
            || target_facts.forced_auto_all_in_sequence_no.is_some())
}

pub(crate) fn hero_responded_after_all_in(
    hero_actions: &[&tracker_parser_core::models::HandActionEvent],
    target_all_in_sequence_no: i32,
) -> bool {
    hero_actions.iter().any(|action| {
        action.seq as i32 > target_all_in_sequence_no
            && (matches!(
                action.action_type,
                ActionType::Call | ActionType::Bet | ActionType::RaiseTo
            ) || action.is_all_in)
    })
}

pub(crate) fn upsert_hole_cards(
    map: &mut BTreeMap<u8, HandHoleCardsRow>,
    seat_no: u8,
    cards: &[String],
    known_to_hero: bool,
    known_at_showdown: bool,
) {
    let entry = map.entry(seat_no).or_insert_with(|| HandHoleCardsRow {
        seat_no: i32::from(seat_no),
        card1: cards.first().cloned(),
        card2: cards.get(1).cloned(),
        known_to_hero: false,
        known_at_showdown: false,
    });

    if entry.card1.is_none() {
        entry.card1 = cards.first().cloned();
    }
    if entry.card2.is_none() {
        entry.card2 = cards.get(1).cloned();
    }
    entry.known_to_hero |= known_to_hero;
    entry.known_at_showdown |= known_at_showdown;
}

pub(crate) fn build_board_row(cards: &[String]) -> Option<HandBoardRow> {
    if cards.is_empty() {
        return None;
    }

    Some(HandBoardRow {
        flop1: cards.first().cloned(),
        flop2: cards.get(1).cloned(),
        flop3: cards.get(2).cloned(),
        turn: cards.get(3).cloned(),
        river: cards.get(4).cloned(),
    })
}

pub(crate) fn build_position_rows(hand: &CanonicalParsedHand) -> Result<Vec<HandPositionRow>> {
    let position_inputs = hand
        .seats
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
    .map_err(|error| {
        anyhow::anyhow!(
            "failed to compute positions for hand {}: {error}",
            hand.header.hand_id
        )
    })?;

    Ok(positions
        .into_iter()
        .map(|position| HandPositionRow {
            seat_no: i32::from(position.seat_no),
            position_index: i32::from(position.position_index),
            position_label: position.position_label.as_str().to_string(),
            preflop_act_order_index: i32::from(position.preflop_act_order_index),
            postflop_act_order_index: i32::from(position.postflop_act_order_index),
        })
        .collect())
}

pub(crate) fn prepare_tournament_summary_import(input: &str) -> Result<PreparedTournamentSummaryImport> {
    let parse_started_at = std::time::Instant::now();
    let summary = parse_tournament_summary(input)?;
    Ok(PreparedTournamentSummaryImport {
        summary,
        parse_ms: parse_started_at.elapsed().as_millis() as u64,
    })
}

pub(crate) fn prepare_hand_history_import(
    input: &str,
    player_profile_id: Uuid,
) -> Result<PreparedHandHistoryImport> {
    let parse_started_at = std::time::Instant::now();
    let hands = split_hand_history(input)?;
    let canonical_hands = hands
        .iter()
        .map(|hand| parse_canonical_hand(&hand.raw_text))
        .collect::<Result<Vec<_>, _>>()?;
    let parse_ms = parse_started_at.elapsed().as_millis() as u64;

    let normalize_started_at = std::time::Instant::now();
    let normalized_hands = canonical_hands
        .iter()
        .map(normalize_hand)
        .collect::<Result<Vec<_>, _>>()?;
    let normalize_ms = normalize_started_at.elapsed().as_millis() as u64;

    let derive_hand_local_started_at = std::time::Instant::now();
    let hand_local_outputs = canonical_hands
        .iter()
        .zip(normalized_hands.iter())
        .map(|(hand, normalized_hand)| build_hand_local_compute_output(hand, normalized_hand))
        .collect::<Result<Vec<_>>>()?;
    let derive_hand_local_ms = derive_hand_local_started_at.elapsed().as_millis() as u64;
    let derive_tournament_started_at = std::time::Instant::now();
    let stage_facts = hand_local_outputs
        .iter()
        .map(|output| output.stage_fact.clone())
        .collect::<Vec<_>>();
    let mbr_stage_resolutions =
        super::mbr_domain::build_mbr_stage_resolutions_from_facts(player_profile_id, &stage_facts);
    let ordered_stage_resolutions = canonical_hands
        .iter()
        .map(|canonical_hand| {
            mbr_stage_resolutions
                .get(&canonical_hand.header.hand_id)
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "missing mbr stage resolution for hand {}",
                        canonical_hand.header.hand_id
                    )
                })
        })
        .collect::<Result<Vec<_>>>()?;
    let derive_tournament_ms = derive_tournament_started_at.elapsed().as_millis() as u64;

    Ok(PreparedHandHistoryImport {
        hands,
        canonical_hands,
        hand_local_outputs,
        ordered_stage_resolutions,
        parse_ms,
        normalize_ms,
        derive_hand_local_ms,
        derive_tournament_ms,
    })
}
