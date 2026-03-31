use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow};
use mbr_stats_runtime::GG_MBR_FT_MAX_PLAYERS;
use tracker_parser_core::models::TournamentSummary;
#[cfg(test)]
use tracker_parser_core::models::CanonicalParsedHand;
use uuid::Uuid;

use super::row_models::*;
use super::util::cents_to_f64;

#[cfg(test)]
pub(crate) fn build_mbr_stage_resolutions(
    player_profile_id: Uuid,
    hands: &[CanonicalParsedHand],
) -> BTreeMap<String, MbrStageResolutionRow> {
    let facts = hands
        .iter()
        .map(|hand| StageHandFact {
            hand_id: hand.header.hand_id.clone(),
            played_at: hand.header.played_at.clone(),
            max_players: hand.header.max_players,
            seat_count: hand.seats.len(),
            exact_hero_boundary_ko_share: None,
        })
        .collect::<Vec<_>>();

    build_mbr_stage_resolutions_from_facts(player_profile_id, &facts)
}

pub(crate) fn resolve_boundary_candidates(facts: &[StageHandFact]) -> BoundaryResolution {
    let mut chronological = facts.iter().collect::<Vec<_>>();
    chronological.sort_by(|left, right| {
        left.played_at
            .cmp(&right.played_at)
            .then_with(|| left.hand_id.cmp(&right.hand_id))
    });

    let Some(first_ft_index) = chronological
        .iter()
        .position(|hand| hand.max_players == GG_MBR_FT_MAX_PLAYERS as u8)
    else {
        return BoundaryResolution {
            candidate_hand_ids: BTreeSet::new(),
            resolution_state: "uncertain".to_string(),
            resolution_method: "timeline_last_non_ft_candidate_v2".to_string(),
            confidence_class: "no_exact_ft_hand".to_string(),
        };
    };

    if first_ft_index == 0 {
        return BoundaryResolution {
            candidate_hand_ids: BTreeSet::new(),
            resolution_state: "uncertain".to_string(),
            resolution_method: "timeline_last_non_ft_candidate_v2".to_string(),
            confidence_class: "no_pre_ft_candidate".to_string(),
        };
    }

    let last_non_ft_timestamp = chronological[first_ft_index - 1].played_at.as_str();
    let candidate_hand_ids = chronological[..first_ft_index]
        .iter()
        .rev()
        .take_while(|hand| hand.played_at == last_non_ft_timestamp)
        .map(|hand| hand.hand_id.clone())
        .collect::<BTreeSet<_>>();

    let (resolution_state, confidence_class) = if candidate_hand_ids.len() == 1 {
        ("exact".to_string(), "single_candidate".to_string())
    } else {
        (
            "uncertain".to_string(),
            "multi_candidate_same_timestamp".to_string(),
        )
    };

    BoundaryResolution {
        candidate_hand_ids,
        resolution_state,
        resolution_method: "timeline_last_non_ft_candidate_v2".to_string(),
        confidence_class,
    }
}

pub(crate) fn build_mbr_stage_resolutions_from_facts(
    player_profile_id: Uuid,
    facts: &[StageHandFact],
) -> BTreeMap<String, MbrStageResolutionRow> {
    let boundary_resolution = resolve_boundary_candidates(facts);
    let boundary_candidate_count = boundary_resolution.candidate_hand_ids.len() as i32;

    facts
        .iter()
        .map(|fact| {
            let played_ft_hand = fact.max_players == GG_MBR_FT_MAX_PLAYERS as u8;
            let ft_players_remaining_exact = played_ft_hand.then_some(fact.seat_count as i32);
            let is_stage_2 = ft_players_remaining_exact == Some(2);
            let is_stage_3_4 = matches!(ft_players_remaining_exact, Some(3 | 4));
            let is_stage_4_5 = matches!(ft_players_remaining_exact, Some(4 | 5));
            let is_stage_5_6 = matches!(ft_players_remaining_exact, Some(5 | 6));
            let is_stage_6_9 = matches!(ft_players_remaining_exact, Some(6..=9));
            let is_boundary_hand = boundary_resolution
                .candidate_hand_ids
                .contains(&fact.hand_id);
            let boundary_is_exact =
                boundary_resolution.resolution_state == "exact" && is_boundary_hand;
            let boundary_ko_value = if boundary_is_exact {
                fact.exact_hero_boundary_ko_share
            } else {
                None
            };

            (
                fact.hand_id.clone(),
                MbrStageResolutionRow {
                    player_profile_id,
                    played_ft_hand,
                    played_ft_hand_state: "exact".to_string(),
                    is_ft_hand: played_ft_hand,
                    ft_players_remaining_exact,
                    is_stage_2,
                    is_stage_3_4,
                    is_stage_4_5,
                    is_stage_5_6,
                    is_stage_6_9,
                    is_boundary_hand,
                    entered_boundary_zone: is_boundary_hand,
                    entered_boundary_zone_state: if boundary_is_exact {
                        "exact".to_string()
                    } else if is_boundary_hand {
                        "estimated".to_string()
                    } else {
                        "exact".to_string()
                    },
                    boundary_resolution_state: boundary_resolution.resolution_state.clone(),
                    boundary_candidate_count,
                    boundary_resolution_method: boundary_resolution.resolution_method.clone(),
                    boundary_confidence_class: boundary_resolution.confidence_class.clone(),
                    ft_table_size: ft_players_remaining_exact,
                    boundary_ko_ev: boundary_ko_value.map(|value| format!("{value:.6}")),
                    boundary_ko_min: boundary_ko_value.map(|value| format!("{value:.6}")),
                    boundary_ko_max: boundary_ko_value.map(|value| format!("{value:.6}")),
                    boundary_ko_method: is_boundary_hand
                        .then_some(boundary_resolution.resolution_method.clone()),
                    boundary_ko_certainty: if boundary_ko_value.is_some() {
                        Some("exact".to_string())
                    } else if is_boundary_hand {
                        Some(boundary_resolution.resolution_state.clone())
                    } else {
                        None
                    },
                    boundary_ko_state: if boundary_ko_value.is_some() {
                        "exact".to_string()
                    } else {
                        "uncertain".to_string()
                    },
                },
            )
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn build_tournament_ft_helper_source_hand(
    hand_id: Uuid,
    hand: &CanonicalParsedHand,
    stage_row: &MbrStageResolutionRow,
) -> TournamentFtHelperSourceHand {
    TournamentFtHelperSourceHand {
        hand_id,
        tournament_hand_order: 0,
        external_hand_id: hand.header.hand_id.clone(),
        hand_started_at_local: hand.header.played_at.clone(),
        played_ft_hand: stage_row.played_ft_hand,
        played_ft_hand_state: stage_row.played_ft_hand_state.clone(),
        ft_table_size: stage_row.ft_table_size,
        entered_boundary_zone: stage_row.entered_boundary_zone,
        boundary_resolution_state: stage_row.boundary_resolution_state.clone(),
        hero_starting_stack: hand.hero_name.as_deref().and_then(|hero_name| {
            hand.seats
                .iter()
                .find(|seat| seat.player_name == hero_name)
                .map(|seat| seat.starting_stack)
        }),
        big_blind: i64::from(hand.header.big_blind),
    }
}

pub(crate) fn build_mbr_tournament_ft_helper_row(
    tournament_id: Uuid,
    player_profile_id: Uuid,
    facts: &[TournamentFtHelperSourceHand],
) -> MbrTournamentFtHelperRow {
    let mut chronological = facts.iter().collect::<Vec<_>>();
    chronological.sort_by(|left, right| {
        left.tournament_hand_order
            .cmp(&right.tournament_hand_order)
            .then_with(|| left.external_hand_id.cmp(&right.external_hand_id))
    });

    let first_ft_hand = chronological
        .iter()
        .find(|fact| fact.played_ft_hand && fact.played_ft_hand_state == "exact")
        .copied();
    let reached_ft_exact = first_ft_hand.is_some();
    let deepest_ft_size_reached = chronological
        .iter()
        .filter(|fact| fact.played_ft_hand && fact.played_ft_hand_state == "exact")
        .filter_map(|fact| fact.ft_table_size)
        .min();
    let entered_boundary_zone = facts.iter().any(|fact| fact.entered_boundary_zone);

    let boundary_resolution_state = {
        let states = facts
            .iter()
            .map(|fact| fact.boundary_resolution_state.as_str())
            .collect::<BTreeSet<_>>();
        if states.len() == 1 {
            states
                .iter()
                .next()
                .copied()
                .unwrap_or("uncertain")
                .to_string()
        } else {
            "inconsistent".to_string()
        }
    };

    let (
        first_ft_hand_id,
        first_ft_hand_started_local,
        first_ft_table_size,
        ft_started_incomplete,
        hero_ft_entry_stack_chips,
        hero_ft_entry_stack_bb,
    ) = match first_ft_hand {
        Some(first_ft_hand) => {
            let first_ft_table_size = first_ft_hand.ft_table_size;
            let hero_ft_entry_stack_bb = match (
                first_ft_hand.hero_starting_stack,
                first_ft_hand.big_blind > 0,
            ) {
                (Some(stack), true) => Some(format!(
                    "{:.6}",
                    stack as f64 / first_ft_hand.big_blind as f64
                )),
                _ => None,
            };

            (
                Some(first_ft_hand.hand_id),
                Some(first_ft_hand.hand_started_at_local.clone()),
                first_ft_table_size,
                first_ft_table_size.map(|table_size| table_size < GG_MBR_FT_MAX_PLAYERS),
                first_ft_hand.hero_starting_stack,
                hero_ft_entry_stack_bb,
            )
        }
        None => (None, None, None, None, None, None),
    };

    MbrTournamentFtHelperRow {
        tournament_id,
        player_profile_id,
        reached_ft_exact,
        first_ft_hand_id,
        first_ft_hand_started_local,
        first_ft_table_size,
        ft_started_incomplete,
        deepest_ft_size_reached,
        hero_ft_entry_stack_chips,
        hero_ft_entry_stack_bb,
        entered_boundary_zone,
        boundary_resolution_state,
    }
}

pub(crate) fn load_ft_helper_source_hands_from_db(
    client: &mut impl postgres::GenericClient,
    tournament_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<TournamentFtHelperSourceHand>> {
    let rows = client.query(
        "SELECT
            h.id AS hand_id,
            h.tournament_hand_order,
            h.external_hand_id,
            h.hand_started_at_local::text,
            msr.played_ft_hand,
            msr.played_ft_hand_state,
            msr.ft_table_size,
            msr.entered_boundary_zone,
            msr.boundary_resolution_state,
            hero_seat.starting_stack AS hero_starting_stack,
            h.big_blind
        FROM core.hands h
        INNER JOIN derived.mbr_stage_resolution msr
            ON msr.hand_id = h.id AND msr.player_profile_id = h.player_profile_id
        LEFT JOIN LATERAL (
            SELECT hs.starting_stack
            FROM core.hand_seats hs
            WHERE hs.hand_id = h.id AND hs.is_hero = TRUE
            LIMIT 1
        ) hero_seat ON TRUE
        WHERE h.tournament_id = $1
            AND h.player_profile_id = $2
        ORDER BY h.tournament_hand_order NULLS LAST, h.external_hand_id, h.id",
        &[&tournament_id, &player_profile_id],
    )?;
    rows.into_iter()
        .map(|row| {
            let tournament_hand_order: Option<i32> = row.get(1);
            Ok(TournamentFtHelperSourceHand {
                hand_id: row.get(0),
                tournament_hand_order: tournament_hand_order.ok_or_else(|| {
                    anyhow!(
                        "missing tournament_hand_order for hand {} in tournament {}",
                        row.get::<_, Uuid>(0),
                        tournament_id
                    )
                })?,
                external_hand_id: row.get(2),
                hand_started_at_local: row.get(3),
                played_ft_hand: row.get(4),
                played_ft_hand_state: row.get(5),
                ft_table_size: row.get(6),
                entered_boundary_zone: row.get(7),
                boundary_resolution_state: row.get(8),
                hero_starting_stack: row.get(9),
                big_blind: row.get(10),
            })
        })
        .collect()
}

pub(crate) fn persist_mbr_tournament_ft_helper(
    tx: &mut impl postgres::GenericClient,
    row: &MbrTournamentFtHelperRow,
) -> Result<()> {
    tx.execute(
        "INSERT INTO derived.mbr_tournament_ft_helper (
            tournament_id,
            player_profile_id,
            reached_ft_exact,
            first_ft_hand_id,
            first_ft_hand_started_local,
            first_ft_table_size,
            ft_started_incomplete,
            deepest_ft_size_reached,
            hero_ft_entry_stack_chips,
            hero_ft_entry_stack_bb,
            entered_boundary_zone,
            boundary_resolution_state
        )
        VALUES (
            $1,
            $2,
            $3,
            $4,
            replace($5, '/', '-')::timestamp,
            $6,
            $7,
            $8,
            $9,
            ($10::text)::numeric(18,6),
            $11,
            $12
        )
        ON CONFLICT (tournament_id, player_profile_id)
        DO UPDATE SET
            reached_ft_exact = EXCLUDED.reached_ft_exact,
            first_ft_hand_id = EXCLUDED.first_ft_hand_id,
            first_ft_hand_started_local = EXCLUDED.first_ft_hand_started_local,
            first_ft_table_size = EXCLUDED.first_ft_table_size,
            ft_started_incomplete = EXCLUDED.ft_started_incomplete,
            deepest_ft_size_reached = EXCLUDED.deepest_ft_size_reached,
            hero_ft_entry_stack_chips = EXCLUDED.hero_ft_entry_stack_chips,
            hero_ft_entry_stack_bb = EXCLUDED.hero_ft_entry_stack_bb,
            entered_boundary_zone = EXCLUDED.entered_boundary_zone,
            boundary_resolution_state = EXCLUDED.boundary_resolution_state",
        &[
            &row.tournament_id,
            &row.player_profile_id,
            &row.reached_ft_exact,
            &row.first_ft_hand_id,
            &row.first_ft_hand_started_local,
            &row.first_ft_table_size,
            &row.ft_started_incomplete,
            &row.deepest_ft_size_reached,
            &row.hero_ft_entry_stack_chips,
            &row.hero_ft_entry_stack_bb,
            &row.entered_boundary_zone,
            &row.boundary_resolution_state,
        ],
    )?;

    Ok(())
}

/// Compute stable tournament_hand_order for all hands in this tournament.
/// Uses the same sort criteria as Rust-side chronological sort: timestamp + external_hand_id + id.
pub(crate) fn compute_tournament_hand_order(
    client: &mut impl postgres::GenericClient,
    tournament_id: Uuid,
) -> Result<()> {
    client.execute(
        "WITH ordered AS (
            SELECT id,
                   ROW_NUMBER() OVER (
                       ORDER BY hand_started_at_local NULLS LAST,
                                external_hand_id,
                                id
                   )::int AS computed_order
            FROM core.hands
            WHERE tournament_id = $1
        )
        UPDATE core.hands h
        SET tournament_hand_order = ordered.computed_order
        FROM ordered
        WHERE h.id = ordered.id",
        &[&tournament_id],
    )?;
    Ok(())
}

pub(crate) fn load_tournament_entry_economics(
    tx: &mut impl postgres::GenericClient,
    context: &ImportContext,
    summary: &TournamentSummary,
) -> Result<TournamentEntryEconomics> {
    let regular_prize_cents: i64 = tx
        .query_opt(
            "SELECT COALESCE((prize.regular_prize_money * 100)::bigint, 0::bigint)
             FROM ref.mbr_buyin_configs config
             LEFT JOIN ref.mbr_regular_prizes prize
               ON prize.buyin_config_id = config.id
              AND prize.finish_place = $5
             WHERE config.room_id = $1
               AND config.format_id = $2
               AND config.buyin_total = ($3::double precision)::numeric(12,2)
               AND config.currency = $4
               AND config.max_players = $6",
            &[
                &context.room_id,
                &context.format_id,
                &cents_to_f64(summary.buy_in_cents + summary.rake_cents + summary.bounty_cents),
                &"USD",
                &(summary.finish_place as i32),
                &(summary.entrants as i32),
            ],
        )?
        .map(|row| row.get(0))
        .ok_or_else(|| {
            anyhow!(
                "missing MBR buy-in config for buyin_total={}, entrants={}",
                summary.buy_in_cents + summary.rake_cents + summary.bounty_cents,
                summary.entrants
            )
        })?;

    resolve_tournament_entry_economics(summary, regular_prize_cents)
}

pub(crate) fn resolve_tournament_entry_economics(
    summary: &TournamentSummary,
    regular_prize_cents: i64,
) -> Result<TournamentEntryEconomics> {
    let mystery_money_cents = summary.payout_cents - regular_prize_cents;
    if mystery_money_cents < 0 {
        return Err(anyhow!(
            "mystery_money_total cannot be negative: payout_cents={}, regular_prize_cents={}",
            summary.payout_cents,
            regular_prize_cents
        ));
    }

    Ok(TournamentEntryEconomics {
        regular_prize_cents,
        mystery_money_cents,
    })
}
