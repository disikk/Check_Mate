use std::collections::BTreeSet;

use anyhow::Result;
use postgres::GenericClient;
use uuid::Uuid;

use crate::{
    models::{SeedStatCoverage, SeedStatSnapshot, SeedStatsFilters},
    registry::FEATURE_VERSION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SummaryTournamentFact {
    tournament_id: Uuid,
    buyin_total_cents: i64,
    payout_cents: i64,
    finish_place: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct HandTournamentFeatureFact {
    tournament_id: Uuid,
    played_ft_hand: bool,
    hero_exact_ko_count: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SeedStatAccumulator {
    pub summary_tournament_count: u64,
    pub hand_tournament_count: u64,
    pub total_buyin_cents: i64,
    pub total_payout_cents: i64,
    pub finish_place_sum: i64,
    pub tournaments_with_ft_reach: u64,
    pub total_ko: u64,
}

pub fn query_seed_stats(
    client: &mut impl GenericClient,
    filters: SeedStatsFilters,
) -> Result<SeedStatSnapshot> {
    let buyin_filter = filters
        .buyin_total_cents
        .as_ref()
        .map(|values| values.iter().copied().collect::<BTreeSet<_>>());
    let summary_facts = load_summary_tournament_facts(
        client,
        filters.organization_id,
        filters.player_profile_id,
    )?;
    let filtered_summary_facts = summary_facts
        .iter()
        .copied()
        .filter(|fact| match &buyin_filter {
            Some(set) => set.contains(&fact.buyin_total_cents),
            None => true,
        })
        .collect::<Vec<_>>();
    let allowed_tournaments = filtered_summary_facts
        .iter()
        .map(|fact| fact.tournament_id)
        .collect::<BTreeSet<_>>();
    let hand_facts = load_hand_tournament_feature_facts(
        client,
        filters.organization_id,
        filters.player_profile_id,
    )?;
    let filtered_hand_facts = hand_facts
        .iter()
        .copied()
        .filter(|fact| allowed_tournaments.contains(&fact.tournament_id))
        .collect::<Vec<_>>();

    let mut tournaments_with_ft_reach = BTreeSet::new();
    let mut hand_tournaments = BTreeSet::new();
    let mut total_ko = 0_u64;

    for fact in &filtered_hand_facts {
        hand_tournaments.insert(fact.tournament_id);
        if fact.played_ft_hand {
            tournaments_with_ft_reach.insert(fact.tournament_id);
        }
        total_ko += fact.hero_exact_ko_count as u64;
    }

    let accumulator = SeedStatAccumulator {
        summary_tournament_count: filtered_summary_facts.len() as u64,
        hand_tournament_count: hand_tournaments.len() as u64,
        total_buyin_cents: filtered_summary_facts
            .iter()
            .map(|fact| fact.buyin_total_cents)
            .sum(),
        total_payout_cents: filtered_summary_facts
            .iter()
            .map(|fact| fact.payout_cents)
            .sum(),
        finish_place_sum: filtered_summary_facts
            .iter()
            .map(|fact| i64::from(fact.finish_place))
            .sum(),
        tournaments_with_ft_reach: tournaments_with_ft_reach.len() as u64,
        total_ko,
    };

    Ok(build_seed_stat_snapshot(accumulator))
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
            finish_place: row.get(3),
        })
        .collect())
}

fn load_hand_tournament_feature_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<HandTournamentFeatureFact>> {
    let rows = client.query(
        "SELECT
            h.tournament_id,
            COALESCE(ft.value, FALSE),
            COALESCE(ko.value::double precision, 0.0::double precision)
         FROM core.hands h
         LEFT JOIN analytics.player_hand_bool_features ft
           ON ft.organization_id = h.organization_id
          AND ft.player_profile_id = h.player_profile_id
          AND ft.hand_id = h.id
          AND ft.feature_key = 'played_ft_hand'
          AND ft.feature_version = $3
         LEFT JOIN analytics.player_hand_num_features ko
           ON ko.organization_id = h.organization_id
          AND ko.player_profile_id = h.player_profile_id
          AND ko.hand_id = h.id
          AND ko.feature_key = 'hero_exact_ko_count'
          AND ko.feature_version = $3
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2",
        &[&organization_id, &player_profile_id, &FEATURE_VERSION],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| HandTournamentFeatureFact {
            tournament_id: row.get(0),
            played_ft_hand: row.get(1),
            hero_exact_ko_count: row.get(2),
        })
        .collect())
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
    let avg_finish_place = if accumulator.summary_tournament_count == 0 {
        None
    } else {
        Some(accumulator.finish_place_sum as f64 / accumulator.summary_tournament_count as f64)
    };
    let final_table_reach_percent = if accumulator.hand_tournament_count == 0 {
        None
    } else {
        Some(
            accumulator.tournaments_with_ft_reach as f64
                / accumulator.hand_tournament_count as f64
                * 100.0,
        )
    };
    let avg_ko_per_tournament = if accumulator.hand_tournament_count == 0 {
        None
    } else {
        Some(accumulator.total_ko as f64 / accumulator.hand_tournament_count as f64)
    };

    SeedStatSnapshot {
        coverage,
        roi_pct,
        avg_finish_place,
        final_table_reach_percent,
        total_ko: accumulator.total_ko,
        avg_ko_per_tournament,
    }
}

#[cfg(test)]
mod tests {
    use super::{SeedStatAccumulator, build_seed_stat_snapshot};

    #[test]
    fn applies_formulae_and_zero_denominator_rules() {
        let snapshot = build_seed_stat_snapshot(SeedStatAccumulator {
            summary_tournament_count: 0,
            hand_tournament_count: 0,
            total_buyin_cents: 0,
            total_payout_cents: 0,
            finish_place_sum: 0,
            tournaments_with_ft_reach: 0,
            total_ko: 0,
        });

        assert_eq!(snapshot.roi_pct, None);
        assert_eq!(snapshot.avg_finish_place, None);
        assert_eq!(snapshot.final_table_reach_percent, None);
        assert_eq!(snapshot.total_ko, 0);
        assert_eq!(snapshot.avg_ko_per_tournament, None);

        let populated = build_seed_stat_snapshot(SeedStatAccumulator {
            summary_tournament_count: 4,
            hand_tournament_count: 2,
            total_buyin_cents: 10_000,
            total_payout_cents: 13_000,
            finish_place_sum: 14,
            tournaments_with_ft_reach: 1,
            total_ko: 3,
        });

        assert_eq!(populated.roi_pct, Some(30.0));
        assert_eq!(populated.avg_finish_place, Some(3.5));
        assert_eq!(populated.final_table_reach_percent, Some(50.0));
        assert_eq!(populated.total_ko, 3);
        assert_eq!(populated.avg_ko_per_tournament, Some(1.5));
    }
}
