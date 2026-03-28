use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use postgres::GenericClient;
use serde::Serialize;
use uuid::Uuid;

use crate::models::{CanonicalStatNumericValue, CanonicalStatPoint, CanonicalStatSnapshot};
use crate::queries::{
    CanonicalQueryInputs, CanonicalQueryScope, TournamentKoEventFact, TournamentStageAttemptFact,
    TournamentStageEntryFact, TournamentStageEventFact, build_canonical_stat_snapshot,
    load_canonical_query_inputs_for_scope,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FtDashboardFilters {
    pub organization_id: Uuid,
    pub player_profile_id: Uuid,
    pub buyin_total_cents: Option<Vec<i64>>,
    pub bundle_id: Option<Uuid>,
    pub date_from_local: Option<String>,
    pub date_to_local: Option<String>,
    pub timezone_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FtDashboardDataState {
    Ready,
    Empty,
    Partial,
    Blocked,
}

impl FtDashboardDataState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Empty => "empty",
            Self::Partial => "partial",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FtValueState {
    Ready,
    Empty,
    Blocked,
}

impl FtValueState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Empty => "empty",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FtDashboardBundleOption {
    pub bundle_id: Uuid,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FtDashboardFilterOptions {
    pub buyin_total_cents: Vec<i64>,
    pub bundle_options: Vec<FtDashboardBundleOption>,
    pub min_date_local: Option<String>,
    pub max_date_local: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FtDashboardSelectedFilters {
    pub buyin_total_cents: Option<Vec<i64>>,
    pub bundle_id: Option<Uuid>,
    pub date_from_local: Option<String>,
    pub date_to_local: Option<String>,
    pub timezone_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FtDashboardCoverage {
    pub tournament_count: u64,
    pub summary_tournament_count: u64,
    pub hand_tournament_count: u64,
    pub bundle_count: u64,
    pub min_started_at_local: Option<String>,
    pub max_started_at_local: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FtDashboardMetricCard {
    pub state: FtValueState,
    pub value: Option<f64>,
    pub aux_value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FtDashboardInlineStat {
    pub state: FtValueState,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FtDashboardBigKoCard {
    pub state: FtValueState,
    pub tier: String,
    pub count: Option<f64>,
    pub occurs_once_every_kos: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FtChartBar {
    pub label: String,
    pub value: f64,
    pub sample_size: u64,
    pub attempts: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FtChartVariant {
    pub bars: Vec<FtChartBar>,
    pub median_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FtDashboardChart {
    pub state: FtValueState,
    pub metric: String,
    pub density_options: Vec<i32>,
    pub default_density_step: Option<i32>,
    pub variants: BTreeMap<String, FtChartVariant>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FtDashboardSnapshot {
    pub filter_options: FtDashboardFilterOptions,
    pub selected_filters: FtDashboardSelectedFilters,
    pub stat_cards: BTreeMap<String, FtDashboardMetricCard>,
    pub big_ko_cards: Vec<FtDashboardBigKoCard>,
    pub inline_stats: BTreeMap<String, FtDashboardInlineStat>,
    pub charts: BTreeMap<String, FtDashboardChart>,
    pub coverage: FtDashboardCoverage,
    pub data_state: FtDashboardDataState,
}

#[derive(Debug, Clone, PartialEq)]
struct StackTournamentRecord {
    tournament_id: Uuid,
    finish_place: Option<i32>,
    buyin_total_cents: i64,
    payout_cents: Option<i64>,
    ft_entry_stack_chips: i64,
    ft_entry_stack_bb: Option<f64>,
    total_exact_ko_value: f64,
    early_ft_exact_ko_value: f64,
    ko_stage_5_6_value: f64,
    ko_stage_6_9_attempt_count: u64,
    ko_stage_5_6_attempt_count: u64,
    stage_5_6_stack_bb: Option<f64>,
}

pub fn query_ft_dashboard(
    client: &mut impl GenericClient,
    filters: FtDashboardFilters,
) -> Result<FtDashboardSnapshot> {
    let all_tournament_ids =
        load_player_tournament_ids(client, filters.organization_id, filters.player_profile_id)?;
    let scoped_tournament_ids =
        resolve_scoped_tournament_ids(client, &filters, &all_tournament_ids)?;
    let bundle_options =
        load_bundle_options(client, filters.organization_id, filters.player_profile_id)?;
    let buyin_options =
        load_buyin_options(client, filters.organization_id, filters.player_profile_id)?;
    let (min_date_local, max_date_local) =
        load_date_range_bounds(client, filters.organization_id, filters.player_profile_id)?;

    let inputs = load_canonical_query_inputs_for_scope(
        client,
        CanonicalQueryScope {
            organization_id: filters.organization_id,
            player_profile_id: filters.player_profile_id,
            buyin_total_cents: filters.buyin_total_cents.clone(),
            allowed_tournament_ids: Some(scoped_tournament_ids.clone()),
        },
    )?;
    let canonical = build_canonical_stat_snapshot(&inputs);

    let stat_cards = build_stat_cards(&canonical);
    let inline_stats = build_inline_stats(&canonical);
    let big_ko_cards = build_big_ko_cards(&canonical);
    let charts = build_charts(&inputs);
    let coverage = build_coverage(
        &inputs,
        filters.bundle_id,
        min_date_local.clone(),
        max_date_local.clone(),
    );
    let data_state = resolve_dashboard_state(&coverage, &stat_cards, &charts);

    Ok(FtDashboardSnapshot {
        filter_options: FtDashboardFilterOptions {
            buyin_total_cents: buyin_options,
            bundle_options,
            min_date_local,
            max_date_local,
        },
        selected_filters: FtDashboardSelectedFilters {
            buyin_total_cents: filters.buyin_total_cents,
            bundle_id: filters.bundle_id,
            date_from_local: filters.date_from_local,
            date_to_local: filters.date_to_local,
            timezone_name: filters.timezone_name,
        },
        stat_cards,
        big_ko_cards,
        inline_stats,
        charts,
        coverage,
        data_state,
    })
}

fn load_player_tournament_ids(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<BTreeSet<Uuid>> {
    let rows = client.query(
        "SELECT id
         FROM core.tournaments
         WHERE organization_id = $1
           AND player_profile_id = $2",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

fn resolve_scoped_tournament_ids(
    client: &mut impl GenericClient,
    filters: &FtDashboardFilters,
    all_tournament_ids: &BTreeSet<Uuid>,
) -> Result<BTreeSet<Uuid>> {
    let mut allowed = all_tournament_ids.clone();

    if let Some(bundle_id) = filters.bundle_id {
        let bundle_ids = load_bundle_tournament_ids(
            client,
            filters.organization_id,
            filters.player_profile_id,
            bundle_id,
        )?;
        allowed = allowed
            .intersection(&bundle_ids)
            .copied()
            .collect::<BTreeSet<_>>();
    }

    if filters.date_from_local.is_some() || filters.date_to_local.is_some() {
        let date_ids = load_date_filtered_tournament_ids(
            client,
            filters.organization_id,
            filters.player_profile_id,
            filters.date_from_local.as_deref(),
            filters.date_to_local.as_deref(),
        )?;
        allowed = allowed
            .intersection(&date_ids)
            .copied()
            .collect::<BTreeSet<_>>();
    }

    Ok(allowed)
}

fn load_bundle_tournament_ids(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    bundle_id: Uuid,
) -> Result<BTreeSet<Uuid>> {
    let rows = client.query(
        "WITH bundle_source_files AS (
             SELECT DISTINCT members.source_file_id
             FROM import.ingest_bundle_files bundle_files
             INNER JOIN import.source_file_members members
               ON members.id = bundle_files.source_file_member_id
             WHERE bundle_files.bundle_id = $3
         )
         SELECT DISTINCT tournament_id
         FROM (
             SELECT tournaments.id AS tournament_id
             FROM core.tournaments tournaments
             INNER JOIN bundle_source_files bundle_files
               ON bundle_files.source_file_id = tournaments.source_summary_file_id
             WHERE tournaments.organization_id = $1
               AND tournaments.player_profile_id = $2
             UNION
             SELECT hands.tournament_id
             FROM core.hands hands
             INNER JOIN bundle_source_files bundle_files
               ON bundle_files.source_file_id = hands.source_file_id
             INNER JOIN core.tournaments tournaments
               ON tournaments.id = hands.tournament_id
             WHERE tournaments.organization_id = $1
               AND tournaments.player_profile_id = $2
         ) AS scoped",
        &[&organization_id, &player_profile_id, &bundle_id],
    )?;

    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

fn load_date_filtered_tournament_ids(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    date_from_local: Option<&str>,
    date_to_local: Option<&str>,
) -> Result<BTreeSet<Uuid>> {
    let rows = match (date_from_local, date_to_local) {
        (Some(from), Some(to)) => client.query(
            "SELECT id
             FROM core.tournaments
             WHERE organization_id = $1
               AND player_profile_id = $2
               AND started_at_local IS NOT NULL
               AND started_at_local >= replace($3, 'T', ' ')::timestamp
               AND started_at_local <= replace($4, 'T', ' ')::timestamp",
            &[&organization_id, &player_profile_id, &from, &to],
        )?,
        (Some(from), None) => client.query(
            "SELECT id
             FROM core.tournaments
             WHERE organization_id = $1
               AND player_profile_id = $2
               AND started_at_local IS NOT NULL
               AND started_at_local >= replace($3, 'T', ' ')::timestamp",
            &[&organization_id, &player_profile_id, &from],
        )?,
        (None, Some(to)) => client.query(
            "SELECT id
             FROM core.tournaments
             WHERE organization_id = $1
               AND player_profile_id = $2
               AND started_at_local IS NOT NULL
               AND started_at_local <= replace($3, 'T', ' ')::timestamp",
            &[&organization_id, &player_profile_id, &to],
        )?,
        (None, None) => vec![],
    };

    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

fn load_buyin_options(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<i64>> {
    let rows = client.query(
        "SELECT DISTINCT ROUND(buyin_total * 100)::bigint AS buyin_total_cents
         FROM core.tournaments
         WHERE organization_id = $1
           AND player_profile_id = $2
         ORDER BY buyin_total_cents",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

fn load_bundle_options(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<FtDashboardBundleOption>> {
    let rows = client.query(
        "SELECT bundles.id,
                to_char(bundles.created_at, 'YYYY-MM-DD HH24:MI') AS created_label,
                COUNT(bundle_files.id)::bigint AS file_count
         FROM import.ingest_bundles bundles
         LEFT JOIN import.ingest_bundle_files bundle_files
           ON bundle_files.bundle_id = bundles.id
         WHERE bundles.organization_id = $1
           AND bundles.player_profile_id = $2
           AND bundles.status IN ('succeeded', 'partial_success')
         GROUP BY bundles.id, bundles.created_at
         ORDER BY bundles.created_at DESC, bundles.id DESC",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows
        .into_iter()
        .map(|row| FtDashboardBundleOption {
            bundle_id: row.get(0),
            label: format!(
                "Bundle {} · {} files",
                row.get::<_, String>(1),
                row.get::<_, i64>(2)
            ),
        })
        .collect())
}

fn load_date_range_bounds(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<(Option<String>, Option<String>)> {
    let row = client.query_one(
        "SELECT
             to_char(min(started_at_local), 'YYYY-MM-DD\"T\"HH24:MI'),
             to_char(max(started_at_local), 'YYYY-MM-DD\"T\"HH24:MI')
         FROM core.tournaments
         WHERE organization_id = $1
           AND player_profile_id = $2
           AND started_at_local IS NOT NULL",
        &[&organization_id, &player_profile_id],
    )?;

    Ok((row.get(0), row.get(1)))
}

fn build_stat_cards(canonical: &CanonicalStatSnapshot) -> BTreeMap<String, FtDashboardMetricCard> {
    let mut cards = BTreeMap::new();
    cards.insert("roi".to_string(), single_stat_card(canonical, "roi_pct"));
    cards.insert(
        "ftReach".to_string(),
        single_stat_card(canonical, "final_table_reach_percent"),
    );
    cards.insert(
        "itm".to_string(),
        single_stat_card(canonical, "itm_percent"),
    );
    cards.insert(
        "avgKo".to_string(),
        dual_stat_card(
            canonical,
            "avg_ko_per_tournament",
            "early_ft_ko_per_tournament",
        ),
    );
    cards.insert("koAttempts1".to_string(), blocked_card());
    cards.insert(
        "roiOnFt".to_string(),
        single_stat_card(canonical, "roi_on_ft_pct"),
    );
    cards.insert(
        "avgFtStack".to_string(),
        dual_stat_card(
            canonical,
            "avg_ft_initial_stack_chips",
            "avg_ft_initial_stack_bb",
        ),
    );
    cards.insert(
        "deepFtReach".to_string(),
        single_stat_card(canonical, "deep_ft_reach_percent"),
    );
    cards.insert(
        "ftStackConv79".to_string(),
        dual_stat_card(
            canonical,
            "ft_stack_conversion_7_9",
            "ko_attempts_per_ft_7_9",
        ),
    );
    cards.insert("koAttempts2".to_string(), blocked_card());
    cards.insert(
        "winningsFromKo".to_string(),
        single_stat_card(canonical, "ko_contribution"),
    );
    cards.insert(
        "avgPlaceFt".to_string(),
        single_stat_card(canonical, "avg_finish_place_ft"),
    );
    cards.insert(
        "deepFtRoi".to_string(),
        single_stat_card(canonical, "deep_ft_roi_pct"),
    );
    cards.insert(
        "ftStackConv56".to_string(),
        dual_stat_card(
            canonical,
            "ft_stack_conversion_5_6",
            "ko_attempts_per_ft_5_6",
        ),
    );
    cards.insert("koAttempts3p".to_string(), blocked_card());
    cards.insert(
        "winningsFromItm".to_string(),
        complement_percent_card(canonical, "ko_contribution"),
    );
    cards.insert(
        "avgPlaceAll".to_string(),
        single_stat_card(canonical, "avg_finish_place"),
    );
    cards.insert(
        "deepFtStack".to_string(),
        dual_stat_card(canonical, "deep_ft_avg_stack_chips", "deep_ft_avg_stack_bb"),
    );
    cards.insert(
        "ftStackConv34".to_string(),
        dual_stat_card(
            canonical,
            "ft_stack_conversion_3_4",
            "ko_attempts_per_ft_3_4",
        ),
    );

    cards
}

fn build_inline_stats(
    canonical: &CanonicalStatSnapshot,
) -> BTreeMap<String, FtDashboardInlineStat> {
    BTreeMap::from([
        (
            "koLuck".to_string(),
            single_inline_stat(canonical, "ko_luck"),
        ),
        (
            "roiAdj".to_string(),
            single_inline_stat(canonical, "roi_adj"),
        ),
    ])
}

fn build_big_ko_cards(canonical: &CanonicalStatSnapshot) -> Vec<FtDashboardBigKoCard> {
    let total_ko_count = stat_to_f64(canonical.values.get("total_ko")).unwrap_or(0.0);

    [
        ("x1.5", "big_ko_x1_5_count"),
        ("x2", "big_ko_x2_count"),
        ("x10", "big_ko_x10_count"),
        ("x100", "big_ko_x100_count"),
        ("x1000", "big_ko_x1000_count"),
        ("x10000", "big_ko_x10000_count"),
    ]
    .into_iter()
    .map(|(tier, key)| {
        let count = stat_to_f64(canonical.values.get(key));
        let state = match count {
            Some(_) => FtValueState::Ready,
            None => FtValueState::Blocked,
        };

        FtDashboardBigKoCard {
            state,
            tier: tier.to_string(),
            count,
            occurs_once_every_kos: count
                .filter(|count| *count > 0.0 && total_ko_count > 0.0)
                .map(|count| total_ko_count / count),
        }
    })
    .collect()
}

fn build_charts(inputs: &CanonicalQueryInputs) -> BTreeMap<String, FtDashboardChart> {
    let stack_records = build_stack_records(inputs);
    BTreeMap::from([
        ("ft".to_string(), build_finish_place_chart(inputs, 1..=9)),
        (
            "pre_ft".to_string(),
            build_finish_place_chart(inputs, 10..=18),
        ),
        ("all".to_string(), build_finish_place_chart(inputs, 1..=18)),
        (
            "ft_stack".to_string(),
            build_ft_stack_distribution_chart(&stack_records),
        ),
        (
            "ft_stack_roi".to_string(),
            build_ft_stack_metric_chart(&stack_records, "roi"),
        ),
        (
            "ft_stack_roi_0_800".to_string(),
            build_short_roi_chart(&stack_records),
        ),
        (
            "ft_stack_conv".to_string(),
            build_ft_stack_metric_chart(&stack_records, "conv"),
        ),
        (
            "ft_stack_conv_7_9".to_string(),
            build_stage_conversion_chart(&stack_records, "7_9"),
        ),
        (
            "ft_stack_conv_5_6".to_string(),
            build_stage_conversion_chart(&stack_records, "5_6"),
        ),
        (
            "ko_attempts".to_string(),
            build_ko_attempt_distribution_chart(inputs),
        ),
        (
            "avg_ko_by_position".to_string(),
            build_avg_ko_by_position_chart(inputs),
        ),
        (
            "avg_ko_by_ft_stack".to_string(),
            build_ft_stack_metric_chart(&stack_records, "avg_ko"),
        ),
        (
            "avg_ko_by_early_ft_stack".to_string(),
            build_ft_stack_metric_chart(&stack_records, "early_avg_ko"),
        ),
    ])
}

fn build_stack_records(inputs: &CanonicalQueryInputs) -> Vec<StackTournamentRecord> {
    let summaries = inputs
        .summary_facts
        .iter()
        .copied()
        .map(|fact| (fact.tournament_id, fact))
        .collect::<BTreeMap<_, _>>();
    let ko_events = inputs
        .ko_event_facts
        .iter()
        .copied()
        .map(|fact| (fact.tournament_id, fact))
        .collect::<BTreeMap<_, _>>();
    let stage_events = inputs
        .stage_event_facts
        .iter()
        .copied()
        .map(|fact| (fact.tournament_id, fact))
        .collect::<BTreeMap<_, _>>();
    let stage_attempts = inputs
        .stage_attempt_facts
        .iter()
        .copied()
        .map(|fact| (fact.tournament_id, fact))
        .collect::<BTreeMap<_, _>>();
    let stage_entries = inputs
        .stage_entry_facts
        .iter()
        .copied()
        .map(|fact| (fact.tournament_id, fact))
        .collect::<BTreeMap<_, _>>();

    inputs
        .ft_helper_facts
        .iter()
        .filter(|fact| fact.reached_ft_exact)
        .filter_map(|fact| {
            let chips = fact.hero_ft_entry_stack_chips?;
            let summary = summaries.get(&fact.tournament_id).copied();
            let ko = ko_events
                .get(&fact.tournament_id)
                .copied()
                .unwrap_or(TournamentKoEventFact {
                    tournament_id: fact.tournament_id,
                    total_exact_ko_event_count: 0,
                    early_ft_exact_ko_event_count: 0,
                    total_exact_ko_share_total: 0.0,
                    exact_ft_ko_share_total: 0.0,
                    early_ft_exact_ko_share_total: 0.0,
                });
            let stage_event = stage_events.get(&fact.tournament_id).copied().unwrap_or(
                TournamentStageEventFact {
                    tournament_id: fact.tournament_id,
                    early_ft_bust_count: 0,
                    ko_stage_2_3_share_total: 0.0,
                    ko_stage_3_4_share_total: 0.0,
                    ko_stage_4_5_share_total: 0.0,
                    ko_stage_5_6_share_total: 0.0,
                    ko_stage_6_9_share_total: 0.0,
                    ko_stage_7_9_share_total: 0.0,
                    transition_exact_ko_share_total: 0.0,
                },
            );
            let stage_attempt = stage_attempts.get(&fact.tournament_id).copied().unwrap_or(
                TournamentStageAttemptFact {
                    tournament_id: fact.tournament_id,
                    exact_ft_attempt_count: 0,
                    transition_exact_attempt_count: 0,
                    transition_exact_opportunity_count: 0,
                    ko_stage_2_3_attempt_count: 0,
                    ko_stage_3_4_attempt_count: 0,
                    ko_stage_4_5_attempt_count: 0,
                    ko_stage_5_6_attempt_count: 0,
                    ko_stage_6_9_attempt_count: 0,
                    ko_stage_7_9_attempt_count: 0,
                },
            );
            let stage_entry = stage_entries.get(&fact.tournament_id).copied().unwrap_or(
                TournamentStageEntryFact {
                    tournament_id: fact.tournament_id,
                    reached_stage_2_3: false,
                    reached_stage_3_4: false,
                    reached_stage_4_5: false,
                    reached_stage_5_6: false,
                    reached_stage_7_9: false,
                    hero_stage_5_6_stack_chips: None,
                    hero_stage_5_6_stack_bb: None,
                    hero_stage_5_6_entry_players: None,
                    hero_stage_3_4_stack_chips: None,
                    hero_stage_3_4_stack_bb: None,
                    hero_stage_3_4_entry_players: None,
                },
            );

            Some(StackTournamentRecord {
                tournament_id: fact.tournament_id,
                finish_place: summary.and_then(|summary| summary.finish_place),
                buyin_total_cents: summary
                    .map(|summary| summary.buyin_total_cents)
                    .unwrap_or(0),
                payout_cents: summary.map(|summary| summary.payout_cents),
                ft_entry_stack_chips: chips,
                ft_entry_stack_bb: fact.hero_ft_entry_stack_bb,
                total_exact_ko_value: ko.total_exact_ko_share_total,
                early_ft_exact_ko_value: ko.early_ft_exact_ko_share_total,
                ko_stage_5_6_value: stage_event.ko_stage_5_6_share_total,
                ko_stage_6_9_attempt_count: stage_attempt.ko_stage_6_9_attempt_count,
                ko_stage_5_6_attempt_count: stage_attempt.ko_stage_5_6_attempt_count,
                stage_5_6_stack_bb: stage_entry.hero_stage_5_6_stack_bb,
            })
        })
        .collect()
}

fn build_finish_place_chart(
    inputs: &CanonicalQueryInputs,
    places: std::ops::RangeInclusive<i32>,
) -> FtDashboardChart {
    let bars = places
        .map(|place| {
            let count = inputs
                .summary_facts
                .iter()
                .filter(|fact| fact.finish_place == Some(place))
                .count() as u64;
            FtChartBar {
                label: place.to_string(),
                value: count as f64,
                sample_size: count,
                attempts: None,
            }
        })
        .collect::<Vec<_>>();
    let state = if bars.iter().any(|bar| bar.sample_size > 0) {
        FtValueState::Ready
    } else {
        FtValueState::Empty
    };

    FtDashboardChart {
        state,
        metric: "count".to_string(),
        density_options: vec![],
        default_density_step: None,
        variants: BTreeMap::from([(
            "default".to_string(),
            FtChartVariant {
                bars,
                median_label: None,
            },
        )]),
    }
}

fn build_ft_stack_distribution_chart(records: &[StackTournamentRecord]) -> FtDashboardChart {
    build_density_chart(records, &[100, 200, 400, 1000], "count")
}

fn build_ft_stack_metric_chart(
    records: &[StackTournamentRecord],
    metric_key: &str,
) -> FtDashboardChart {
    build_density_chart(records, &[100, 200, 400, 1000], metric_key)
}

fn build_short_roi_chart(records: &[StackTournamentRecord]) -> FtDashboardChart {
    let mut variants = BTreeMap::new();
    for step in [50, 100] {
        let bars = build_short_roi_bars(records, step);
        let state = if bars.iter().any(|bar| bar.sample_size > 0) {
            FtValueState::Ready
        } else {
            FtValueState::Blocked
        };
        variants.insert(
            step.to_string(),
            FtChartVariant {
                bars,
                median_label: None,
            },
        );
        if state == FtValueState::Ready {
            return FtDashboardChart {
                state,
                metric: "roi".to_string(),
                density_options: vec![50, 100],
                default_density_step: Some(50),
                variants,
            };
        }
    }

    FtDashboardChart {
        state: FtValueState::Blocked,
        metric: "roi".to_string(),
        density_options: vec![50, 100],
        default_density_step: Some(50),
        variants,
    }
}

fn build_stage_conversion_chart(
    records: &[StackTournamentRecord],
    stage: &str,
) -> FtDashboardChart {
    let ranges = [
        ("500-1200", 500_i64, 1200_i64),
        ("1200-1800", 1200_i64, 1800_i64),
        ("1800-3000", 1800_i64, 3000_i64),
        ("3000+", 3000_i64, i64::MAX),
    ];

    let bars = ranges
        .into_iter()
        .map(|(label, min, max)| {
            let matching = records
                .iter()
                .filter(|record| {
                    record.ft_entry_stack_chips >= min && record.ft_entry_stack_chips < max
                })
                .collect::<Vec<_>>();

            let sample_size = matching.len() as u64;
            if sample_size == 0 {
                return FtChartBar {
                    label: label.to_string(),
                    value: 0.0,
                    sample_size: 0,
                    attempts: None,
                };
            }

            let (events_sum, denom_sum, attempts_sum) = matching.iter().fold(
                (0.0_f64, 0.0_f64, 0_u64),
                |(events, denom, attempts), record| match stage {
                    "7_9" => (
                        events + record.early_ft_exact_ko_value,
                        denom + record.ft_entry_stack_bb.unwrap_or(0.0),
                        attempts + record.ko_stage_6_9_attempt_count,
                    ),
                    "5_6" => (
                        events + record.ko_stage_5_6_value,
                        denom + record.stage_5_6_stack_bb.unwrap_or(0.0),
                        attempts + record.ko_stage_5_6_attempt_count,
                    ),
                    _ => (events, denom, attempts),
                },
            );

            FtChartBar {
                label: label.to_string(),
                value: if denom_sum > 0.0 {
                    events_sum / denom_sum
                } else {
                    0.0
                },
                sample_size,
                attempts: Some(attempts_sum as f64 / sample_size as f64),
            }
        })
        .collect::<Vec<_>>();

    let state = if bars.iter().any(|bar| bar.sample_size > 0) {
        FtValueState::Ready
    } else {
        FtValueState::Blocked
    };

    FtDashboardChart {
        state,
        metric: "conv".to_string(),
        density_options: vec![],
        default_density_step: None,
        variants: BTreeMap::from([(
            "default".to_string(),
            FtChartVariant {
                bars,
                median_label: None,
            },
        )]),
    }
}

fn build_ko_attempt_distribution_chart(inputs: &CanonicalQueryInputs) -> FtDashboardChart {
    let attempts_by_tournament = inputs
        .stage_attempt_facts
        .iter()
        .map(|fact| {
            (
                fact.tournament_id,
                fact.ko_stage_2_3_attempt_count
                    + fact.ko_stage_3_4_attempt_count
                    + fact.ko_stage_4_5_attempt_count
                    + fact.ko_stage_5_6_attempt_count
                    + fact.ko_stage_6_9_attempt_count
                    + fact.ko_stage_7_9_attempt_count,
            )
        })
        .collect::<BTreeMap<_, _>>();

    let labels = [
        ("1".to_string(), 1_u64, 1_u64),
        ("2".to_string(), 2_u64, 2_u64),
        ("3".to_string(), 3_u64, 3_u64),
        ("4".to_string(), 4_u64, 4_u64),
        ("5+".to_string(), 5_u64, u64::MAX),
    ];
    let bars = labels
        .into_iter()
        .map(|(label, min, max)| {
            let count = attempts_by_tournament
                .values()
                .filter(|attempts| **attempts >= min && **attempts <= max)
                .count() as u64;
            FtChartBar {
                label,
                value: count as f64,
                sample_size: count,
                attempts: None,
            }
        })
        .collect::<Vec<_>>();

    let state = if bars.iter().any(|bar| bar.sample_size > 0) {
        FtValueState::Ready
    } else {
        FtValueState::Blocked
    };

    FtDashboardChart {
        state,
        metric: "count".to_string(),
        density_options: vec![],
        default_density_step: None,
        variants: BTreeMap::from([(
            "default".to_string(),
            FtChartVariant {
                bars,
                median_label: None,
            },
        )]),
    }
}

fn build_avg_ko_by_position_chart(inputs: &CanonicalQueryInputs) -> FtDashboardChart {
    let ko_by_tournament = inputs
        .ko_event_facts
        .iter()
        .map(|fact| (fact.tournament_id, fact.total_exact_ko_share_total))
        .collect::<BTreeMap<_, _>>();

    let bars = (1..=8)
        .map(|place| {
            let matching = inputs
                .summary_facts
                .iter()
                .filter(|fact| fact.finish_place == Some(place))
                .collect::<Vec<_>>();
            let sample_size = matching.len() as u64;
            let total = matching
                .iter()
                .map(|fact| {
                    ko_by_tournament
                        .get(&fact.tournament_id)
                        .copied()
                        .unwrap_or(0.0)
                })
                .sum::<f64>();
            FtChartBar {
                label: place.to_string(),
                value: if sample_size > 0 {
                    total / sample_size as f64
                } else {
                    0.0
                },
                sample_size,
                attempts: None,
            }
        })
        .collect::<Vec<_>>();

    let state = if bars.iter().any(|bar| bar.sample_size > 0) {
        FtValueState::Ready
    } else {
        FtValueState::Blocked
    };

    FtDashboardChart {
        state,
        metric: "avg_ko".to_string(),
        density_options: vec![],
        default_density_step: None,
        variants: BTreeMap::from([(
            "default".to_string(),
            FtChartVariant {
                bars,
                median_label: None,
            },
        )]),
    }
}

fn build_density_chart(
    records: &[StackTournamentRecord],
    density_options: &[i32],
    metric_key: &str,
) -> FtDashboardChart {
    let mut variants = BTreeMap::new();
    for step in density_options {
        let bars = build_ft_stack_bars(records, *step as i64, metric_key);
        variants.insert(
            step.to_string(),
            FtChartVariant {
                median_label: median_label_for_records(records, *step as i64),
                bars,
            },
        );
    }

    let state = if records.is_empty() {
        FtValueState::Blocked
    } else {
        FtValueState::Ready
    };

    FtDashboardChart {
        state,
        metric: metric_key.to_string(),
        density_options: density_options.to_vec(),
        default_density_step: density_options.first().copied(),
        variants,
    }
}

fn build_ft_stack_bars(
    records: &[StackTournamentRecord],
    step: i64,
    metric_key: &str,
) -> Vec<FtChartBar> {
    build_ft_intervals(step)
        .into_iter()
        .map(|(label, min, max)| {
            let matching = records
                .iter()
                .filter(|record| {
                    record.ft_entry_stack_chips >= min && record.ft_entry_stack_chips < max
                })
                .collect::<Vec<_>>();
            let sample_size = matching.len() as u64;
            if sample_size == 0 {
                return FtChartBar {
                    label,
                    value: 0.0,
                    sample_size: 0,
                    attempts: None,
                };
            }

            let value = match metric_key {
                "count" => sample_size as f64,
                "roi" => {
                    let (payout_sum, buyin_sum) =
                        matching
                            .iter()
                            .fold((0_i64, 0_i64), |(payout, buyin), record| {
                                (
                                    payout + record.payout_cents.unwrap_or(0),
                                    buyin + record.buyin_total_cents,
                                )
                            });
                    roi_from_totals(payout_sum, buyin_sum).unwrap_or(0.0)
                }
                "conv" => {
                    let events = matching
                        .iter()
                        .map(|record| record.early_ft_exact_ko_value)
                        .sum::<f64>();
                    let stack_sum = matching
                        .iter()
                        .filter_map(|record| record.ft_entry_stack_bb)
                        .sum::<f64>();
                    ratio_to_float_f64(events, stack_sum).unwrap_or(0.0)
                }
                "avg_ko" => {
                    matching
                        .iter()
                        .map(|record| record.total_exact_ko_value)
                        .sum::<f64>()
                        / sample_size as f64
                }
                "early_avg_ko" => {
                    matching
                        .iter()
                        .map(|record| record.early_ft_exact_ko_value)
                        .sum::<f64>()
                        / sample_size as f64
                }
                _ => 0.0,
            };
            let attempts = match metric_key {
                "conv" => Some(
                    matching
                        .iter()
                        .map(|record| record.ko_stage_6_9_attempt_count)
                        .sum::<u64>() as f64
                        / sample_size as f64,
                ),
                _ => None,
            };

            FtChartBar {
                label,
                value,
                sample_size,
                attempts,
            }
        })
        .collect()
}

fn build_short_roi_bars(records: &[StackTournamentRecord], step: i32) -> Vec<FtChartBar> {
    build_short_intervals(step as i64)
        .into_iter()
        .map(|(label, min, max)| {
            let matching = records
                .iter()
                .filter(|record| {
                    record.ft_entry_stack_chips >= min && record.ft_entry_stack_chips < max
                })
                .collect::<Vec<_>>();
            let sample_size = matching.len() as u64;
            let (payout_sum, buyin_sum) =
                matching
                    .iter()
                    .fold((0_i64, 0_i64), |(payout, buyin), record| {
                        (
                            payout + record.payout_cents.unwrap_or(0),
                            buyin + record.buyin_total_cents,
                        )
                    });
            FtChartBar {
                label,
                value: roi_from_totals(payout_sum, buyin_sum).unwrap_or(0.0),
                sample_size,
                attempts: None,
            }
        })
        .collect()
}

fn build_ft_intervals(step: i64) -> Vec<(String, i64, i64)> {
    let mut intervals = vec![("≤800".to_string(), 0_i64, 800_i64)];
    let mut current = 800_i64;
    while current < 4_000 {
        let next_boundary = current + step;
        if next_boundary > 4_000 {
            break;
        }
        intervals.push((format_range(current, next_boundary), current, next_boundary));
        current = next_boundary;
    }
    intervals.push(("≥4k".to_string(), 4_000_i64, i64::MAX));
    intervals
}

fn build_short_intervals(step: i64) -> Vec<(String, i64, i64)> {
    let mut intervals = Vec::new();
    let mut current = 0_i64;
    while current < 1_500 {
        let next_boundary = current + step;
        intervals.push((format!("{current}-{next_boundary}"), current, next_boundary));
        current = next_boundary;
    }
    intervals
}

fn median_label_for_records(records: &[StackTournamentRecord], step: i64) -> Option<String> {
    let mut values = records
        .iter()
        .map(|record| record.ft_entry_stack_chips)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let median = values[values.len() / 2];
    build_ft_intervals(step)
        .into_iter()
        .find(|(_, min, max)| median >= *min && median < *max)
        .map(|(label, _, _)| label)
        .or_else(|| Some("≥4k".to_string()))
}

fn format_range(start: i64, end: i64) -> String {
    fn label(value: i64) -> String {
        if value >= 1_000 {
            let as_k = (value as f64) / 1_000.0;
            if (as_k - as_k.round()).abs() < f64::EPSILON {
                format!("{}k", as_k.round() as i64)
            } else {
                format!("{as_k:.1}k").replace(".0k", "k")
            }
        } else {
            value.to_string()
        }
    }

    format!("{}-{}", label(start), label(end))
}

fn build_coverage(
    inputs: &CanonicalQueryInputs,
    bundle_id: Option<Uuid>,
    fallback_min: Option<String>,
    fallback_max: Option<String>,
) -> FtDashboardCoverage {
    let summary_tournament_ids = inputs
        .summary_facts
        .iter()
        .map(|fact| fact.tournament_id)
        .collect::<BTreeSet<_>>();
    let hand_tournament_ids = inputs
        .hand_covered_tournaments
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let tournament_ids = inputs
        .tournament_buyin_facts
        .iter()
        .map(|fact| fact.tournament_id)
        .collect::<BTreeSet<_>>();

    FtDashboardCoverage {
        tournament_count: tournament_ids.len() as u64,
        summary_tournament_count: summary_tournament_ids.len() as u64,
        hand_tournament_count: hand_tournament_ids.len() as u64,
        bundle_count: bundle_id.map(|_| 1).unwrap_or(0),
        min_started_at_local: fallback_min,
        max_started_at_local: fallback_max,
    }
}

fn resolve_dashboard_state(
    coverage: &FtDashboardCoverage,
    stat_cards: &BTreeMap<String, FtDashboardMetricCard>,
    charts: &BTreeMap<String, FtDashboardChart>,
) -> FtDashboardDataState {
    if coverage.tournament_count == 0 {
        return FtDashboardDataState::Empty;
    }

    let ready_card_count = stat_cards
        .values()
        .filter(|card| card.state == FtValueState::Ready)
        .count();
    let ready_chart_count = charts
        .values()
        .filter(|chart| chart.state == FtValueState::Ready)
        .count();

    if ready_card_count == 0 && ready_chart_count == 0 {
        return FtDashboardDataState::Blocked;
    }

    if coverage.summary_tournament_count == 0 || coverage.hand_tournament_count == 0 {
        return FtDashboardDataState::Partial;
    }

    FtDashboardDataState::Ready
}

fn single_stat_card(canonical: &CanonicalStatSnapshot, key: &str) -> FtDashboardMetricCard {
    match stat_to_f64(canonical.values.get(key)) {
        Some(value) => FtDashboardMetricCard {
            state: FtValueState::Ready,
            value: Some(value),
            aux_value: None,
        },
        None => blocked_card(),
    }
}

fn complement_percent_card(canonical: &CanonicalStatSnapshot, key: &str) -> FtDashboardMetricCard {
    match stat_to_f64(canonical.values.get(key)) {
        Some(value) => FtDashboardMetricCard {
            state: FtValueState::Ready,
            value: Some(100.0 - value),
            aux_value: None,
        },
        None => blocked_card(),
    }
}

fn dual_stat_card(
    canonical: &CanonicalStatSnapshot,
    primary_key: &str,
    aux_key: &str,
) -> FtDashboardMetricCard {
    let primary = stat_to_f64(canonical.values.get(primary_key));
    let aux = stat_to_f64(canonical.values.get(aux_key));
    if primary.is_none() {
        return blocked_card();
    }

    FtDashboardMetricCard {
        state: FtValueState::Ready,
        value: primary,
        aux_value: aux,
    }
}

fn blocked_card() -> FtDashboardMetricCard {
    FtDashboardMetricCard {
        state: FtValueState::Blocked,
        value: None,
        aux_value: None,
    }
}

fn single_inline_stat(canonical: &CanonicalStatSnapshot, key: &str) -> FtDashboardInlineStat {
    FtDashboardInlineStat {
        state: if stat_to_f64(canonical.values.get(key)).is_some() {
            FtValueState::Ready
        } else {
            FtValueState::Blocked
        },
        value: stat_to_f64(canonical.values.get(key)),
    }
}

fn stat_to_f64(point: Option<&CanonicalStatPoint>) -> Option<f64> {
    let point = point?;
    match &point.value {
        Some(CanonicalStatNumericValue::Integer(value)) => Some(*value as f64),
        Some(CanonicalStatNumericValue::Float(value)) => Some(*value),
        None => None,
    }
}

fn ratio_to_float_f64(numerator: f64, denominator: f64) -> Option<f64> {
    (denominator > 0.0).then_some(numerator / denominator)
}

fn roi_from_totals(payout_cents: i64, buyin_cents: i64) -> Option<f64> {
    (buyin_cents > 0).then_some(((payout_cents - buyin_cents) as f64 / buyin_cents as f64) * 100.0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        FtValueState, build_big_ko_cards, build_inline_stats, build_stat_cards,
    };
    use crate::models::{CanonicalStatPoint, CanonicalStatSnapshot, SeedStatCoverage};

    fn snapshot_with_values(values: BTreeMap<String, CanonicalStatPoint>) -> CanonicalStatSnapshot {
        CanonicalStatSnapshot {
            coverage: SeedStatCoverage {
                summary_tournament_count: 1,
                hand_tournament_count: 1,
            },
            values,
        }
    }

    #[test]
    fn dashboard_money_surface_uses_canonical_money_and_big_ko_values() {
        let canonical = snapshot_with_values(BTreeMap::from([
            (
                "ko_contribution".to_string(),
                CanonicalStatPoint::from_optional_float(Some(43.825069294733595)),
            ),
            (
                "ko_luck".to_string(),
                CanonicalStatPoint::from_optional_float(Some(-441.25)),
            ),
            (
                "roi_adj".to_string(),
                CanonicalStatPoint::from_optional_float(Some(0.7415254237288136)),
            ),
            (
                "total_ko".to_string(),
                CanonicalStatPoint::from_optional_float(Some(78.0)),
            ),
            (
                "big_ko_x1_5_count".to_string(),
                CanonicalStatPoint::from_optional_float(Some(0.0)),
            ),
            (
                "big_ko_x2_count".to_string(),
                CanonicalStatPoint::from_optional_float(Some(1.0)),
            ),
            (
                "big_ko_x10_count".to_string(),
                CanonicalStatPoint::from_optional_float(Some(0.0)),
            ),
            (
                "big_ko_x100_count".to_string(),
                CanonicalStatPoint::from_optional_float(Some(0.0)),
            ),
            (
                "big_ko_x1000_count".to_string(),
                CanonicalStatPoint::from_optional_float(Some(0.0)),
            ),
            (
                "big_ko_x10000_count".to_string(),
                CanonicalStatPoint::from_optional_float(Some(0.0)),
            ),
        ]));

        let stat_cards = build_stat_cards(&canonical);
        let inline_stats = build_inline_stats(&canonical);
        let big_ko_cards = build_big_ko_cards(&canonical);

        assert_eq!(stat_cards["winningsFromKo"].state, FtValueState::Ready);
        assert_eq!(
            stat_cards["winningsFromKo"].value,
            Some(43.825069294733595)
        );
        assert_eq!(stat_cards["winningsFromItm"].state, FtValueState::Ready);
        assert_eq!(
            stat_cards["winningsFromItm"].value,
            Some(56.174930705266405)
        );
        assert_eq!(inline_stats["koLuck"].state, FtValueState::Ready);
        assert_eq!(inline_stats["koLuck"].value, Some(-441.25));
        assert_eq!(inline_stats["roiAdj"].state, FtValueState::Ready);
        assert_eq!(inline_stats["roiAdj"].value, Some(0.7415254237288136));

        let x2_card = big_ko_cards
            .iter()
            .find(|card| card.tier == "x2")
            .expect("x2 big ko card");
        assert_eq!(x2_card.state, FtValueState::Ready);
        assert_eq!(x2_card.count, Some(1.0));
        assert_eq!(x2_card.occurs_once_every_kos, Some(78.0));

        let x15_card = big_ko_cards
            .iter()
            .find(|card| card.tier == "x1.5")
            .expect("x1.5 big ko card");
        assert_eq!(x15_card.state, FtValueState::Ready);
        assert_eq!(x15_card.count, Some(0.0));
        assert_eq!(x15_card.occurs_once_every_kos, None);
    }

    #[test]
    fn dashboard_money_surface_blocks_when_canonical_values_are_missing() {
        let canonical = snapshot_with_values(BTreeMap::new());

        let stat_cards = build_stat_cards(&canonical);
        let inline_stats = build_inline_stats(&canonical);
        let big_ko_cards = build_big_ko_cards(&canonical);

        assert_eq!(stat_cards["winningsFromKo"].state, FtValueState::Blocked);
        assert_eq!(stat_cards["winningsFromItm"].state, FtValueState::Blocked);
        assert_eq!(inline_stats["koLuck"].state, FtValueState::Blocked);
        assert_eq!(inline_stats["roiAdj"].state, FtValueState::Blocked);
        assert!(big_ko_cards
            .iter()
            .all(|card| card.state == FtValueState::Blocked));
    }
}
