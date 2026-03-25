use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use postgres::GenericClient;
use uuid::Uuid;

use crate::big_ko::{
    MysteryEnvelope, expected_big_ko_bucket_probabilities, expected_hero_mystery_cents,
};
use crate::models::{
    CanonicalStatPoint, CanonicalStatSnapshot, SeedStatCoverage, SeedStatSnapshot, SeedStatsFilters,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TournamentBuyinFact {
    tournament_id: Uuid,
    buyin_total_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SummaryTournamentFact {
    tournament_id: Uuid,
    buyin_total_cents: i64,
    payout_cents: i64,
    regular_prize_cents: i64,
    finish_place: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TournamentFtHelperFact {
    tournament_id: Uuid,
    reached_ft_exact: bool,
    ft_started_incomplete: Option<bool>,
    deepest_ft_size_reached: Option<i32>,
    hero_ft_entry_stack_chips: Option<i64>,
    hero_ft_entry_stack_bb: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TournamentKoEventFact {
    tournament_id: Uuid,
    total_exact_ko_event_count: u64,
    early_ft_exact_ko_event_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DeepFtEntryFact {
    tournament_id: Uuid,
    hero_stack_chips: Option<i64>,
    hero_stack_bb: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TournamentStageEventFact {
    tournament_id: Uuid,
    early_ft_bust_count: u64,
    ko_stage_2_3_event_count: u64,
    ko_stage_3_4_event_count: u64,
    ko_stage_4_5_event_count: u64,
    ko_stage_5_6_event_count: u64,
    ko_stage_6_9_event_count: u64,
    ko_stage_7_9_event_count: u64,
    pre_ft_ko_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TournamentStageAttemptFact {
    tournament_id: Uuid,
    ko_stage_2_3_attempt_count: u64,
    ko_stage_3_4_attempt_count: u64,
    ko_stage_4_5_attempt_count: u64,
    ko_stage_5_6_attempt_count: u64,
    ko_stage_6_9_attempt_count: u64,
    ko_stage_7_9_attempt_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TournamentStageEntryFact {
    tournament_id: Uuid,
    reached_stage_2_3: bool,
    reached_stage_3_4: bool,
    reached_stage_4_5: bool,
    reached_stage_5_6: bool,
    reached_stage_7_9: bool,
    hero_stage_5_6_stack_bb: Option<f64>,
    hero_stage_3_4_stack_bb: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TournamentKoMoneyEventFact {
    tournament_id: Uuid,
    share_micros: i64,
    is_stage_2_3: bool,
    is_stage_3_4: bool,
    is_stage_4_5: bool,
    is_stage_5_6: bool,
    is_stage_6_9: bool,
    is_stage_7_9: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MysteryEnvelopeFact {
    buyin_total_cents: i64,
    sort_order: i32,
    payout_cents: i64,
    frequency_per_100m: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TournamentPreFtChipFact {
    tournament_id: Uuid,
    chip_delta: i64,
}

#[derive(Debug, Clone, PartialEq)]
struct CanonicalQueryInputs {
    tournament_buyin_facts: Vec<TournamentBuyinFact>,
    summary_facts: Vec<SummaryTournamentFact>,
    hand_covered_tournaments: Vec<Uuid>,
    ft_helper_facts: Vec<TournamentFtHelperFact>,
    ko_event_facts: Vec<TournamentKoEventFact>,
    deep_ft_entry_facts: Vec<DeepFtEntryFact>,
    stage_event_facts: Vec<TournamentStageEventFact>,
    stage_attempt_facts: Vec<TournamentStageAttemptFact>,
    stage_entry_facts: Vec<TournamentStageEntryFact>,
    ko_money_event_facts: Vec<TournamentKoMoneyEventFact>,
    mystery_envelope_facts: Vec<MysteryEnvelopeFact>,
    pre_ft_chip_facts: Vec<TournamentPreFtChipFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SeedStatAccumulator {
    pub summary_tournament_count: u64,
    pub summary_finish_place_count: u64,
    pub hand_tournament_count: u64,
    pub total_buyin_cents: i64,
    pub total_payout_cents: i64,
    pub finish_place_sum: i64,
    pub tournaments_with_ft_reach: u64,
    pub total_ko_event_count: u64,
    pub early_ft_ko_event_count: u64,
}

pub fn query_seed_stats(
    client: &mut impl GenericClient,
    filters: SeedStatsFilters,
) -> Result<SeedStatSnapshot> {
    let inputs = load_canonical_query_inputs(client, filters)?;
    Ok(build_seed_stat_snapshot(build_seed_stat_accumulator(
        &inputs.summary_facts,
        &inputs.hand_covered_tournaments,
        &inputs.ft_helper_facts,
        &inputs.ko_event_facts,
    )))
}

pub fn query_canonical_stats(
    client: &mut impl GenericClient,
    filters: SeedStatsFilters,
) -> Result<CanonicalStatSnapshot> {
    let inputs = load_canonical_query_inputs(client, filters)?;
    Ok(build_canonical_stat_snapshot(&inputs))
}

fn load_canonical_query_inputs(
    client: &mut impl GenericClient,
    filters: SeedStatsFilters,
) -> Result<CanonicalQueryInputs> {
    let buyin_filter = filters
        .buyin_total_cents
        .as_ref()
        .map(|values| values.iter().copied().collect::<BTreeSet<_>>());
    let tournament_buyin_facts =
        load_tournament_buyin_facts(client, filters.organization_id, filters.player_profile_id)?;
    let allowed_tournaments = tournament_buyin_facts
        .iter()
        .filter(|fact| match &buyin_filter {
            Some(set) => set.contains(&fact.buyin_total_cents),
            None => true,
        })
        .map(|fact| fact.tournament_id)
        .collect::<BTreeSet<_>>();
    let tournament_buyin_facts = tournament_buyin_facts
        .into_iter()
        .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
        .collect::<Vec<_>>();
    let summary_facts =
        load_summary_tournament_facts(client, filters.organization_id, filters.player_profile_id)?
            .into_iter()
            .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
            .collect::<Vec<_>>();
    let hand_covered_tournaments = load_hand_covered_tournament_ids(
        client,
        filters.organization_id,
        filters.player_profile_id,
    )?
    .into_iter()
    .filter(|tournament_id| allowed_tournaments.contains(tournament_id))
    .collect::<Vec<_>>();
    let ft_helper_facts = load_tournament_ft_helper_facts(
        client,
        filters.organization_id,
        filters.player_profile_id,
    )?
    .into_iter()
    .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
    .collect::<Vec<_>>();
    let ko_event_facts =
        load_tournament_ko_event_facts(client, filters.organization_id, filters.player_profile_id)?
            .into_iter()
            .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
            .collect::<Vec<_>>();
    let deep_ft_entry_facts =
        load_deep_ft_entry_facts(client, filters.organization_id, filters.player_profile_id)?
            .into_iter()
            .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
            .collect::<Vec<_>>();
    let stage_event_facts =
        load_stage_event_facts(client, filters.organization_id, filters.player_profile_id)?
            .into_iter()
            .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
            .collect::<Vec<_>>();
    let stage_attempt_facts =
        load_stage_attempt_facts(client, filters.organization_id, filters.player_profile_id)?
            .into_iter()
            .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
            .collect::<Vec<_>>();
    let stage_entry_facts =
        load_stage_entry_facts(client, filters.organization_id, filters.player_profile_id)?
            .into_iter()
            .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
            .collect::<Vec<_>>();
    let ko_money_event_facts = load_tournament_ko_money_event_facts(
        client,
        filters.organization_id,
        filters.player_profile_id,
    )?
    .into_iter()
    .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
    .collect::<Vec<_>>();
    let mystery_envelope_facts = load_mystery_envelope_facts(client)?;
    let pre_ft_chip_facts =
        load_pre_ft_chip_facts(client, filters.organization_id, filters.player_profile_id)?
            .into_iter()
            .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
            .collect::<Vec<_>>();

    Ok(CanonicalQueryInputs {
        tournament_buyin_facts,
        summary_facts,
        hand_covered_tournaments,
        ft_helper_facts,
        ko_event_facts,
        deep_ft_entry_facts,
        stage_event_facts,
        stage_attempt_facts,
        stage_entry_facts,
        ko_money_event_facts,
        mystery_envelope_facts,
        pre_ft_chip_facts,
    })
}

fn load_tournament_buyin_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<TournamentBuyinFact>> {
    let rows = client.query(
        "SELECT id, (buyin_total * 100)::bigint
         FROM core.tournaments
         WHERE organization_id = $1
           AND player_profile_id = $2",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| TournamentBuyinFact {
            tournament_id: row.get(0),
            buyin_total_cents: row.get(1),
        })
        .collect())
}

fn load_summary_tournament_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<SummaryTournamentFact>> {
    let rows = client.query(
        "SELECT
            t.id,
            (t.buyin_total * 100)::bigint,
            COALESCE((te.total_payout_money * 100)::bigint, 0::bigint),
            COALESCE((te.regular_prize_money * 100)::bigint, 0::bigint),
            te.finish_place
         FROM core.tournaments t
         INNER JOIN core.tournament_entries te
           ON te.tournament_id = t.id
          AND te.player_profile_id = t.player_profile_id
         WHERE t.organization_id = $1
           AND t.player_profile_id = $2",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| SummaryTournamentFact {
            tournament_id: row.get(0),
            buyin_total_cents: row.get(1),
            payout_cents: row.get(2),
            regular_prize_cents: row.get(3),
            finish_place: row.get(4),
        })
        .collect())
}

fn load_hand_covered_tournament_ids(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<Uuid>> {
    let rows = client.query(
        "SELECT DISTINCT h.tournament_id
         FROM core.hands h
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

fn load_tournament_ft_helper_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<TournamentFtHelperFact>> {
    let rows = client.query(
        "SELECT
            helper.tournament_id,
            helper.reached_ft_exact,
            helper.ft_started_incomplete,
            helper.deepest_ft_size_reached,
            helper.hero_ft_entry_stack_chips,
            helper.hero_ft_entry_stack_bb::double precision
         FROM derived.mbr_tournament_ft_helper helper
         INNER JOIN core.tournaments t
           ON t.id = helper.tournament_id
         WHERE t.organization_id = $1
           AND helper.player_profile_id = $2",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| TournamentFtHelperFact {
            tournament_id: row.get(0),
            reached_ft_exact: row.get(1),
            ft_started_incomplete: row.get(2),
            deepest_ft_size_reached: row.get(3),
            hero_ft_entry_stack_chips: row.get(4),
            hero_ft_entry_stack_bb: row.get(5),
        })
        .collect())
}

fn load_tournament_ko_event_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<TournamentKoEventFact>> {
    let rows = client.query(
        "SELECT
            h.tournament_id,
            COUNT(*) FILTER (
                WHERE he.hero_involved
                  AND he.certainty_state = 'exact'
            )::bigint AS total_exact_ko_event_count,
            COUNT(*) FILTER (
                WHERE he.hero_involved
                  AND he.certainty_state = 'exact'
                  AND msr.is_stage_6_9
            )::bigint AS early_ft_exact_ko_event_count
         FROM core.hands h
         LEFT JOIN derived.mbr_stage_resolution msr
           ON msr.hand_id = h.id
          AND msr.player_profile_id = h.player_profile_id
         LEFT JOIN derived.hand_eliminations he
           ON he.hand_id = h.id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2
         GROUP BY h.tournament_id",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| TournamentKoEventFact {
            tournament_id: row.get(0),
            total_exact_ko_event_count: row.get::<_, i64>(1) as u64,
            early_ft_exact_ko_event_count: row.get::<_, i64>(2) as u64,
        })
        .collect())
}

fn load_deep_ft_entry_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<DeepFtEntryFact>> {
    let rows = client.query(
        "SELECT DISTINCT ON (h.tournament_id)
            h.tournament_id,
            hs.starting_stack,
            CASE
                WHEN h.big_blind > 0
                THEN hs.starting_stack::double precision / h.big_blind::double precision
                ELSE NULL
            END
         FROM core.hands h
         INNER JOIN derived.mbr_stage_resolution msr
           ON msr.hand_id = h.id
          AND msr.player_profile_id = h.player_profile_id
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = h.id
          AND hs.is_hero IS TRUE
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2
           AND msr.ft_players_remaining_exact IS NOT NULL
           AND msr.ft_players_remaining_exact <= 5
         ORDER BY
            h.tournament_id,
            h.hand_started_at_local NULLS LAST,
            h.external_hand_id,
            h.id",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| DeepFtEntryFact {
            tournament_id: row.get(0),
            hero_stack_chips: row.get(1),
            hero_stack_bb: row.get(2),
        })
        .collect())
}

fn load_stage_event_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<TournamentStageEventFact>> {
    let rows = client.query(
        "SELECT
            h.tournament_id,
            COUNT(*) FILTER (
                WHERE he.certainty_state = 'exact'
                  AND eliminated_seat.is_hero IS TRUE
                  AND msr.is_stage_6_9
            )::bigint AS early_ft_bust_count,
            COUNT(*) FILTER (
                WHERE he.hero_involved IS TRUE
                  AND he.certainty_state = 'exact'
                  AND msr.ft_players_remaining_exact IN (2, 3)
            )::bigint AS ko_stage_2_3_event_count,
            COUNT(*) FILTER (
                WHERE he.hero_involved IS TRUE
                  AND he.certainty_state = 'exact'
                  AND msr.is_stage_3_4
            )::bigint AS ko_stage_3_4_event_count,
            COUNT(*) FILTER (
                WHERE he.hero_involved IS TRUE
                  AND he.certainty_state = 'exact'
                  AND msr.is_stage_4_5
            )::bigint AS ko_stage_4_5_event_count,
            COUNT(*) FILTER (
                WHERE he.hero_involved IS TRUE
                  AND he.certainty_state = 'exact'
                  AND msr.is_stage_5_6
            )::bigint AS ko_stage_5_6_event_count,
            COUNT(*) FILTER (
                WHERE he.hero_involved IS TRUE
                  AND he.certainty_state = 'exact'
                  AND msr.is_stage_6_9
            )::bigint AS ko_stage_6_9_event_count,
            COUNT(*) FILTER (
                WHERE he.hero_involved IS TRUE
                  AND he.certainty_state = 'exact'
                  AND msr.ft_players_remaining_exact IN (7, 8, 9)
            )::bigint AS ko_stage_7_9_event_count,
            COUNT(*) FILTER (
                WHERE he.hero_involved IS TRUE
                  AND he.certainty_state = 'exact'
                  AND helper.first_ft_hand_started_local IS NOT NULL
                  AND h.hand_started_at_local IS NOT NULL
                  AND COALESCE(msr.is_boundary_hand, FALSE) IS FALSE
                  AND h.hand_started_at_local < helper.first_ft_hand_started_local
            )::bigint AS pre_ft_ko_count
         FROM core.hands h
         LEFT JOIN derived.mbr_stage_resolution msr
           ON msr.hand_id = h.id
          AND msr.player_profile_id = h.player_profile_id
         LEFT JOIN derived.hand_eliminations he
           ON he.hand_id = h.id
         LEFT JOIN core.hand_seats eliminated_seat
           ON eliminated_seat.hand_id = he.hand_id
          AND eliminated_seat.seat_no = he.eliminated_seat_no
         LEFT JOIN derived.mbr_tournament_ft_helper helper
           ON helper.tournament_id = h.tournament_id
          AND helper.player_profile_id = h.player_profile_id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2
         GROUP BY h.tournament_id",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| TournamentStageEventFact {
            tournament_id: row.get(0),
            early_ft_bust_count: row.get::<_, i64>(1) as u64,
            ko_stage_2_3_event_count: row.get::<_, i64>(2) as u64,
            ko_stage_3_4_event_count: row.get::<_, i64>(3) as u64,
            ko_stage_4_5_event_count: row.get::<_, i64>(4) as u64,
            ko_stage_5_6_event_count: row.get::<_, i64>(5) as u64,
            ko_stage_6_9_event_count: row.get::<_, i64>(6) as u64,
            ko_stage_7_9_event_count: row.get::<_, i64>(7) as u64,
            pre_ft_ko_count: row.get::<_, i64>(8) as u64,
        })
        .collect())
}

fn load_stage_attempt_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<TournamentStageAttemptFact>> {
    let rows = client.query(
        "WITH attempt_targets AS (
            SELECT DISTINCT
                h.tournament_id,
                h.id AS hand_id,
                target.seat_no AS target_seat_no
             FROM core.hands h
             INNER JOIN core.hand_seats hero_seat
               ON hero_seat.hand_id = h.id
              AND hero_seat.is_hero IS TRUE
             INNER JOIN core.hand_seats target
               ON target.hand_id = h.id
              AND target.is_hero IS FALSE
              AND target.starting_stack > 0
              AND hero_seat.starting_stack >= target.starting_stack
             WHERE h.organization_id = $1
               AND h.player_profile_id = $2
               AND EXISTS (
                   SELECT 1
                   FROM core.hand_actions target_action
                   WHERE target_action.hand_id = h.id
                     AND target_action.seat_no = target.seat_no
                     AND target_action.is_all_in IS TRUE
               )
               AND EXISTS (
                   SELECT 1
                   FROM core.hand_pot_eligibility hero_pe
                   INNER JOIN core.hand_pot_eligibility target_pe
                     ON target_pe.hand_id = hero_pe.hand_id
                    AND target_pe.pot_no = hero_pe.pot_no
                   WHERE hero_pe.hand_id = h.id
                     AND hero_pe.seat_no = hero_seat.seat_no
                     AND target_pe.seat_no = target.seat_no
               )
         )
         SELECT
            attempts.tournament_id,
            COUNT(*) FILTER (
                WHERE msr.ft_players_remaining_exact IN (2, 3)
            )::bigint AS ko_stage_2_3_attempt_count,
            COUNT(*) FILTER (
                WHERE msr.is_stage_3_4
            )::bigint AS ko_stage_3_4_attempt_count,
            COUNT(*) FILTER (
                WHERE msr.is_stage_4_5
            )::bigint AS ko_stage_4_5_attempt_count,
            COUNT(*) FILTER (
                WHERE msr.is_stage_5_6
            )::bigint AS ko_stage_5_6_attempt_count,
            COUNT(*) FILTER (
                WHERE msr.is_stage_6_9
            )::bigint AS ko_stage_6_9_attempt_count,
            COUNT(*) FILTER (
                WHERE msr.ft_players_remaining_exact IN (7, 8, 9)
            )::bigint AS ko_stage_7_9_attempt_count
         FROM attempt_targets attempts
         INNER JOIN derived.mbr_stage_resolution msr
           ON msr.hand_id = attempts.hand_id
          AND msr.player_profile_id = $2
         GROUP BY attempts.tournament_id",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| TournamentStageAttemptFact {
            tournament_id: row.get(0),
            ko_stage_2_3_attempt_count: row.get::<_, i64>(1) as u64,
            ko_stage_3_4_attempt_count: row.get::<_, i64>(2) as u64,
            ko_stage_4_5_attempt_count: row.get::<_, i64>(3) as u64,
            ko_stage_5_6_attempt_count: row.get::<_, i64>(4) as u64,
            ko_stage_6_9_attempt_count: row.get::<_, i64>(5) as u64,
            ko_stage_7_9_attempt_count: row.get::<_, i64>(6) as u64,
        })
        .collect())
}

fn load_stage_entry_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<TournamentStageEntryFact>> {
    let rows = client.query(
        "WITH stage_reach AS (
            SELECT
                h.tournament_id,
                BOOL_OR(msr.ft_players_remaining_exact IN (2, 3)) AS reached_stage_2_3,
                BOOL_OR(msr.is_stage_3_4) AS reached_stage_3_4,
                BOOL_OR(msr.is_stage_4_5) AS reached_stage_4_5,
                BOOL_OR(msr.is_stage_5_6) AS reached_stage_5_6,
                BOOL_OR(msr.ft_players_remaining_exact IN (7, 8, 9)) AS reached_stage_7_9
             FROM core.hands h
             INNER JOIN derived.mbr_stage_resolution msr
               ON msr.hand_id = h.id
              AND msr.player_profile_id = h.player_profile_id
             WHERE h.organization_id = $1
               AND h.player_profile_id = $2
             GROUP BY h.tournament_id
         ),
         first_stage_5_6_entries AS (
            SELECT DISTINCT ON (h.tournament_id)
                h.tournament_id,
                CASE
                    WHEN h.big_blind > 0
                    THEN hs.starting_stack::double precision / h.big_blind::double precision
                    ELSE NULL
                END AS hero_stage_5_6_stack_bb
             FROM core.hands h
             INNER JOIN derived.mbr_stage_resolution msr
               ON msr.hand_id = h.id
              AND msr.player_profile_id = h.player_profile_id
             INNER JOIN core.hand_seats hs
               ON hs.hand_id = h.id
              AND hs.is_hero IS TRUE
             WHERE h.organization_id = $1
               AND h.player_profile_id = $2
               AND msr.is_stage_5_6
             ORDER BY
                h.tournament_id,
                h.hand_started_at_local NULLS LAST,
                h.external_hand_id,
                h.id
         ),
         first_stage_3_4_entries AS (
            SELECT DISTINCT ON (h.tournament_id)
                h.tournament_id,
                CASE
                    WHEN h.big_blind > 0
                    THEN hs.starting_stack::double precision / h.big_blind::double precision
                    ELSE NULL
                END AS hero_stage_3_4_stack_bb
             FROM core.hands h
             INNER JOIN derived.mbr_stage_resolution msr
               ON msr.hand_id = h.id
              AND msr.player_profile_id = h.player_profile_id
             INNER JOIN core.hand_seats hs
               ON hs.hand_id = h.id
              AND hs.is_hero IS TRUE
             WHERE h.organization_id = $1
               AND h.player_profile_id = $2
               AND msr.is_stage_3_4
             ORDER BY
                h.tournament_id,
                h.hand_started_at_local NULLS LAST,
                h.external_hand_id,
                h.id
         )
         SELECT
            reach.tournament_id,
            reach.reached_stage_2_3,
            reach.reached_stage_3_4,
            reach.reached_stage_4_5,
            reach.reached_stage_5_6,
            reach.reached_stage_7_9,
            stage_5_6.hero_stage_5_6_stack_bb,
            stage_3_4.hero_stage_3_4_stack_bb
         FROM stage_reach reach
         LEFT JOIN first_stage_5_6_entries stage_5_6
           ON stage_5_6.tournament_id = reach.tournament_id
         LEFT JOIN first_stage_3_4_entries stage_3_4
           ON stage_3_4.tournament_id = reach.tournament_id",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| TournamentStageEntryFact {
            tournament_id: row.get(0),
            reached_stage_2_3: row.get(1),
            reached_stage_3_4: row.get(2),
            reached_stage_4_5: row.get(3),
            reached_stage_5_6: row.get(4),
            reached_stage_7_9: row.get(5),
            hero_stage_5_6_stack_bb: row.get(6),
            hero_stage_3_4_stack_bb: row.get(7),
        })
        .collect())
}

fn load_tournament_ko_money_event_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<TournamentKoMoneyEventFact>> {
    let rows = client.query(
        "SELECT
            h.tournament_id,
            (COALESCE(he.hero_ko_share_total, he.hero_share_fraction) * 1000000)::bigint,
            COALESCE(msr.ft_players_remaining_exact IN (2, 3), FALSE),
            COALESCE(msr.is_stage_3_4, FALSE),
            COALESCE(msr.is_stage_4_5, FALSE),
            COALESCE(msr.is_stage_5_6, FALSE),
            COALESCE(msr.is_stage_6_9, FALSE),
            COALESCE(msr.ft_players_remaining_exact IN (7, 8, 9), FALSE)
         FROM core.hands h
         INNER JOIN derived.hand_eliminations he
           ON he.hand_id = h.id
         LEFT JOIN derived.mbr_stage_resolution msr
           ON msr.hand_id = h.id
          AND msr.player_profile_id = h.player_profile_id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2
           AND he.hero_involved IS TRUE
           AND he.certainty_state = 'exact'
           AND COALESCE(he.hero_ko_share_total, he.hero_share_fraction) IS NOT NULL
           AND COALESCE(he.hero_ko_share_total, he.hero_share_fraction) > 0",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| TournamentKoMoneyEventFact {
            tournament_id: row.get(0),
            share_micros: row.get(1),
            is_stage_2_3: row.get(2),
            is_stage_3_4: row.get(3),
            is_stage_4_5: row.get(4),
            is_stage_5_6: row.get(5),
            is_stage_6_9: row.get(6),
            is_stage_7_9: row.get(7),
        })
        .collect())
}

fn load_mystery_envelope_facts(
    client: &mut impl GenericClient,
) -> Result<Vec<MysteryEnvelopeFact>> {
    let rows = client.query(
        "SELECT
            (cfg.buyin_total * 100)::bigint,
            envelope.sort_order,
            (envelope.payout_money * 100)::bigint,
            envelope.frequency_per_100m
         FROM ref.mbr_mystery_envelopes envelope
         INNER JOIN ref.mbr_buyin_configs cfg
           ON cfg.id = envelope.buyin_config_id",
        &[],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| MysteryEnvelopeFact {
            buyin_total_cents: row.get(0),
            sort_order: row.get(1),
            payout_cents: row.get(2),
            frequency_per_100m: row.get(3),
        })
        .collect())
}

fn load_pre_ft_chip_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<TournamentPreFtChipFact>> {
    let rows = client.query(
        "SELECT
            helper.tournament_id,
            COALESCE(pre_ft_snapshot.hero_final_stack, 1000::bigint) - 1000::bigint
         FROM derived.mbr_tournament_ft_helper helper
         INNER JOIN core.tournaments t
           ON t.id = helper.tournament_id
         LEFT JOIN LATERAL (
            SELECT
                (resolution.final_stacks ->> hero.player_name)::bigint AS hero_final_stack
            FROM core.hands h
            INNER JOIN core.hand_seats hero
              ON hero.hand_id = h.id
             AND hero.is_hero IS TRUE
            INNER JOIN derived.hand_state_resolutions resolution
              ON resolution.hand_id = h.id
             AND resolution.resolution_version = 'gg_mbr_v1'
            LEFT JOIN derived.mbr_stage_resolution msr
              ON msr.hand_id = h.id
             AND msr.player_profile_id = h.player_profile_id
            WHERE h.tournament_id = helper.tournament_id
              AND h.player_profile_id = helper.player_profile_id
              AND (
                  helper.first_ft_hand_started_local IS NULL
                  OR (
                      helper.boundary_resolution_state = 'exact'
                      AND h.hand_started_at_local IS NOT NULL
                      AND h.hand_started_at_local < helper.first_ft_hand_started_local
                      AND COALESCE(msr.is_boundary_hand, FALSE) IS FALSE
                  )
              )
            ORDER BY
                h.hand_started_at_local DESC NULLS LAST,
                h.external_hand_id DESC,
                h.id DESC
            LIMIT 1
         ) AS pre_ft_snapshot
           ON TRUE
         WHERE t.organization_id = $1
           AND helper.player_profile_id = $2
           AND (
               helper.first_ft_hand_started_local IS NULL
               OR helper.boundary_resolution_state = 'exact'
           )",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| TournamentPreFtChipFact {
            tournament_id: row.get(0),
            chip_delta: row.get(1),
        })
        .collect())
}

fn build_seed_stat_accumulator(
    summary_facts: &[SummaryTournamentFact],
    hand_covered_tournaments: &[Uuid],
    ft_helper_facts: &[TournamentFtHelperFact],
    ko_event_facts: &[TournamentKoEventFact],
) -> SeedStatAccumulator {
    let ft_helper_by_tournament = ft_helper_facts
        .iter()
        .copied()
        .map(|fact| (fact.tournament_id, fact.reached_ft_exact))
        .collect::<BTreeMap<_, _>>();
    let ko_events_by_tournament = ko_event_facts
        .iter()
        .copied()
        .map(|fact| (fact.tournament_id, fact))
        .collect::<BTreeMap<_, _>>();

    let mut tournaments_with_ft_reach = 0_u64;
    let mut total_ko_event_count = 0_u64;
    let mut early_ft_ko_event_count = 0_u64;

    for tournament_id in hand_covered_tournaments {
        if ft_helper_by_tournament
            .get(tournament_id)
            .copied()
            .unwrap_or(false)
        {
            tournaments_with_ft_reach += 1;
        }
        if let Some(ko_fact) = ko_events_by_tournament.get(tournament_id) {
            total_ko_event_count += ko_fact.total_exact_ko_event_count;
            early_ft_ko_event_count += ko_fact.early_ft_exact_ko_event_count;
        }
    }

    SeedStatAccumulator {
        summary_tournament_count: summary_facts.len() as u64,
        summary_finish_place_count: summary_facts
            .iter()
            .filter(|fact| fact.finish_place.is_some())
            .count() as u64,
        hand_tournament_count: hand_covered_tournaments.len() as u64,
        total_buyin_cents: summary_facts
            .iter()
            .map(|fact| fact.buyin_total_cents)
            .sum(),
        total_payout_cents: summary_facts.iter().map(|fact| fact.payout_cents).sum(),
        finish_place_sum: summary_facts
            .iter()
            .filter_map(|fact| fact.finish_place)
            .map(i64::from)
            .sum(),
        tournaments_with_ft_reach,
        total_ko_event_count,
        early_ft_ko_event_count,
    }
}

fn build_canonical_stat_snapshot(inputs: &CanonicalQueryInputs) -> CanonicalStatSnapshot {
    let tournament_buyin_facts = &inputs.tournament_buyin_facts;
    let summary_facts = &inputs.summary_facts;
    let hand_covered_tournaments = &inputs.hand_covered_tournaments;
    let ft_helper_facts = &inputs.ft_helper_facts;
    let ko_event_facts = &inputs.ko_event_facts;
    let deep_ft_entry_facts = &inputs.deep_ft_entry_facts;
    let stage_event_facts = &inputs.stage_event_facts;
    let stage_attempt_facts = &inputs.stage_attempt_facts;
    let stage_entry_facts = &inputs.stage_entry_facts;
    let ko_money_event_facts = &inputs.ko_money_event_facts;
    let mystery_envelope_facts = &inputs.mystery_envelope_facts;
    let pre_ft_chip_facts = &inputs.pre_ft_chip_facts;

    let seed_snapshot = build_seed_stat_snapshot(build_seed_stat_accumulator(
        summary_facts,
        hand_covered_tournaments,
        ft_helper_facts,
        ko_event_facts,
    ));
    let mut canonical = seed_snapshot.to_canonical_snapshot();
    let ft_helper_by_tournament = ft_helper_facts
        .iter()
        .copied()
        .map(|fact| (fact.tournament_id, fact))
        .collect::<BTreeMap<_, _>>();
    let hand_covered_tournament_set = hand_covered_tournaments
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let tournament_buyin_by_tournament = tournament_buyin_facts
        .iter()
        .copied()
        .map(|fact| (fact.tournament_id, fact.buyin_total_cents))
        .collect::<BTreeMap<_, _>>();
    let envelopes_by_buyin = mystery_envelope_facts.iter().fold(
        BTreeMap::<i64, Vec<MysteryEnvelope>>::new(),
        |mut acc, fact| {
            acc.entry(fact.buyin_total_cents)
                .or_default()
                .push(MysteryEnvelope {
                    sort_order: fact.sort_order,
                    payout_cents: fact.payout_cents,
                    frequency_per_100m: fact.frequency_per_100m,
                });
            acc
        },
    );
    let ko_money_events_by_tournament = ko_money_event_facts.iter().copied().fold(
        BTreeMap::<Uuid, Vec<TournamentKoMoneyEventFact>>::new(),
        |mut acc, fact| {
            acc.entry(fact.tournament_id).or_default().push(fact);
            acc
        },
    );
    let estimated_ko_cents_by_tournament = tournament_buyin_by_tournament
        .iter()
        .filter_map(|(tournament_id, buyin_total_cents)| {
            let envelopes = envelopes_by_buyin.get(buyin_total_cents)?;
            let expected_cents = ko_money_events_by_tournament
                .get(tournament_id)
                .map(|events| {
                    events
                        .iter()
                        .filter_map(|event| {
                            expected_hero_mystery_cents(event.share_micros, envelopes)
                        })
                        .sum::<f64>()
                })
                .unwrap_or(0.0);
            Some((*tournament_id, expected_cents))
        })
        .collect::<BTreeMap<_, _>>();
    let mut big_ko_bucket_counts = BTreeMap::from([
        ("big_ko_x1_5_count".to_string(), 0.0),
        ("big_ko_x2_count".to_string(), 0.0),
        ("big_ko_x10_count".to_string(), 0.0),
        ("big_ko_x100_count".to_string(), 0.0),
        ("big_ko_x1000_count".to_string(), 0.0),
        ("big_ko_x10000_count".to_string(), 0.0),
    ]);
    let mut ko_stage_2_3_money_cents = 0.0;
    let mut ko_stage_3_4_money_cents = 0.0;
    let mut ko_stage_4_5_money_cents = 0.0;
    let mut ko_stage_5_6_money_cents = 0.0;
    let mut ko_stage_6_9_money_cents = 0.0;
    let mut ko_stage_7_9_money_cents = 0.0;
    let mut adjusted_support_tournament_count = 0_u64;
    let mut adjusted_total_buyin_cents = 0_i64;
    let mut adjusted_total_payout_cents = 0_i64;
    let mut adjusted_regular_prize_cents = 0_i64;
    let mut actual_supported_ko_cents = 0_i64;
    let mut estimated_supported_ko_cents = 0.0;

    let mut ft_finish_place_sum = 0_i64;
    let mut ft_finish_place_count = 0_u64;
    let mut no_ft_finish_place_sum = 0_i64;
    let mut no_ft_finish_place_count = 0_u64;
    let mut ft_buyin_cents = 0_i64;
    let mut ft_payout_cents = 0_i64;
    let mut itm_count = 0_u64;
    let mut winnings_from_itm_cents = 0_i64;
    let mut winnings_from_ko_cents = 0_i64;
    let mut total_payout_cents = 0_i64;
    let mut deep_ft_buyin_cents = 0_i64;
    let mut deep_ft_payout_cents = 0_i64;

    for (tournament_id, events) in &ko_money_events_by_tournament {
        let Some(buyin_total_cents) = tournament_buyin_by_tournament.get(tournament_id) else {
            continue;
        };
        let Some(envelopes) = envelopes_by_buyin.get(buyin_total_cents) else {
            continue;
        };
        let bucket_probabilities = expected_big_ko_bucket_probabilities(envelopes);

        for event in events {
            let Some(expected_cents) = expected_hero_mystery_cents(event.share_micros, envelopes)
            else {
                continue;
            };

            if event.is_stage_2_3 {
                ko_stage_2_3_money_cents += expected_cents;
            }
            if event.is_stage_3_4 {
                ko_stage_3_4_money_cents += expected_cents;
            }
            if event.is_stage_4_5 {
                ko_stage_4_5_money_cents += expected_cents;
            }
            if event.is_stage_5_6 {
                ko_stage_5_6_money_cents += expected_cents;
            }
            if event.is_stage_6_9 {
                ko_stage_6_9_money_cents += expected_cents;
            }
            if event.is_stage_7_9 {
                ko_stage_7_9_money_cents += expected_cents;
            }

            for (bucket_key, probability) in &bucket_probabilities {
                if let Some(total) = big_ko_bucket_counts.get_mut(bucket_key) {
                    *total += probability;
                }
            }
        }
    }

    for summary_fact in summary_facts {
        if summary_fact.regular_prize_cents > 0 {
            itm_count += 1;
        }
        winnings_from_itm_cents += summary_fact.regular_prize_cents;
        winnings_from_ko_cents += summary_fact.payout_cents - summary_fact.regular_prize_cents;
        total_payout_cents += summary_fact.payout_cents;

        if let Some(ft_helper_fact) = ft_helper_by_tournament.get(&summary_fact.tournament_id) {
            if ft_helper_fact.reached_ft_exact {
                if let Some(finish_place) = summary_fact.finish_place {
                    ft_finish_place_sum += i64::from(finish_place);
                    ft_finish_place_count += 1;
                }
                ft_buyin_cents += summary_fact.buyin_total_cents;
                ft_payout_cents += summary_fact.payout_cents;
            } else if let Some(finish_place) = summary_fact.finish_place {
                no_ft_finish_place_sum += i64::from(finish_place);
                no_ft_finish_place_count += 1;
            }

            if matches!(ft_helper_fact.deepest_ft_size_reached, Some(0..=5)) {
                deep_ft_buyin_cents += summary_fact.buyin_total_cents;
                deep_ft_payout_cents += summary_fact.payout_cents;
            }
        }

        if hand_covered_tournament_set.contains(&summary_fact.tournament_id)
            && let Some(estimated_ko_cents) =
                estimated_ko_cents_by_tournament.get(&summary_fact.tournament_id)
        {
            adjusted_support_tournament_count += 1;
            adjusted_total_buyin_cents += summary_fact.buyin_total_cents;
            adjusted_total_payout_cents += summary_fact.payout_cents;
            adjusted_regular_prize_cents += summary_fact.regular_prize_cents;
            actual_supported_ko_cents +=
                summary_fact.payout_cents - summary_fact.regular_prize_cents;
            estimated_supported_ko_cents += *estimated_ko_cents;
        }
    }

    let ft_reached_count = ft_helper_facts
        .iter()
        .filter(|fact| fact.reached_ft_exact)
        .count() as u64;
    let incomplete_ft_count = ft_helper_facts
        .iter()
        .filter(|fact| fact.reached_ft_exact && fact.ft_started_incomplete == Some(true))
        .count() as u64;

    let avg_ft_initial_stack_chips = average_i64(
        ft_helper_facts
            .iter()
            .filter_map(|fact| fact.hero_ft_entry_stack_chips)
            .sum(),
        ft_helper_facts
            .iter()
            .filter(|fact| fact.hero_ft_entry_stack_chips.is_some())
            .count() as u64,
    );
    let avg_ft_initial_stack_bb = average_f64(
        ft_helper_facts
            .iter()
            .filter_map(|fact| fact.hero_ft_entry_stack_bb)
            .sum(),
        ft_helper_facts
            .iter()
            .filter(|fact| fact.hero_ft_entry_stack_bb.is_some())
            .count() as u64,
    );
    let deep_ft_reach_count = hand_covered_tournaments
        .iter()
        .filter(|tournament_id| {
            ft_helper_by_tournament
                .get(tournament_id)
                .and_then(|fact| fact.deepest_ft_size_reached)
                .is_some_and(|size| size <= 5)
        })
        .count() as u64;
    let deep_ft_avg_stack_chips = average_i64(
        deep_ft_entry_facts
            .iter()
            .filter_map(|fact| fact.hero_stack_chips)
            .sum(),
        deep_ft_entry_facts
            .iter()
            .filter(|fact| fact.hero_stack_chips.is_some())
            .count() as u64,
    );
    let deep_ft_avg_stack_bb = average_f64(
        deep_ft_entry_facts
            .iter()
            .filter_map(|fact| fact.hero_stack_bb)
            .sum(),
        deep_ft_entry_facts
            .iter()
            .filter(|fact| fact.hero_stack_bb.is_some())
            .count() as u64,
    );
    let early_ft_bust_count = stage_event_facts
        .iter()
        .map(|fact| fact.early_ft_bust_count)
        .sum::<u64>();
    let ko_stage_2_3_event_count = stage_event_facts
        .iter()
        .map(|fact| fact.ko_stage_2_3_event_count)
        .sum::<u64>();
    let ko_stage_3_4_event_count = stage_event_facts
        .iter()
        .map(|fact| fact.ko_stage_3_4_event_count)
        .sum::<u64>();
    let ko_stage_4_5_event_count = stage_event_facts
        .iter()
        .map(|fact| fact.ko_stage_4_5_event_count)
        .sum::<u64>();
    let ko_stage_5_6_event_count = stage_event_facts
        .iter()
        .map(|fact| fact.ko_stage_5_6_event_count)
        .sum::<u64>();
    let ko_stage_6_9_event_count = stage_event_facts
        .iter()
        .map(|fact| fact.ko_stage_6_9_event_count)
        .sum::<u64>();
    let ko_stage_7_9_event_count = stage_event_facts
        .iter()
        .map(|fact| fact.ko_stage_7_9_event_count)
        .sum::<u64>();
    let pre_ft_ko_count = stage_event_facts
        .iter()
        .map(|fact| fact.pre_ft_ko_count)
        .sum::<u64>();
    let ko_stage_2_3_attempt_count = stage_attempt_facts
        .iter()
        .map(|fact| fact.ko_stage_2_3_attempt_count)
        .sum::<u64>();
    let ko_stage_3_4_attempt_count = stage_attempt_facts
        .iter()
        .map(|fact| fact.ko_stage_3_4_attempt_count)
        .sum::<u64>();
    let ko_stage_4_5_attempt_count = stage_attempt_facts
        .iter()
        .map(|fact| fact.ko_stage_4_5_attempt_count)
        .sum::<u64>();
    let ko_stage_5_6_attempt_count = stage_attempt_facts
        .iter()
        .map(|fact| fact.ko_stage_5_6_attempt_count)
        .sum::<u64>();
    let ko_stage_6_9_attempt_count = stage_attempt_facts
        .iter()
        .map(|fact| fact.ko_stage_6_9_attempt_count)
        .sum::<u64>();
    let ko_stage_7_9_attempt_count = stage_attempt_facts
        .iter()
        .map(|fact| fact.ko_stage_7_9_attempt_count)
        .sum::<u64>();
    let stage_2_3_tournament_count = stage_entry_facts
        .iter()
        .filter(|fact| fact.reached_stage_2_3)
        .count() as u64;
    let stage_3_4_tournament_count = stage_entry_facts
        .iter()
        .filter(|fact| fact.reached_stage_3_4)
        .count() as u64;
    let stage_4_5_tournament_count = stage_entry_facts
        .iter()
        .filter(|fact| fact.reached_stage_4_5)
        .count() as u64;
    let stage_5_6_tournament_count = stage_entry_facts
        .iter()
        .filter(|fact| fact.reached_stage_5_6)
        .count() as u64;
    let stage_7_9_tournament_count = stage_entry_facts
        .iter()
        .filter(|fact| fact.reached_stage_7_9)
        .count() as u64;
    let ft_entry_stack_bb_total = ft_helper_facts
        .iter()
        .filter(|fact| fact.reached_ft_exact)
        .filter_map(|fact| fact.hero_ft_entry_stack_bb)
        .sum::<f64>();
    let stage_5_6_entry_stack_bb_total = stage_entry_facts
        .iter()
        .filter_map(|fact| fact.hero_stage_5_6_stack_bb)
        .sum::<f64>();
    let stage_3_4_entry_stack_bb_total = stage_entry_facts
        .iter()
        .filter_map(|fact| fact.hero_stage_3_4_stack_bb)
        .sum::<f64>();
    let pre_ft_chip_delta_sum = pre_ft_chip_facts
        .iter()
        .map(|fact| fact.chip_delta)
        .sum::<i64>();
    let pre_ft_tournament_count = pre_ft_chip_facts.len() as u64;

    canonical.values.insert(
        "avg_finish_place_ft".to_string(),
        CanonicalStatPoint::from_optional_float(average_i64(
            ft_finish_place_sum,
            ft_finish_place_count,
        )),
    );
    canonical.values.insert(
        "avg_finish_place_no_ft".to_string(),
        CanonicalStatPoint::from_optional_float(average_i64(
            no_ft_finish_place_sum,
            no_ft_finish_place_count,
        )),
    );
    canonical.values.insert(
        "avg_ft_initial_stack_chips".to_string(),
        CanonicalStatPoint::from_optional_float(avg_ft_initial_stack_chips),
    );
    canonical.values.insert(
        "avg_ft_initial_stack_bb".to_string(),
        CanonicalStatPoint::from_optional_float(avg_ft_initial_stack_bb),
    );
    canonical.values.insert(
        "incomplete_ft_percent".to_string(),
        CanonicalStatPoint::from_optional_float(ratio_to_percent(
            incomplete_ft_count,
            ft_reached_count,
        )),
    );
    canonical.values.insert(
        "itm_percent".to_string(),
        CanonicalStatPoint::from_optional_float(ratio_to_percent(
            itm_count,
            summary_facts.len() as u64,
        )),
    );
    canonical.values.insert(
        "roi_on_ft_pct".to_string(),
        CanonicalStatPoint::from_optional_float(roi_from_totals(ft_payout_cents, ft_buyin_cents)),
    );
    canonical.values.insert(
        "winnings_from_itm".to_string(),
        CanonicalStatPoint::from_optional_float(Some(cents_to_money(winnings_from_itm_cents))),
    );
    canonical.values.insert(
        "winnings_from_ko_total".to_string(),
        CanonicalStatPoint::from_optional_float(Some(cents_to_money(winnings_from_ko_cents))),
    );
    canonical.values.insert(
        "ko_contribution_percent".to_string(),
        CanonicalStatPoint::from_optional_float(portion_to_percent(
            winnings_from_ko_cents,
            total_payout_cents,
        )),
    );
    canonical.values.insert(
        "deep_ft_reach_percent".to_string(),
        CanonicalStatPoint::from_optional_float(ratio_to_percent(
            deep_ft_reach_count,
            hand_covered_tournaments.len() as u64,
        )),
    );
    canonical.values.insert(
        "deep_ft_avg_stack_chips".to_string(),
        CanonicalStatPoint::from_optional_float(deep_ft_avg_stack_chips),
    );
    canonical.values.insert(
        "deep_ft_avg_stack_bb".to_string(),
        CanonicalStatPoint::from_optional_float(deep_ft_avg_stack_bb),
    );
    canonical.values.insert(
        "deep_ft_roi_pct".to_string(),
        CanonicalStatPoint::from_optional_float(roi_from_totals(
            deep_ft_payout_cents,
            deep_ft_buyin_cents,
        )),
    );
    canonical.values.insert(
        "early_ft_bust_count".to_string(),
        CanonicalStatPoint::from_integer(early_ft_bust_count),
    );
    canonical.values.insert(
        "early_ft_bust_per_tournament".to_string(),
        CanonicalStatPoint::from_optional_float(average_u64(early_ft_bust_count, ft_reached_count)),
    );
    canonical.values.insert(
        "ko_stage_2_3_event_count".to_string(),
        CanonicalStatPoint::from_integer(ko_stage_2_3_event_count),
    );
    canonical.values.insert(
        "ko_stage_2_3_money_total".to_string(),
        CanonicalStatPoint::from_optional_float(Some(cents_to_money_f64(ko_stage_2_3_money_cents))),
    );
    canonical.values.insert(
        "ko_stage_2_3_attempts_per_tournament".to_string(),
        CanonicalStatPoint::from_optional_float(average_u64(
            ko_stage_2_3_attempt_count,
            stage_2_3_tournament_count,
        )),
    );
    canonical.values.insert(
        "ko_stage_3_4_event_count".to_string(),
        CanonicalStatPoint::from_integer(ko_stage_3_4_event_count),
    );
    canonical.values.insert(
        "ko_stage_3_4_money_total".to_string(),
        CanonicalStatPoint::from_optional_float(Some(cents_to_money_f64(ko_stage_3_4_money_cents))),
    );
    canonical.values.insert(
        "ko_stage_3_4_attempts_per_tournament".to_string(),
        CanonicalStatPoint::from_optional_float(average_u64(
            ko_stage_3_4_attempt_count,
            stage_3_4_tournament_count,
        )),
    );
    canonical.values.insert(
        "ko_stage_4_5_event_count".to_string(),
        CanonicalStatPoint::from_integer(ko_stage_4_5_event_count),
    );
    canonical.values.insert(
        "ko_stage_4_5_money_total".to_string(),
        CanonicalStatPoint::from_optional_float(Some(cents_to_money_f64(ko_stage_4_5_money_cents))),
    );
    canonical.values.insert(
        "ko_stage_4_5_attempts_per_tournament".to_string(),
        CanonicalStatPoint::from_optional_float(average_u64(
            ko_stage_4_5_attempt_count,
            stage_4_5_tournament_count,
        )),
    );
    canonical.values.insert(
        "ko_stage_5_6_event_count".to_string(),
        CanonicalStatPoint::from_integer(ko_stage_5_6_event_count),
    );
    canonical.values.insert(
        "ko_stage_5_6_money_total".to_string(),
        CanonicalStatPoint::from_optional_float(Some(cents_to_money_f64(ko_stage_5_6_money_cents))),
    );
    canonical.values.insert(
        "ko_stage_5_6_attempts_per_tournament".to_string(),
        CanonicalStatPoint::from_optional_float(average_u64(
            ko_stage_5_6_attempt_count,
            stage_5_6_tournament_count,
        )),
    );
    canonical.values.insert(
        "ko_stage_6_9_event_count".to_string(),
        CanonicalStatPoint::from_integer(ko_stage_6_9_event_count),
    );
    canonical.values.insert(
        "ko_stage_6_9_money_total".to_string(),
        CanonicalStatPoint::from_optional_float(Some(cents_to_money_f64(ko_stage_6_9_money_cents))),
    );
    canonical.values.insert(
        "ko_stage_7_9_event_count".to_string(),
        CanonicalStatPoint::from_integer(ko_stage_7_9_event_count),
    );
    canonical.values.insert(
        "ko_stage_7_9_money_total".to_string(),
        CanonicalStatPoint::from_optional_float(Some(cents_to_money_f64(ko_stage_7_9_money_cents))),
    );
    canonical.values.insert(
        "ko_stage_7_9_attempts_per_tournament".to_string(),
        CanonicalStatPoint::from_optional_float(average_u64(
            ko_stage_7_9_attempt_count,
            stage_7_9_tournament_count,
        )),
    );
    canonical.values.insert(
        "pre_ft_ko_count".to_string(),
        CanonicalStatPoint::from_integer(pre_ft_ko_count),
    );
    canonical.values.insert(
        "ft_stack_conversion".to_string(),
        CanonicalStatPoint::from_optional_float(ratio_to_float(
            ko_stage_6_9_event_count,
            ft_entry_stack_bb_total,
        )),
    );
    canonical.values.insert(
        "avg_ko_attempts_per_ft".to_string(),
        CanonicalStatPoint::from_optional_float(average_u64(
            ko_stage_6_9_attempt_count,
            ft_reached_count,
        )),
    );
    canonical.values.insert(
        "ko_attempts_success_rate".to_string(),
        CanonicalStatPoint::from_optional_float(ratio_to_fraction(
            ko_stage_6_9_event_count,
            ko_stage_6_9_attempt_count,
        )),
    );
    canonical.values.insert(
        "ft_stack_conversion_7_9".to_string(),
        CanonicalStatPoint::from_optional_float(ratio_to_float(
            ko_stage_6_9_event_count,
            ft_entry_stack_bb_total,
        )),
    );
    canonical.values.insert(
        "ft_stack_conversion_7_9_attempts".to_string(),
        CanonicalStatPoint::from_optional_float(average_u64(
            ko_stage_6_9_attempt_count,
            ft_reached_count,
        )),
    );
    canonical.values.insert(
        "ft_stack_conversion_5_6".to_string(),
        CanonicalStatPoint::from_optional_float(ratio_to_float(
            ko_stage_5_6_event_count,
            stage_5_6_entry_stack_bb_total,
        )),
    );
    canonical.values.insert(
        "ft_stack_conversion_5_6_attempts".to_string(),
        CanonicalStatPoint::from_optional_float(average_u64(
            ko_stage_5_6_attempt_count,
            stage_5_6_tournament_count,
        )),
    );
    canonical.values.insert(
        "ft_stack_conversion_3_4".to_string(),
        CanonicalStatPoint::from_optional_float(ratio_to_float(
            ko_stage_3_4_event_count,
            stage_3_4_entry_stack_bb_total,
        )),
    );
    canonical.values.insert(
        "ft_stack_conversion_3_4_attempts".to_string(),
        CanonicalStatPoint::from_optional_float(average_u64(
            ko_stage_3_4_attempt_count,
            stage_3_4_tournament_count,
        )),
    );
    canonical.values.insert(
        "ko_contribution_adjusted_percent".to_string(),
        CanonicalStatPoint::from_optional_float(
            (adjusted_support_tournament_count > 0)
                .then(|| {
                    portion_to_percent_f64(
                        estimated_supported_ko_cents,
                        adjusted_total_payout_cents,
                    )
                })
                .flatten(),
        ),
    );
    canonical.values.insert(
        "ko_luck_money_delta".to_string(),
        CanonicalStatPoint::from_optional_float((adjusted_support_tournament_count > 0).then(
            || cents_to_money_f64(actual_supported_ko_cents as f64 - estimated_supported_ko_cents),
        )),
    );
    canonical.values.insert(
        "roi_adj_pct".to_string(),
        CanonicalStatPoint::from_optional_float(
            (adjusted_support_tournament_count > 0)
                .then(|| {
                    roi_from_adjusted_components(
                        adjusted_regular_prize_cents,
                        estimated_supported_ko_cents,
                        adjusted_total_buyin_cents,
                    )
                })
                .flatten(),
        ),
    );
    canonical.values.insert(
        "pre_ft_chipev".to_string(),
        CanonicalStatPoint::from_optional_float(average_i64(
            pre_ft_chip_delta_sum,
            pre_ft_tournament_count,
        )),
    );
    for (bucket_key, bucket_count) in big_ko_bucket_counts {
        canonical.values.insert(
            bucket_key,
            CanonicalStatPoint::from_optional_float(Some(bucket_count)),
        );
    }

    canonical
}

pub(crate) fn build_seed_stat_snapshot(accumulator: SeedStatAccumulator) -> SeedStatSnapshot {
    let coverage = SeedStatCoverage {
        summary_tournament_count: accumulator.summary_tournament_count,
        hand_tournament_count: accumulator.hand_tournament_count,
    };

    let roi_pct = if accumulator.total_buyin_cents == 0 {
        None
    } else {
        Some(
            ((accumulator.total_payout_cents - accumulator.total_buyin_cents) as f64
                / accumulator.total_buyin_cents as f64)
                * 100.0,
        )
    };
    let avg_finish_place = if accumulator.summary_finish_place_count == 0 {
        None
    } else {
        Some(accumulator.finish_place_sum as f64 / accumulator.summary_finish_place_count as f64)
    };
    let final_table_reach_percent = if accumulator.hand_tournament_count == 0 {
        None
    } else {
        Some(
            accumulator.tournaments_with_ft_reach as f64 / accumulator.hand_tournament_count as f64
                * 100.0,
        )
    };
    let avg_ko_event_per_tournament = if accumulator.hand_tournament_count == 0 {
        None
    } else {
        Some(accumulator.total_ko_event_count as f64 / accumulator.hand_tournament_count as f64)
    };

    SeedStatSnapshot {
        coverage,
        roi_pct,
        avg_finish_place,
        final_table_reach_percent,
        total_ko_event_count: accumulator.total_ko_event_count,
        avg_ko_event_per_tournament,
        early_ft_ko_event_count: accumulator.early_ft_ko_event_count,
        early_ft_ko_event_per_tournament: if accumulator.tournaments_with_ft_reach == 0 {
            None
        } else {
            Some(
                accumulator.early_ft_ko_event_count as f64
                    / accumulator.tournaments_with_ft_reach as f64,
            )
        },
    }
}

fn average_i64(sum: i64, count: u64) -> Option<f64> {
    (count > 0).then_some(sum as f64 / count as f64)
}

fn average_f64(sum: f64, count: u64) -> Option<f64> {
    (count > 0).then_some(sum / count as f64)
}

fn average_u64(sum: u64, count: u64) -> Option<f64> {
    (count > 0).then_some(sum as f64 / count as f64)
}

fn ratio_to_percent(numerator: u64, denominator: u64) -> Option<f64> {
    (denominator > 0).then_some(numerator as f64 / denominator as f64 * 100.0)
}

fn ratio_to_fraction(numerator: u64, denominator: u64) -> Option<f64> {
    (denominator > 0).then_some(numerator as f64 / denominator as f64)
}

fn ratio_to_float(numerator: u64, denominator: f64) -> Option<f64> {
    (denominator > 0.0).then_some(numerator as f64 / denominator)
}

fn portion_to_percent(numerator: i64, denominator: i64) -> Option<f64> {
    (denominator > 0).then_some(numerator as f64 / denominator as f64 * 100.0)
}

fn portion_to_percent_f64(numerator: f64, denominator: i64) -> Option<f64> {
    (denominator > 0).then_some(numerator / denominator as f64 * 100.0)
}

fn roi_from_totals(total_payout_cents: i64, total_buyin_cents: i64) -> Option<f64> {
    if total_buyin_cents == 0 {
        None
    } else {
        Some(((total_payout_cents - total_buyin_cents) as f64 / total_buyin_cents as f64) * 100.0)
    }
}

fn cents_to_money(cents: i64) -> f64 {
    cents as f64 / 100.0
}

fn cents_to_money_f64(cents: f64) -> f64 {
    cents / 100.0
}

fn roi_from_adjusted_components(
    regular_prize_cents: i64,
    estimated_ko_cents: f64,
    total_buyin_cents: i64,
) -> Option<f64> {
    (total_buyin_cents > 0).then_some(
        ((regular_prize_cents as f64 + estimated_ko_cents) - total_buyin_cents as f64)
            / total_buyin_cents as f64
            * 100.0,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CanonicalQueryInputs, DeepFtEntryFact, MysteryEnvelopeFact, SeedStatAccumulator,
        SummaryTournamentFact, TournamentBuyinFact, TournamentFtHelperFact,
        TournamentKoMoneyEventFact, TournamentPreFtChipFact, TournamentStageAttemptFact,
        TournamentStageEntryFact, TournamentStageEventFact, build_canonical_stat_snapshot,
        build_seed_stat_snapshot,
    };
    use crate::models::CanonicalStatPoint;
    use uuid::Uuid;

    #[test]
    fn applies_formulae_and_zero_denominator_rules() {
        let snapshot = build_seed_stat_snapshot(SeedStatAccumulator {
            summary_tournament_count: 0,
            summary_finish_place_count: 0,
            hand_tournament_count: 0,
            total_buyin_cents: 0,
            total_payout_cents: 0,
            finish_place_sum: 0,
            tournaments_with_ft_reach: 0,
            total_ko_event_count: 0,
            early_ft_ko_event_count: 0,
        });

        assert_eq!(snapshot.roi_pct, None);
        assert_eq!(snapshot.avg_finish_place, None);
        assert_eq!(snapshot.final_table_reach_percent, None);
        assert_eq!(snapshot.total_ko_event_count, 0);
        assert_eq!(snapshot.avg_ko_event_per_tournament, None);
        assert_eq!(snapshot.early_ft_ko_event_count, 0);
        assert_eq!(snapshot.early_ft_ko_event_per_tournament, None);

        let populated = build_seed_stat_snapshot(SeedStatAccumulator {
            summary_tournament_count: 4,
            summary_finish_place_count: 4,
            hand_tournament_count: 2,
            total_buyin_cents: 10_000,
            total_payout_cents: 13_000,
            finish_place_sum: 14,
            tournaments_with_ft_reach: 1,
            total_ko_event_count: 3,
            early_ft_ko_event_count: 1,
        });

        assert_eq!(populated.roi_pct, Some(30.0));
        assert_eq!(populated.avg_finish_place, Some(3.5));
        assert_eq!(populated.final_table_reach_percent, Some(50.0));
        assert_eq!(populated.total_ko_event_count, 3);
        assert_eq!(populated.avg_ko_event_per_tournament, Some(1.5));
        assert_eq!(populated.early_ft_ko_event_count, 1);
        assert_eq!(populated.early_ft_ko_event_per_tournament, Some(1.0));
    }

    #[test]
    fn canonical_snapshot_combines_phase_a_summary_and_ft_helper_metrics() {
        let tournament_1 = Uuid::from_u128(1);
        let tournament_2 = Uuid::from_u128(2);
        let tournament_3 = Uuid::from_u128(3);
        let tournament_4 = Uuid::from_u128(4);

        let canonical = build_canonical_stat_snapshot(&CanonicalQueryInputs {
            tournament_buyin_facts: vec![],
            summary_facts: vec![
                SummaryTournamentFact {
                    tournament_id: tournament_1,
                    buyin_total_cents: 2_500,
                    payout_cents: 10_000,
                    regular_prize_cents: 4_000,
                    finish_place: Some(1),
                },
                SummaryTournamentFact {
                    tournament_id: tournament_2,
                    buyin_total_cents: 2_500,
                    payout_cents: 0,
                    regular_prize_cents: 0,
                    finish_place: Some(14),
                },
                SummaryTournamentFact {
                    tournament_id: tournament_3,
                    buyin_total_cents: 2_500,
                    payout_cents: 2_000,
                    regular_prize_cents: 2_000,
                    finish_place: Some(3),
                },
            ],
            hand_covered_tournaments: vec![tournament_1, tournament_2, tournament_4],
            ft_helper_facts: vec![
                TournamentFtHelperFact {
                    tournament_id: tournament_1,
                    reached_ft_exact: true,
                    ft_started_incomplete: Some(false),
                    deepest_ft_size_reached: Some(4),
                    hero_ft_entry_stack_chips: Some(1_800),
                    hero_ft_entry_stack_bb: Some(18.0),
                },
                TournamentFtHelperFact {
                    tournament_id: tournament_2,
                    reached_ft_exact: false,
                    ft_started_incomplete: None,
                    deepest_ft_size_reached: None,
                    hero_ft_entry_stack_chips: None,
                    hero_ft_entry_stack_bb: None,
                },
                TournamentFtHelperFact {
                    tournament_id: tournament_4,
                    reached_ft_exact: true,
                    ft_started_incomplete: Some(true),
                    deepest_ft_size_reached: Some(5),
                    hero_ft_entry_stack_chips: Some(2_700),
                    hero_ft_entry_stack_bb: Some(27.0),
                },
            ],
            ko_event_facts: vec![],
            deep_ft_entry_facts: vec![
                DeepFtEntryFact {
                    tournament_id: tournament_1,
                    hero_stack_chips: Some(900),
                    hero_stack_bb: Some(9.0),
                },
                DeepFtEntryFact {
                    tournament_id: tournament_4,
                    hero_stack_chips: Some(1_500),
                    hero_stack_bb: Some(15.0),
                },
            ],
            stage_event_facts: vec![],
            stage_attempt_facts: vec![],
            stage_entry_facts: vec![],
            ko_money_event_facts: vec![],
            mystery_envelope_facts: vec![],
            pre_ft_chip_facts: vec![],
        });

        assert_eq!(canonical.coverage.summary_tournament_count, 3);
        assert_eq!(canonical.coverage.hand_tournament_count, 3);
        assert_eq!(
            canonical.values["avg_finish_place_ft"],
            CanonicalStatPoint::from_optional_float(Some(1.0))
        );
        assert_eq!(
            canonical.values["avg_finish_place_no_ft"],
            CanonicalStatPoint::from_optional_float(Some(14.0))
        );
        assert_eq!(
            canonical.values["avg_ft_initial_stack_chips"],
            CanonicalStatPoint::from_optional_float(Some(2_250.0))
        );
        assert_eq!(
            canonical.values["avg_ft_initial_stack_bb"],
            CanonicalStatPoint::from_optional_float(Some(22.5))
        );
        assert_eq!(
            canonical.values["incomplete_ft_percent"],
            CanonicalStatPoint::from_optional_float(Some(50.0))
        );
        assert_eq!(
            canonical.values["itm_percent"],
            CanonicalStatPoint::from_optional_float(Some(66.66666666666666))
        );
        assert_eq!(
            canonical.values["roi_on_ft_pct"],
            CanonicalStatPoint::from_optional_float(Some(300.0))
        );
        assert_eq!(
            canonical.values["winnings_from_itm"],
            CanonicalStatPoint::from_optional_float(Some(60.0))
        );
        assert_eq!(
            canonical.values["winnings_from_ko_total"],
            CanonicalStatPoint::from_optional_float(Some(60.0))
        );
        assert_eq!(
            canonical.values["ko_contribution_percent"],
            CanonicalStatPoint::from_optional_float(Some(50.0))
        );
        assert_eq!(
            canonical.values["deep_ft_reach_percent"],
            CanonicalStatPoint::from_optional_float(Some(66.66666666666666))
        );
        assert_eq!(
            canonical.values["deep_ft_avg_stack_chips"],
            CanonicalStatPoint::from_optional_float(Some(1_200.0))
        );
        assert_eq!(
            canonical.values["deep_ft_avg_stack_bb"],
            CanonicalStatPoint::from_optional_float(Some(12.0))
        );
        assert_eq!(
            canonical.values["deep_ft_roi_pct"],
            CanonicalStatPoint::from_optional_float(Some(300.0))
        );
    }

    #[test]
    fn canonical_snapshot_combines_phase_b_stage_event_metrics() {
        let tournament_1 = Uuid::from_u128(11);
        let tournament_2 = Uuid::from_u128(12);
        let tournament_3 = Uuid::from_u128(13);

        let canonical = build_canonical_stat_snapshot(&CanonicalQueryInputs {
            tournament_buyin_facts: vec![],
            summary_facts: vec![],
            hand_covered_tournaments: vec![tournament_1, tournament_2, tournament_3],
            ft_helper_facts: vec![
                TournamentFtHelperFact {
                    tournament_id: tournament_1,
                    reached_ft_exact: true,
                    ft_started_incomplete: Some(false),
                    deepest_ft_size_reached: Some(3),
                    hero_ft_entry_stack_chips: Some(1_500),
                    hero_ft_entry_stack_bb: Some(15.0),
                },
                TournamentFtHelperFact {
                    tournament_id: tournament_2,
                    reached_ft_exact: true,
                    ft_started_incomplete: Some(true),
                    deepest_ft_size_reached: Some(6),
                    hero_ft_entry_stack_chips: Some(1_200),
                    hero_ft_entry_stack_bb: Some(12.0),
                },
                TournamentFtHelperFact {
                    tournament_id: tournament_3,
                    reached_ft_exact: false,
                    ft_started_incomplete: None,
                    deepest_ft_size_reached: None,
                    hero_ft_entry_stack_chips: None,
                    hero_ft_entry_stack_bb: None,
                },
            ],
            ko_event_facts: vec![],
            deep_ft_entry_facts: vec![],
            stage_event_facts: vec![
                TournamentStageEventFact {
                    tournament_id: tournament_1,
                    early_ft_bust_count: 1,
                    ko_stage_2_3_event_count: 1,
                    ko_stage_3_4_event_count: 1,
                    ko_stage_4_5_event_count: 0,
                    ko_stage_5_6_event_count: 1,
                    ko_stage_6_9_event_count: 2,
                    ko_stage_7_9_event_count: 1,
                    pre_ft_ko_count: 1,
                },
                TournamentStageEventFact {
                    tournament_id: tournament_2,
                    early_ft_bust_count: 0,
                    ko_stage_2_3_event_count: 0,
                    ko_stage_3_4_event_count: 2,
                    ko_stage_4_5_event_count: 1,
                    ko_stage_5_6_event_count: 0,
                    ko_stage_6_9_event_count: 1,
                    ko_stage_7_9_event_count: 1,
                    pre_ft_ko_count: 0,
                },
            ],
            stage_attempt_facts: vec![],
            stage_entry_facts: vec![],
            ko_money_event_facts: vec![],
            mystery_envelope_facts: vec![],
            pre_ft_chip_facts: vec![],
        });

        assert_eq!(
            canonical.values["early_ft_bust_count"],
            CanonicalStatPoint::from_integer(1)
        );
        assert_eq!(
            canonical.values["early_ft_bust_per_tournament"],
            CanonicalStatPoint::from_optional_float(Some(0.5))
        );
        assert_eq!(
            canonical.values["ko_stage_2_3_event_count"],
            CanonicalStatPoint::from_integer(1)
        );
        assert_eq!(
            canonical.values["ko_stage_3_4_event_count"],
            CanonicalStatPoint::from_integer(3)
        );
        assert_eq!(
            canonical.values["ko_stage_4_5_event_count"],
            CanonicalStatPoint::from_integer(1)
        );
        assert_eq!(
            canonical.values["ko_stage_5_6_event_count"],
            CanonicalStatPoint::from_integer(1)
        );
        assert_eq!(
            canonical.values["ko_stage_6_9_event_count"],
            CanonicalStatPoint::from_integer(3)
        );
        assert_eq!(
            canonical.values["ko_stage_7_9_event_count"],
            CanonicalStatPoint::from_integer(2)
        );
        assert_eq!(
            canonical.values["pre_ft_ko_count"],
            CanonicalStatPoint::from_integer(1)
        );
    }

    #[test]
    fn canonical_snapshot_combines_phase_b_conversion_metrics() {
        let tournament_1 = Uuid::from_u128(21);
        let tournament_2 = Uuid::from_u128(22);
        let tournament_3 = Uuid::from_u128(23);

        let canonical = build_canonical_stat_snapshot(&CanonicalQueryInputs {
            tournament_buyin_facts: vec![],
            summary_facts: vec![],
            hand_covered_tournaments: vec![tournament_1, tournament_2, tournament_3],
            ft_helper_facts: vec![
                TournamentFtHelperFact {
                    tournament_id: tournament_1,
                    reached_ft_exact: true,
                    ft_started_incomplete: Some(false),
                    deepest_ft_size_reached: Some(3),
                    hero_ft_entry_stack_chips: Some(2_000),
                    hero_ft_entry_stack_bb: Some(20.0),
                },
                TournamentFtHelperFact {
                    tournament_id: tournament_2,
                    reached_ft_exact: true,
                    ft_started_incomplete: Some(false),
                    deepest_ft_size_reached: Some(4),
                    hero_ft_entry_stack_chips: Some(1_000),
                    hero_ft_entry_stack_bb: Some(10.0),
                },
                TournamentFtHelperFact {
                    tournament_id: tournament_3,
                    reached_ft_exact: false,
                    ft_started_incomplete: None,
                    deepest_ft_size_reached: None,
                    hero_ft_entry_stack_chips: None,
                    hero_ft_entry_stack_bb: None,
                },
            ],
            ko_event_facts: vec![],
            deep_ft_entry_facts: vec![],
            stage_event_facts: vec![
                TournamentStageEventFact {
                    tournament_id: tournament_1,
                    early_ft_bust_count: 0,
                    ko_stage_2_3_event_count: 0,
                    ko_stage_3_4_event_count: 1,
                    ko_stage_4_5_event_count: 0,
                    ko_stage_5_6_event_count: 1,
                    ko_stage_6_9_event_count: 2,
                    ko_stage_7_9_event_count: 1,
                    pre_ft_ko_count: 0,
                },
                TournamentStageEventFact {
                    tournament_id: tournament_2,
                    early_ft_bust_count: 0,
                    ko_stage_2_3_event_count: 0,
                    ko_stage_3_4_event_count: 2,
                    ko_stage_4_5_event_count: 0,
                    ko_stage_5_6_event_count: 0,
                    ko_stage_6_9_event_count: 1,
                    ko_stage_7_9_event_count: 1,
                    pre_ft_ko_count: 0,
                },
            ],
            stage_attempt_facts: vec![
                TournamentStageAttemptFact {
                    tournament_id: tournament_1,
                    ko_stage_2_3_attempt_count: 0,
                    ko_stage_3_4_attempt_count: 1,
                    ko_stage_4_5_attempt_count: 0,
                    ko_stage_5_6_attempt_count: 2,
                    ko_stage_6_9_attempt_count: 4,
                    ko_stage_7_9_attempt_count: 3,
                },
                TournamentStageAttemptFact {
                    tournament_id: tournament_2,
                    ko_stage_2_3_attempt_count: 0,
                    ko_stage_3_4_attempt_count: 3,
                    ko_stage_4_5_attempt_count: 0,
                    ko_stage_5_6_attempt_count: 1,
                    ko_stage_6_9_attempt_count: 2,
                    ko_stage_7_9_attempt_count: 1,
                },
            ],
            stage_entry_facts: vec![
                TournamentStageEntryFact {
                    tournament_id: tournament_1,
                    reached_stage_2_3: false,
                    reached_stage_3_4: true,
                    reached_stage_4_5: true,
                    reached_stage_5_6: true,
                    reached_stage_7_9: true,
                    hero_stage_5_6_stack_bb: Some(10.0),
                    hero_stage_3_4_stack_bb: Some(5.0),
                },
                TournamentStageEntryFact {
                    tournament_id: tournament_2,
                    reached_stage_2_3: false,
                    reached_stage_3_4: true,
                    reached_stage_4_5: false,
                    reached_stage_5_6: true,
                    reached_stage_7_9: true,
                    hero_stage_5_6_stack_bb: Some(8.0),
                    hero_stage_3_4_stack_bb: Some(4.0),
                },
            ],
            ko_money_event_facts: vec![],
            mystery_envelope_facts: vec![],
            pre_ft_chip_facts: vec![],
        });

        assert_eq!(
            canonical.values["ft_stack_conversion"],
            CanonicalStatPoint::from_optional_float(Some(0.1))
        );
        assert_eq!(
            canonical.values["avg_ko_attempts_per_ft"],
            CanonicalStatPoint::from_optional_float(Some(3.0))
        );
        assert_eq!(
            canonical.values["ko_attempts_success_rate"],
            CanonicalStatPoint::from_optional_float(Some(0.5))
        );
        assert_eq!(
            canonical.values["ft_stack_conversion_7_9"],
            CanonicalStatPoint::from_optional_float(Some(0.1))
        );
        assert_eq!(
            canonical.values["ft_stack_conversion_7_9_attempts"],
            CanonicalStatPoint::from_optional_float(Some(3.0))
        );
        assert_eq!(
            canonical.values["ft_stack_conversion_5_6"],
            CanonicalStatPoint::from_optional_float(Some(1.0 / 18.0))
        );
        assert_eq!(
            canonical.values["ft_stack_conversion_5_6_attempts"],
            CanonicalStatPoint::from_optional_float(Some(1.5))
        );
        assert_eq!(
            canonical.values["ft_stack_conversion_3_4"],
            CanonicalStatPoint::from_optional_float(Some(3.0 / 9.0))
        );
        assert_eq!(
            canonical.values["ft_stack_conversion_3_4_attempts"],
            CanonicalStatPoint::from_optional_float(Some(2.0))
        );
        assert_eq!(
            canonical.values["ko_stage_2_3_attempts_per_tournament"],
            CanonicalStatPoint::from_optional_float(None)
        );
        assert_eq!(
            canonical.values["ko_stage_3_4_attempts_per_tournament"],
            CanonicalStatPoint::from_optional_float(Some(2.0))
        );
        assert_eq!(
            canonical.values["ko_stage_4_5_attempts_per_tournament"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["ko_stage_5_6_attempts_per_tournament"],
            CanonicalStatPoint::from_optional_float(Some(1.5))
        );
        assert_eq!(
            canonical.values["ko_stage_7_9_attempts_per_tournament"],
            CanonicalStatPoint::from_optional_float(Some(2.0))
        );
    }

    #[test]
    fn canonical_snapshot_surfaces_remaining_phase_c_and_d_keys() {
        let tournament_1 = Uuid::from_u128(31);

        let canonical = build_canonical_stat_snapshot(&CanonicalQueryInputs {
            tournament_buyin_facts: vec![TournamentBuyinFact {
                tournament_id: tournament_1,
                buyin_total_cents: 1_000,
            }],
            summary_facts: vec![SummaryTournamentFact {
                tournament_id: tournament_1,
                buyin_total_cents: 1_000,
                payout_cents: 0,
                regular_prize_cents: 0,
                finish_place: Some(9),
            }],
            hand_covered_tournaments: vec![tournament_1],
            ft_helper_facts: vec![],
            ko_event_facts: vec![],
            deep_ft_entry_facts: vec![],
            stage_event_facts: vec![],
            stage_attempt_facts: vec![],
            stage_entry_facts: vec![],
            ko_money_event_facts: vec![],
            mystery_envelope_facts: vec![],
            pre_ft_chip_facts: vec![],
        });

        assert_eq!(
            canonical.values["ko_stage_2_3_money_total"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["ko_stage_3_4_money_total"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["ko_stage_4_5_money_total"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["ko_stage_5_6_money_total"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["ko_stage_6_9_money_total"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["ko_stage_7_9_money_total"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["ko_contribution_adjusted_percent"],
            CanonicalStatPoint::from_optional_float(None)
        );
        assert_eq!(
            canonical.values["ko_luck_money_delta"],
            CanonicalStatPoint::from_optional_float(None)
        );
        assert_eq!(
            canonical.values["roi_adj_pct"],
            CanonicalStatPoint::from_optional_float(None)
        );
        assert_eq!(
            canonical.values["pre_ft_chipev"],
            CanonicalStatPoint::from_optional_float(None)
        );
        assert_eq!(
            canonical.values["big_ko_x1_5_count"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["big_ko_x2_count"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["big_ko_x10_count"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["big_ko_x100_count"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["big_ko_x1000_count"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
        assert_eq!(
            canonical.values["big_ko_x10000_count"],
            CanonicalStatPoint::from_optional_float(Some(0.0))
        );
    }

    #[test]
    fn canonical_snapshot_combines_phase_c_estimated_money_metrics() {
        let tournament_1 = Uuid::from_u128(41);

        let canonical = build_canonical_stat_snapshot(&CanonicalQueryInputs {
            tournament_buyin_facts: vec![TournamentBuyinFact {
                tournament_id: tournament_1,
                buyin_total_cents: 1_000,
            }],
            summary_facts: vec![SummaryTournamentFact {
                tournament_id: tournament_1,
                buyin_total_cents: 1_000,
                payout_cents: 8_000,
                regular_prize_cents: 1_000,
                finish_place: Some(1),
            }],
            hand_covered_tournaments: vec![tournament_1],
            ft_helper_facts: vec![],
            ko_event_facts: vec![],
            deep_ft_entry_facts: vec![],
            stage_event_facts: vec![],
            stage_attempt_facts: vec![],
            stage_entry_facts: vec![],
            ko_money_event_facts: vec![
                TournamentKoMoneyEventFact {
                    tournament_id: tournament_1,
                    share_micros: 1_000_000,
                    is_stage_2_3: false,
                    is_stage_3_4: false,
                    is_stage_4_5: false,
                    is_stage_5_6: false,
                    is_stage_6_9: true,
                    is_stage_7_9: true,
                },
                TournamentKoMoneyEventFact {
                    tournament_id: tournament_1,
                    share_micros: 500_000,
                    is_stage_2_3: false,
                    is_stage_3_4: true,
                    is_stage_4_5: false,
                    is_stage_5_6: false,
                    is_stage_6_9: false,
                    is_stage_7_9: false,
                },
            ],
            mystery_envelope_facts: vec![
                MysteryEnvelopeFact {
                    buyin_total_cents: 1_000,
                    sort_order: 4,
                    payout_cents: 10_000,
                    frequency_per_100m: 1,
                },
                MysteryEnvelopeFact {
                    buyin_total_cents: 1_000,
                    sort_order: 5,
                    payout_cents: 2_000,
                    frequency_per_100m: 3,
                },
            ],
            pre_ft_chip_facts: vec![TournamentPreFtChipFact {
                tournament_id: tournament_1,
                chip_delta: 1_500,
            }],
        });

        assert_eq!(
            canonical.values["ko_stage_3_4_money_total"],
            CanonicalStatPoint::from_optional_float(Some(20.0))
        );
        assert_eq!(
            canonical.values["ko_stage_6_9_money_total"],
            CanonicalStatPoint::from_optional_float(Some(40.0))
        );
        assert_eq!(
            canonical.values["ko_stage_7_9_money_total"],
            CanonicalStatPoint::from_optional_float(Some(40.0))
        );
        assert_eq!(
            canonical.values["ko_contribution_adjusted_percent"],
            CanonicalStatPoint::from_optional_float(Some(75.0))
        );
        assert_eq!(
            canonical.values["ko_luck_money_delta"],
            CanonicalStatPoint::from_optional_float(Some(10.0))
        );
        assert_eq!(
            canonical.values["roi_adj_pct"],
            CanonicalStatPoint::from_optional_float(Some(600.0))
        );
        assert_eq!(
            canonical.values["pre_ft_chipev"],
            CanonicalStatPoint::from_optional_float(Some(1_500.0))
        );
        assert_eq!(
            canonical.values["big_ko_x10_count"],
            CanonicalStatPoint::from_optional_float(Some(0.5))
        );
        assert_eq!(
            canonical.values["big_ko_x2_count"],
            CanonicalStatPoint::from_optional_float(Some(1.5))
        );
    }
}
