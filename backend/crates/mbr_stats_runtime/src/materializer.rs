use std::collections::BTreeMap;

use anyhow::Result;
use postgres::{GenericClient, Row, types::ToSql};
use uuid::Uuid;

use crate::{
    models::{
        HandFeatureFacts, MaterializationReport, MaterializedHandFeatures,
        MaterializedStreetFeatures, StreetFeatureFacts, StreetFeatureParticipant,
    },
    registry::{
        FEATURE_VERSION, FeatureGrain, FeatureTableFamily, GG_MBR_FT_MAX_PLAYERS, feature_registry,
        ft_stage_bucket,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StreetFeatureRowSummary {
    street_row_count: u64,
    street_bool_rows: u64,
    street_num_rows: u64,
    street_enum_rows: u64,
}

const INSERT_CHUNK_SIZE: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandBoolInsertRow {
    hand_id: Uuid,
    feature_key: String,
    value: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct HandNumInsertRow {
    hand_id: Uuid,
    feature_key: String,
    value: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandEnumInsertRow {
    hand_id: Uuid,
    feature_key: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreetBoolInsertRow {
    hand_id: Uuid,
    seat_no: i32,
    street: String,
    feature_key: String,
    value: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct StreetNumInsertRow {
    hand_id: Uuid,
    seat_no: i32,
    street: String,
    feature_key: String,
    value: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreetEnumInsertRow {
    hand_id: Uuid,
    seat_no: i32,
    street: String,
    feature_key: String,
    value: String,
}

pub fn materialize_player_hand_features(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<MaterializationReport> {
    materialize_player_hand_features_inner(client, organization_id, player_profile_id, None)
}

pub fn materialize_player_hand_features_for_tournaments(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    tournament_ids: &[Uuid],
) -> Result<MaterializationReport> {
    if tournament_ids.is_empty() {
        return Ok(empty_materialization_report());
    }

    materialize_player_hand_features_inner(
        client,
        organization_id,
        player_profile_id,
        Some(tournament_ids),
    )
}

pub fn materialize_player_hand_features_for_bundle(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    bundle_id: Uuid,
) -> Result<MaterializationReport> {
    let tournament_ids =
        load_bundle_tournament_ids(client, bundle_id, organization_id, player_profile_id)?;
    materialize_player_hand_features_for_tournaments(
        client,
        organization_id,
        player_profile_id,
        &tournament_ids,
    )
}

fn materialize_player_hand_features_inner(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    tournament_ids: Option<&[Uuid]>,
) -> Result<MaterializationReport> {
    match tournament_ids {
        Some(ids) => {
            delete_existing_feature_rows_for_tournaments(
                client,
                organization_id,
                player_profile_id,
                ids,
            )?;
        }
        None => delete_existing_feature_rows(client, organization_id, player_profile_id)?,
    }

    let hand_facts =
        load_hand_feature_facts(client, organization_id, player_profile_id, tournament_ids)?;
    let hand_rows = build_feature_rows(&hand_facts);
    persist_feature_rows(client, organization_id, player_profile_id, &hand_rows)?;

    let street_facts =
        load_street_feature_facts(client, organization_id, player_profile_id, tournament_ids)?;
    let street_rows = build_street_feature_rows(&street_facts);
    persist_street_feature_rows(client, organization_id, player_profile_id, &street_rows)?;
    let street_summary = summarize_street_feature_rows(&street_rows);

    Ok(MaterializationReport {
        hand_count: hand_rows.len() as u64,
        bool_rows: hand_rows
            .iter()
            .flat_map(|row| row.bool_values.values())
            .count() as u64,
        num_rows: hand_rows
            .iter()
            .flat_map(|row| row.num_values.values())
            .filter(|value| value.is_some())
            .count() as u64,
        enum_rows: hand_rows
            .iter()
            .flat_map(|row| row.enum_values.values())
            .count() as u64,
        street_row_count: street_summary.street_row_count,
        street_bool_rows: street_summary.street_bool_rows,
        street_num_rows: street_summary.street_num_rows,
        street_enum_rows: street_summary.street_enum_rows,
    })
}

fn empty_materialization_report() -> MaterializationReport {
    MaterializationReport {
        hand_count: 0,
        bool_rows: 0,
        num_rows: 0,
        enum_rows: 0,
        street_row_count: 0,
        street_bool_rows: 0,
        street_num_rows: 0,
        street_enum_rows: 0,
    }
}

fn load_bundle_tournament_ids(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<Uuid>> {
    // Join through source_file_member_id → file_fragments → hands.
    // The old source_file_id join caused a combinatorial explosion when
    // all member jobs shared one parent archive (prepared-pairs.zip):
    // 16K jobs × 220K hands = ~3.6 billion row cross product.
    Ok(client
        .query(
            "SELECT DISTINCT h.tournament_id
             FROM import.import_jobs jobs
             INNER JOIN import.file_fragments ff
               ON ff.source_file_member_id = jobs.source_file_member_id
             INNER JOIN core.hands h
               ON h.raw_fragment_id = ff.id
             WHERE jobs.bundle_id = $1
               AND jobs.job_kind = 'file_ingest'
               AND h.organization_id = $2
               AND h.player_profile_id = $3
             ORDER BY h.tournament_id",
            &[&bundle_id, &organization_id, &player_profile_id],
        )?
        .into_iter()
        .map(|row| row.get(0))
        .collect())
}

fn delete_existing_feature_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<()> {
    for table in [
        "analytics.player_hand_bool_features",
        "analytics.player_hand_num_features",
        "analytics.player_hand_enum_features",
        "analytics.player_street_bool_features",
        "analytics.player_street_num_features",
        "analytics.player_street_enum_features",
    ] {
        client.execute(
            &format!(
                "DELETE FROM {table}
                 WHERE organization_id = $1
                   AND player_profile_id = $2
                   AND feature_version = $3"
            ),
            &[&organization_id, &player_profile_id, &FEATURE_VERSION],
        )?;
    }

    Ok(())
}

fn delete_existing_feature_rows_for_tournaments(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    tournament_ids: &[Uuid],
) -> Result<()> {
    if tournament_ids.is_empty() {
        return Ok(());
    }

    let tournament_clause = placeholder_list(4, tournament_ids.len());
    let mut params: Vec<&(dyn ToSql + Sync)> =
        vec![&organization_id, &player_profile_id, &FEATURE_VERSION];
    params.extend(tournament_ids.iter().map(|id| id as &(dyn ToSql + Sync)));

    for table in [
        "analytics.player_hand_bool_features",
        "analytics.player_hand_num_features",
        "analytics.player_hand_enum_features",
        "analytics.player_street_bool_features",
        "analytics.player_street_num_features",
        "analytics.player_street_enum_features",
    ] {
        let statement = format!(
            "DELETE FROM {table} AS target
             USING core.hands AS hands
             WHERE target.hand_id = hands.id
               AND target.organization_id = $1
               AND target.player_profile_id = $2
               AND target.feature_version = $3
               AND hands.organization_id = $1
               AND hands.player_profile_id = $2
               AND hands.tournament_id IN ({tournament_clause})"
        );
        client.execute(&statement, &params)?;
    }

    Ok(())
}

fn placeholder_list(start_index: usize, item_count: usize) -> String {
    (0..item_count)
        .map(|offset| format!("${}", start_index + offset))
        .collect::<Vec<_>>()
        .join(", ")
}

fn load_hand_feature_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    tournament_ids: Option<&[Uuid]>,
) -> Result<Vec<HandFeatureFacts>> {
    let mut statement = String::from(
        "WITH hand_attempt_counts AS (
            SELECT
                hand_id,
                COUNT(*)::bigint AS ko_attempt_count
             FROM derived.hand_ko_attempts
             WHERE player_profile_id = $2
             GROUP BY hand_id
         ),
         hand_opportunity_counts AS (
            SELECT
                hand_id,
                COUNT(*)::bigint AS ko_opportunity_count
             FROM derived.hand_ko_opportunities
             WHERE player_profile_id = $2
             GROUP BY hand_id
         )
         SELECT
            h.id,
            h.tournament_id,
            CASE
                WHEN msr.played_ft_hand IS TRUE
                 AND msr.played_ft_hand_state = 'exact'
                THEN TRUE
                ELSE FALSE
            END AS played_ft_hand,
            CASE
                WHEN msr.played_ft_hand IS TRUE
                 AND msr.played_ft_hand_state = 'exact'
                THEN msr.ft_table_size
                ELSE NULL
            END AS ft_table_size,
            COALESCE(msr.is_boundary_hand, FALSE) AS is_boundary_hand,
            COALESCE(SUM(
                CASE
                    WHEN hero_winner.hand_id IS NOT NULL
                     AND he.ko_certainty_state = 'exact'
                    THEN 1
                    ELSE 0
                END
            ), 0)::bigint AS exact_ko_count,
            COALESCE(SUM(
                CASE
                    WHEN hero_winner.hand_id IS NOT NULL
                     AND he.ko_certainty_state = 'exact'
                     AND COALESCE(array_length(he.ko_winner_set, 1), 0) > 1
                    THEN 1
                    ELSE 0
                END
            ), 0)::bigint AS split_ko_count,
            COALESCE(SUM(
                CASE
                    WHEN hero_winner.hand_id IS NOT NULL
                     AND he.ko_certainty_state = 'exact'
                     AND COALESCE(he.last_busting_pot_no, 0) > 1
                    THEN 1
                    ELSE 0
                END
            ), 0)::bigint AS sidepot_ko_count,
            COALESCE(hand_attempt_counts.ko_attempt_count, 0)::bigint AS ko_attempt_count,
            COALESCE(hand_opportunity_counts.ko_opportunity_count, 0)::bigint AS ko_opportunity_count
         FROM core.hands h
         LEFT JOIN derived.mbr_stage_resolution msr
           ON msr.hand_id = h.id
          AND msr.player_profile_id = h.player_profile_id
         LEFT JOIN derived.hand_eliminations he
           ON he.hand_id = h.id
         LEFT JOIN core.hand_seats hero_winner
           ON hero_winner.hand_id = he.hand_id
          AND hero_winner.is_hero IS TRUE
          AND hero_winner.player_name = ANY(he.ko_winner_set)
         LEFT JOIN hand_attempt_counts
           ON hand_attempt_counts.hand_id = h.id
         LEFT JOIN hand_opportunity_counts
           ON hand_opportunity_counts.hand_id = h.id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2"
    );
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&organization_id, &player_profile_id];
    if let Some(ids) = tournament_ids {
        statement.push_str(&format!(
            "
           AND h.tournament_id IN ({})",
            placeholder_list(3, ids.len())
        ));
        params.extend(ids.iter().map(|id| id as &(dyn ToSql + Sync)));
    }
    statement.push_str(
        "
         GROUP BY
            h.id,
            h.tournament_id,
            msr.played_ft_hand,
            msr.played_ft_hand_state,
            msr.ft_table_size,
            msr.is_boundary_hand,
            hand_attempt_counts.ko_attempt_count,
            hand_opportunity_counts.ko_opportunity_count
         ORDER BY h.id",
    );

    let rows = client.query(&statement, &params)?;
    Ok(rows.into_iter().map(row_to_hand_feature_facts).collect())
}

fn row_to_hand_feature_facts(row: Row) -> HandFeatureFacts {
    HandFeatureFacts {
        hand_id: row.get(0),
        tournament_id: row.get(1),
        played_ft_hand: row.get(2),
        ft_table_size: row.get(3),
        is_boundary_hand: row.get(4),
        exact_ko_count: row.get::<_, i64>(5) as u32,
        split_ko_count: row.get::<_, i64>(6) as u32,
        sidepot_ko_count: row.get::<_, i64>(7) as u32,
        ko_attempt_count: row.get::<_, i64>(8) as u32,
        ko_opportunity_count: row.get::<_, i64>(9) as u32,
    }
}

fn load_street_feature_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    tournament_ids: Option<&[Uuid]>,
) -> Result<Vec<StreetFeatureFacts>> {
    let mut postflop_statement = String::from(
        "SELECT
            shs.hand_id,
            shs.seat_no,
            shs.street,
            hs.is_hero,
            COALESCE(hhc.known_at_showdown, FALSE),
            shs.best_hand_class,
            shs.best_hand_rank_value,
            shs.made_hand_category,
            shs.draw_category,
            shs.overcards_count,
            shs.has_air,
            shs.missed_flush_draw,
            shs.missed_straight_draw,
            shs.certainty_state
         FROM core.hands h
         INNER JOIN derived.street_hand_strength shs
           ON shs.hand_id = h.id
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = shs.hand_id
          AND hs.seat_no = shs.seat_no
         LEFT JOIN core.hand_hole_cards hhc
           ON hhc.hand_id = shs.hand_id
          AND hhc.seat_no = shs.seat_no
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2",
    );
    let mut postflop_params: Vec<&(dyn ToSql + Sync)> = vec![&organization_id, &player_profile_id];
    if let Some(ids) = tournament_ids {
        postflop_statement.push_str(&format!(
            "
           AND h.tournament_id IN ({})",
            placeholder_list(3, ids.len())
        ));
        postflop_params.extend(ids.iter().map(|id| id as &(dyn ToSql + Sync)));
    }
    postflop_statement.push_str(
        "
         ORDER BY shs.hand_id, shs.seat_no, shs.street",
    );

    let mut rows = client
        .query(&postflop_statement, &postflop_params)?
        .into_iter()
        .map(row_to_postflop_street_feature_facts)
        .collect::<Vec<_>>();

    let mut preflop_statement = String::from(
        "SELECT
            psh.hand_id,
            psh.seat_no,
            'preflop'::text AS street,
            hs.is_hero,
            COALESCE(hhc.known_at_showdown, FALSE),
            psh.starter_hand_class,
            psh.certainty_state
         FROM core.hands h
         INNER JOIN derived.preflop_starting_hands psh
           ON psh.hand_id = h.id
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = psh.hand_id
          AND hs.seat_no = psh.seat_no
         LEFT JOIN core.hand_hole_cards hhc
           ON hhc.hand_id = psh.hand_id
          AND hhc.seat_no = psh.seat_no
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2",
    );
    let mut preflop_params: Vec<&(dyn ToSql + Sync)> = vec![&organization_id, &player_profile_id];
    if let Some(ids) = tournament_ids {
        preflop_statement.push_str(&format!(
            "
           AND h.tournament_id IN ({})",
            placeholder_list(3, ids.len())
        ));
        preflop_params.extend(ids.iter().map(|id| id as &(dyn ToSql + Sync)));
    }
    preflop_statement.push_str(
        "
         ORDER BY psh.hand_id, psh.seat_no",
    );

    rows.extend(
        client
            .query(&preflop_statement, &preflop_params)?
            .into_iter()
            .map(row_to_preflop_street_feature_facts),
    );

    Ok(rows)
}

fn row_to_postflop_street_feature_facts(row: Row) -> StreetFeatureFacts {
    let is_hero: bool = row.get(3);
    let known_at_showdown: bool = row.get(4);

    StreetFeatureFacts {
        hand_id: row.get(0),
        seat_no: row.get(1),
        street: row.get(2),
        participant: if is_hero {
            StreetFeatureParticipant::Hero
        } else if known_at_showdown {
            StreetFeatureParticipant::ShowdownKnownOpponent
        } else {
            StreetFeatureParticipant::UnknownOpponent
        },
        starter_hand_class: None,
        best_hand_class: Some(row.get(5)),
        best_hand_rank_value: row.get(6),
        made_hand_category: Some(row.get(7)),
        draw_category: Some(row.get(8)),
        overcards_count: Some(row.get(9)),
        has_air: Some(row.get(10)),
        missed_flush_draw: Some(row.get(11)),
        missed_straight_draw: Some(row.get(12)),
        certainty_state: row.get(13),
    }
}

fn row_to_preflop_street_feature_facts(row: Row) -> StreetFeatureFacts {
    let is_hero: bool = row.get(3);
    let known_at_showdown: bool = row.get(4);

    StreetFeatureFacts {
        hand_id: row.get(0),
        seat_no: row.get(1),
        street: row.get(2),
        participant: if is_hero {
            StreetFeatureParticipant::Hero
        } else if known_at_showdown {
            StreetFeatureParticipant::ShowdownKnownOpponent
        } else {
            StreetFeatureParticipant::UnknownOpponent
        },
        starter_hand_class: Some(row.get(5)),
        best_hand_class: None,
        best_hand_rank_value: None,
        made_hand_category: None,
        draw_category: None,
        overcards_count: None,
        has_air: None,
        missed_flush_draw: None,
        missed_straight_draw: None,
        certainty_state: row.get(6),
    }
}

pub(crate) fn build_feature_rows(facts: &[HandFeatureFacts]) -> Vec<MaterializedHandFeatures> {
    facts
        .iter()
        .map(|fact| {
            let mut bool_values = BTreeMap::new();
            bool_values.insert("played_ft_hand".to_string(), fact.played_ft_hand);
            bool_values.insert("is_ft_hand".to_string(), fact.played_ft_hand);
            bool_values.insert("is_stage_2".to_string(), fact.ft_table_size == Some(2));
            bool_values.insert(
                "is_stage_3_4".to_string(),
                matches!(fact.ft_table_size, Some(3 | 4)),
            );
            bool_values.insert(
                "is_stage_4_5".to_string(),
                matches!(fact.ft_table_size, Some(4 | 5)),
            );
            bool_values.insert(
                "is_stage_5_6".to_string(),
                matches!(fact.ft_table_size, Some(5 | 6)),
            );
            bool_values.insert(
                "is_stage_6_9".to_string(),
                matches!(fact.ft_table_size, Some(6..=GG_MBR_FT_MAX_PLAYERS)),
            );
            bool_values.insert("is_boundary_hand".to_string(), fact.is_boundary_hand);
            bool_values.insert("has_exact_ko_event".to_string(), fact.exact_ko_count > 0);
            bool_values.insert("has_split_ko_event".to_string(), fact.split_ko_count > 0);
            bool_values.insert(
                "has_sidepot_ko_event".to_string(),
                fact.sidepot_ko_count > 0,
            );
            bool_values.insert("has_ko_attempt".to_string(), fact.ko_attempt_count > 0);
            bool_values.insert(
                "has_ko_opportunity".to_string(),
                fact.ko_opportunity_count > 0,
            );

            let mut num_values = BTreeMap::new();
            num_values.insert(
                "ft_table_size".to_string(),
                Some(fact.ft_table_size.unwrap_or_default() as f64),
            );
            num_values.insert(
                "ft_players_remaining_exact".to_string(),
                fact.ft_table_size.map(|value| value as f64),
            );
            num_values.insert(
                "hero_exact_ko_event_count".to_string(),
                Some(fact.exact_ko_count as f64),
            );
            num_values.insert(
                "hero_split_ko_event_count".to_string(),
                Some(fact.split_ko_count as f64),
            );
            num_values.insert(
                "hero_sidepot_ko_event_count".to_string(),
                Some(fact.sidepot_ko_count as f64),
            );
            num_values.insert(
                "hero_ko_attempt_count".to_string(),
                Some(fact.ko_attempt_count as f64),
            );
            num_values.insert(
                "hero_ko_opportunity_count".to_string(),
                Some(fact.ko_opportunity_count as f64),
            );

            let mut enum_values = BTreeMap::new();
            enum_values.insert(
                "ft_stage_bucket".to_string(),
                ft_stage_bucket(fact.played_ft_hand, fact.ft_table_size)
                    .as_str()
                    .to_string(),
            );

            MaterializedHandFeatures {
                hand_id: fact.hand_id,
                tournament_id: fact.tournament_id,
                bool_values,
                num_values,
                enum_values,
            }
        })
        .collect()
}

pub(crate) fn build_street_feature_rows(
    facts: &[StreetFeatureFacts],
) -> Vec<MaterializedStreetFeatures> {
    facts
        .iter()
        .filter(|fact| {
            matches!(
                fact.participant,
                StreetFeatureParticipant::Hero | StreetFeatureParticipant::ShowdownKnownOpponent
            )
        })
        .map(|fact| {
            let mut bool_values = BTreeMap::new();
            if let Some(has_air) = fact.has_air {
                bool_values.insert("has_air".to_string(), has_air);
            }
            if let Some(missed_flush_draw) = fact.missed_flush_draw {
                bool_values.insert("missed_flush_draw".to_string(), missed_flush_draw);
            }
            if let Some(missed_straight_draw) = fact.missed_straight_draw {
                bool_values.insert("missed_straight_draw".to_string(), missed_straight_draw);
            }

            let mut num_values = BTreeMap::new();
            if let Some(best_hand_rank_value) = fact.best_hand_rank_value {
                num_values.insert(
                    "best_hand_rank_value".to_string(),
                    Some(best_hand_rank_value as f64),
                );
            }
            if let Some(overcards_count) = fact.overcards_count {
                num_values.insert("overcards_count".to_string(), Some(overcards_count as f64));
            }

            let mut enum_values = BTreeMap::new();
            if let Some(starter_hand_class) = &fact.starter_hand_class {
                enum_values.insert("starter_hand_class".to_string(), starter_hand_class.clone());
            }
            if let Some(best_hand_class) = &fact.best_hand_class {
                enum_values.insert("best_hand_class".to_string(), best_hand_class.clone());
            }
            if let Some(made_hand_category) = &fact.made_hand_category {
                enum_values.insert("made_hand_category".to_string(), made_hand_category.clone());
            }
            if let Some(draw_category) = &fact.draw_category {
                enum_values.insert("draw_category".to_string(), draw_category.clone());
            }
            enum_values.insert("certainty_state".to_string(), fact.certainty_state.clone());

            MaterializedStreetFeatures {
                hand_id: fact.hand_id,
                seat_no: fact.seat_no,
                street: fact.street.clone(),
                bool_values,
                num_values,
                enum_values,
            }
        })
        .collect()
}

fn summarize_street_feature_rows(rows: &[MaterializedStreetFeatures]) -> StreetFeatureRowSummary {
    StreetFeatureRowSummary {
        street_row_count: rows.len() as u64,
        street_bool_rows: rows
            .iter()
            .map(|row| row.bool_values.len() as u64)
            .sum::<u64>(),
        street_num_rows: rows
            .iter()
            .flat_map(|row| row.num_values.values())
            .filter(|value| value.is_some())
            .count() as u64,
        street_enum_rows: rows
            .iter()
            .map(|row| row.enum_values.len() as u64)
            .sum::<u64>(),
    }
}

fn persist_feature_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[MaterializedHandFeatures],
) -> Result<()> {
    let registry = feature_registry();
    let mut bool_rows = Vec::new();
    let mut num_rows = Vec::new();
    let mut enum_rows = Vec::new();

    for row in rows {
        for feature in registry
            .iter()
            .filter(|feature| feature.grain == FeatureGrain::Hand)
        {
            match feature.table_family {
                FeatureTableFamily::Bool => bool_rows.push(HandBoolInsertRow {
                    hand_id: row.hand_id,
                    feature_key: feature.key.to_string(),
                    value: row.bool_values[feature.key],
                }),
                FeatureTableFamily::Num => {
                    let Some(value) = row.num_values[feature.key] else {
                        continue;
                    };
                    num_rows.push(HandNumInsertRow {
                        hand_id: row.hand_id,
                        feature_key: feature.key.to_string(),
                        value,
                    });
                }
                FeatureTableFamily::Enum => enum_rows.push(HandEnumInsertRow {
                    hand_id: row.hand_id,
                    feature_key: feature.key.to_string(),
                    value: row.enum_values[feature.key].clone(),
                }),
            }
        }
    }

    insert_hand_bool_rows(client, organization_id, player_profile_id, &bool_rows)?;
    insert_hand_num_rows(client, organization_id, player_profile_id, &num_rows)?;
    insert_hand_enum_rows(client, organization_id, player_profile_id, &enum_rows)?;

    Ok(())
}

fn insert_hand_bool_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[HandBoolInsertRow],
) -> Result<()> {
    for chunk in rows.chunks(INSERT_CHUNK_SIZE) {
        let mut params: Vec<&(dyn ToSql + Sync)> =
            vec![&organization_id, &player_profile_id, &FEATURE_VERSION];
        let mut values = String::new();
        for (index, row) in chunk.iter().enumerate() {
            let base = 4 + index * 3;
            if !values.is_empty() {
                values.push_str(", ");
            }
            values.push_str(&format!(
                "($1, $2, ${}, ${}, $3, ${})",
                base,
                base + 1,
                base + 2
            ));
            params.push(&row.hand_id);
            params.push(&row.feature_key);
            params.push(&row.value);
        }
        client.execute(
            &format!(
                "INSERT INTO analytics.player_hand_bool_features (
                    organization_id,
                    player_profile_id,
                    hand_id,
                    feature_key,
                    feature_version,
                    value
                )
                VALUES {values}"
            ),
            &params,
        )?;
    }

    Ok(())
}

fn insert_hand_num_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[HandNumInsertRow],
) -> Result<()> {
    for chunk in rows.chunks(INSERT_CHUNK_SIZE) {
        let mut params: Vec<&(dyn ToSql + Sync)> =
            vec![&organization_id, &player_profile_id, &FEATURE_VERSION];
        let mut values = String::new();
        for (index, row) in chunk.iter().enumerate() {
            let base = 4 + index * 3;
            if !values.is_empty() {
                values.push_str(", ");
            }
            values.push_str(&format!(
                "($1, $2, ${}, ${}, $3, (${}::double precision)::numeric(18,6))",
                base,
                base + 1,
                base + 2
            ));
            params.push(&row.hand_id);
            params.push(&row.feature_key);
            params.push(&row.value);
        }
        client.execute(
            &format!(
                "INSERT INTO analytics.player_hand_num_features (
                    organization_id,
                    player_profile_id,
                    hand_id,
                    feature_key,
                    feature_version,
                    value
                )
                VALUES {values}"
            ),
            &params,
        )?;
    }

    Ok(())
}

fn insert_hand_enum_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[HandEnumInsertRow],
) -> Result<()> {
    for chunk in rows.chunks(INSERT_CHUNK_SIZE) {
        let mut params: Vec<&(dyn ToSql + Sync)> =
            vec![&organization_id, &player_profile_id, &FEATURE_VERSION];
        let mut values = String::new();
        for (index, row) in chunk.iter().enumerate() {
            let base = 4 + index * 3;
            if !values.is_empty() {
                values.push_str(", ");
            }
            values.push_str(&format!(
                "($1, $2, ${}, ${}, $3, ${})",
                base,
                base + 1,
                base + 2
            ));
            params.push(&row.hand_id);
            params.push(&row.feature_key);
            params.push(&row.value);
        }
        client.execute(
            &format!(
                "INSERT INTO analytics.player_hand_enum_features (
                    organization_id,
                    player_profile_id,
                    hand_id,
                    feature_key,
                    feature_version,
                    value
                )
                VALUES {values}"
            ),
            &params,
        )?;
    }

    Ok(())
}

fn persist_street_feature_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[MaterializedStreetFeatures],
) -> Result<()> {
    let registry = feature_registry();
    let mut bool_rows = Vec::new();
    let mut num_rows = Vec::new();
    let mut enum_rows = Vec::new();

    for row in rows {
        for feature in registry
            .iter()
            .filter(|feature| feature.grain == FeatureGrain::Street)
        {
            match feature.table_family {
                FeatureTableFamily::Bool => {
                    let Some(value) = row.bool_values.get(feature.key) else {
                        continue;
                    };
                    bool_rows.push(StreetBoolInsertRow {
                        hand_id: row.hand_id,
                        seat_no: row.seat_no,
                        street: row.street.clone(),
                        feature_key: feature.key.to_string(),
                        value: *value,
                    });
                }
                FeatureTableFamily::Num => {
                    let Some(Some(value)) = row.num_values.get(feature.key) else {
                        continue;
                    };
                    num_rows.push(StreetNumInsertRow {
                        hand_id: row.hand_id,
                        seat_no: row.seat_no,
                        street: row.street.clone(),
                        feature_key: feature.key.to_string(),
                        value: *value,
                    });
                }
                FeatureTableFamily::Enum => {
                    let Some(value) = row.enum_values.get(feature.key) else {
                        continue;
                    };
                    enum_rows.push(StreetEnumInsertRow {
                        hand_id: row.hand_id,
                        seat_no: row.seat_no,
                        street: row.street.clone(),
                        feature_key: feature.key.to_string(),
                        value: value.clone(),
                    });
                }
            }
        }
    }

    insert_street_bool_rows(client, organization_id, player_profile_id, &bool_rows)?;
    insert_street_num_rows(client, organization_id, player_profile_id, &num_rows)?;
    insert_street_enum_rows(client, organization_id, player_profile_id, &enum_rows)?;

    Ok(())
}

fn insert_street_bool_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[StreetBoolInsertRow],
) -> Result<()> {
    for chunk in rows.chunks(INSERT_CHUNK_SIZE) {
        let mut params: Vec<&(dyn ToSql + Sync)> =
            vec![&organization_id, &player_profile_id, &FEATURE_VERSION];
        let mut values = String::new();
        for (index, row) in chunk.iter().enumerate() {
            let base = 4 + index * 5;
            if !values.is_empty() {
                values.push_str(", ");
            }
            values.push_str(&format!(
                "($1, $2, ${}, ${}, ${}, ${}, $3, ${})",
                base,
                base + 1,
                base + 2,
                base + 3,
                base + 4
            ));
            params.push(&row.hand_id);
            params.push(&row.seat_no);
            params.push(&row.street);
            params.push(&row.feature_key);
            params.push(&row.value);
        }
        client.execute(
            &format!(
                "INSERT INTO analytics.player_street_bool_features (
                    organization_id,
                    player_profile_id,
                    hand_id,
                    seat_no,
                    street,
                    feature_key,
                    feature_version,
                    value
                )
                VALUES {values}"
            ),
            &params,
        )?;
    }

    Ok(())
}

fn insert_street_num_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[StreetNumInsertRow],
) -> Result<()> {
    for chunk in rows.chunks(INSERT_CHUNK_SIZE) {
        let mut params: Vec<&(dyn ToSql + Sync)> =
            vec![&organization_id, &player_profile_id, &FEATURE_VERSION];
        let mut values = String::new();
        for (index, row) in chunk.iter().enumerate() {
            let base = 4 + index * 5;
            if !values.is_empty() {
                values.push_str(", ");
            }
            values.push_str(&format!(
                "($1, $2, ${}, ${}, ${}, ${}, $3, (${}::double precision)::numeric(18,6))",
                base,
                base + 1,
                base + 2,
                base + 3,
                base + 4
            ));
            params.push(&row.hand_id);
            params.push(&row.seat_no);
            params.push(&row.street);
            params.push(&row.feature_key);
            params.push(&row.value);
        }
        client.execute(
            &format!(
                "INSERT INTO analytics.player_street_num_features (
                    organization_id,
                    player_profile_id,
                    hand_id,
                    seat_no,
                    street,
                    feature_key,
                    feature_version,
                    value
                )
                VALUES {values}"
            ),
            &params,
        )?;
    }

    Ok(())
}

fn insert_street_enum_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[StreetEnumInsertRow],
) -> Result<()> {
    for chunk in rows.chunks(INSERT_CHUNK_SIZE) {
        let mut params: Vec<&(dyn ToSql + Sync)> =
            vec![&organization_id, &player_profile_id, &FEATURE_VERSION];
        let mut values = String::new();
        for (index, row) in chunk.iter().enumerate() {
            let base = 4 + index * 5;
            if !values.is_empty() {
                values.push_str(", ");
            }
            values.push_str(&format!(
                "($1, $2, ${}, ${}, ${}, ${}, $3, ${})",
                base,
                base + 1,
                base + 2,
                base + 3,
                base + 4
            ));
            params.push(&row.hand_id);
            params.push(&row.seat_no);
            params.push(&row.street);
            params.push(&row.feature_key);
            params.push(&row.value);
        }
        client.execute(
            &format!(
                "INSERT INTO analytics.player_street_enum_features (
                    organization_id,
                    player_profile_id,
                    hand_id,
                    seat_no,
                    street,
                    feature_key,
                    feature_version,
                    value
                )
                VALUES {values}"
            ),
            &params,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_feature_rows, build_street_feature_rows, summarize_street_feature_rows};
    use crate::{
        models::{
            HandFeatureFacts, MaterializedStreetFeatures, StreetFeatureFacts,
            StreetFeatureParticipant,
        },
        registry::{FeatureGrain, FtStageBucket, feature_registry},
    };
    use std::collections::{BTreeMap, BTreeSet};
    use uuid::Uuid;

    #[test]
    fn emits_dense_default_rows_for_non_ft_hand_without_ko() {
        let rows = build_feature_rows(&[HandFeatureFacts {
            hand_id: Uuid::nil(),
            tournament_id: Uuid::nil(),
            played_ft_hand: false,
            ft_table_size: None,
            is_boundary_hand: false,
            exact_ko_count: 0,
            split_ko_count: 0,
            sidepot_ko_count: 0,
            ko_attempt_count: 0,
            ko_opportunity_count: 0,
        }]);

        assert_eq!(rows.len(), 1);
        assert!(!rows[0].bool_values["played_ft_hand"]);
        assert!(!rows[0].bool_values["is_ft_hand"]);
        assert!(!rows[0].bool_values["is_boundary_hand"]);
        assert!(!rows[0].bool_values["has_exact_ko_event"]);
        assert_eq!(rows[0].num_values["hero_exact_ko_event_count"], Some(0.0));
        assert_eq!(rows[0].num_values["ft_players_remaining_exact"], None);
        assert_eq!(
            rows[0].enum_values["ft_stage_bucket"],
            FtStageBucket::NotFt.as_str()
        );
    }

    #[test]
    fn counts_exact_split_and_sidepot_ko_features() {
        let rows = build_feature_rows(&[HandFeatureFacts {
            hand_id: Uuid::nil(),
            tournament_id: Uuid::nil(),
            played_ft_hand: true,
            ft_table_size: Some(5),
            is_boundary_hand: false,
            exact_ko_count: 2,
            split_ko_count: 1,
            sidepot_ko_count: 1,
            ko_attempt_count: 0,
            ko_opportunity_count: 0,
        }]);

        assert!(rows[0].bool_values["played_ft_hand"]);
        assert!(rows[0].bool_values["is_ft_hand"]);
        assert!(rows[0].bool_values["has_exact_ko_event"]);
        assert!(rows[0].bool_values["has_split_ko_event"]);
        assert!(rows[0].bool_values["has_sidepot_ko_event"]);
        assert_eq!(rows[0].num_values["hero_exact_ko_event_count"], Some(2.0));
        assert_eq!(rows[0].num_values["hero_split_ko_event_count"], Some(1.0));
        assert_eq!(rows[0].num_values["hero_sidepot_ko_event_count"], Some(1.0));
        assert_eq!(
            rows[0].enum_values["ft_stage_bucket"],
            FtStageBucket::Ft56.as_str()
        );
    }

    #[test]
    fn build_feature_rows_emits_formal_stage_predicates_from_exact_ft_counts() {
        let rows = build_feature_rows(&[HandFeatureFacts {
            hand_id: Uuid::nil(),
            tournament_id: Uuid::nil(),
            played_ft_hand: true,
            ft_table_size: Some(5),
            is_boundary_hand: false,
            exact_ko_count: 0,
            split_ko_count: 0,
            sidepot_ko_count: 0,
            ko_attempt_count: 0,
            ko_opportunity_count: 0,
        }]);

        assert!(rows[0].bool_values["is_ft_hand"]);
        assert!(!rows[0].bool_values["is_stage_2"]);
        assert!(!rows[0].bool_values["is_stage_3_4"]);
        assert!(rows[0].bool_values["is_stage_4_5"]);
        assert!(rows[0].bool_values["is_stage_5_6"]);
        assert!(!rows[0].bool_values["is_stage_6_9"]);
        assert_eq!(rows[0].num_values["ft_players_remaining_exact"], Some(5.0));
    }

    #[test]
    fn build_feature_rows_only_populates_hand_grain_registry_keys() {
        let rows = build_feature_rows(&[HandFeatureFacts {
            hand_id: Uuid::nil(),
            tournament_id: Uuid::nil(),
            played_ft_hand: true,
            ft_table_size: Some(9),
            is_boundary_hand: false,
            exact_ko_count: 1,
            split_ko_count: 0,
            sidepot_ko_count: 0,
            ko_attempt_count: 2,
            ko_opportunity_count: 3,
        }]);

        let hand_bool_keys = feature_registry()
            .iter()
            .filter(|feature| feature.grain == FeatureGrain::Hand)
            .filter(|feature| {
                matches!(
                    feature.table_family,
                    crate::registry::FeatureTableFamily::Bool
                )
            })
            .map(|feature| feature.key)
            .collect::<Vec<_>>();
        let hand_num_keys = feature_registry()
            .iter()
            .filter(|feature| feature.grain == FeatureGrain::Hand)
            .filter(|feature| {
                matches!(
                    feature.table_family,
                    crate::registry::FeatureTableFamily::Num
                )
            })
            .map(|feature| feature.key)
            .collect::<Vec<_>>();
        let hand_enum_keys = feature_registry()
            .iter()
            .filter(|feature| feature.grain == FeatureGrain::Hand)
            .filter(|feature| {
                matches!(
                    feature.table_family,
                    crate::registry::FeatureTableFamily::Enum
                )
            })
            .map(|feature| feature.key)
            .collect::<Vec<_>>();

        let mut actual_bool_keys = rows[0]
            .bool_values
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        actual_bool_keys.sort_unstable();
        let mut expected_bool_keys = hand_bool_keys;
        expected_bool_keys.sort_unstable();
        assert_eq!(actual_bool_keys, expected_bool_keys);

        let mut actual_num_keys = rows[0]
            .num_values
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        actual_num_keys.sort_unstable();
        let mut expected_num_keys = hand_num_keys;
        expected_num_keys.sort_unstable();
        assert_eq!(actual_num_keys, expected_num_keys);

        let mut actual_enum_keys = rows[0]
            .enum_values
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        actual_enum_keys.sort_unstable();
        let mut expected_enum_keys = hand_enum_keys;
        expected_enum_keys.sort_unstable();
        assert_eq!(actual_enum_keys, expected_enum_keys);
    }

    #[test]
    fn emits_attempt_and_opportunity_hand_features_for_runtime_contract_v2() {
        let rows = build_feature_rows(&[HandFeatureFacts {
            hand_id: Uuid::nil(),
            tournament_id: Uuid::nil(),
            played_ft_hand: true,
            ft_table_size: Some(7),
            is_boundary_hand: true,
            exact_ko_count: 1,
            split_ko_count: 0,
            sidepot_ko_count: 0,
            ko_attempt_count: 2,
            ko_opportunity_count: 3,
        }]);

        assert!(rows[0].bool_values["has_ko_attempt"]);
        assert!(rows[0].bool_values["has_ko_opportunity"]);
        assert_eq!(rows[0].num_values["hero_ko_attempt_count"], Some(2.0));
        assert_eq!(rows[0].num_values["hero_ko_opportunity_count"], Some(3.0));
    }

    #[test]
    fn emits_street_rows_only_for_hero_and_showdown_known_opponents() {
        let rows = build_street_feature_rows(&[
            StreetFeatureFacts {
                hand_id: Uuid::nil(),
                seat_no: 7,
                street: "flop".to_string(),
                participant: StreetFeatureParticipant::Hero,
                starter_hand_class: None,
                best_hand_class: Some("pair".to_string()),
                best_hand_rank_value: Some(1),
                made_hand_category: Some("overpair".to_string()),
                draw_category: Some("none".to_string()),
                overcards_count: Some(0),
                has_air: Some(false),
                missed_flush_draw: Some(false),
                missed_straight_draw: Some(false),
                certainty_state: "exact".to_string(),
            },
            StreetFeatureFacts {
                hand_id: Uuid::nil(),
                seat_no: 3,
                street: "flop".to_string(),
                participant: StreetFeatureParticipant::ShowdownKnownOpponent,
                starter_hand_class: None,
                best_hand_class: Some("two_pair".to_string()),
                best_hand_rank_value: Some(2),
                made_hand_category: Some("two_pair".to_string()),
                draw_category: Some("none".to_string()),
                overcards_count: Some(0),
                has_air: Some(false),
                missed_flush_draw: Some(false),
                missed_straight_draw: Some(false),
                certainty_state: "exact".to_string(),
            },
            StreetFeatureFacts {
                hand_id: Uuid::nil(),
                seat_no: 5,
                street: "flop".to_string(),
                participant: StreetFeatureParticipant::UnknownOpponent,
                starter_hand_class: None,
                best_hand_class: Some("high_card".to_string()),
                best_hand_rank_value: None,
                made_hand_category: Some("none".to_string()),
                draw_category: Some("none".to_string()),
                overcards_count: Some(2),
                has_air: Some(true),
                missed_flush_draw: Some(false),
                missed_straight_draw: Some(false),
                certainty_state: "exact".to_string(),
            },
        ]);

        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows.iter().map(|row| row.seat_no).collect::<BTreeSet<_>>(),
            BTreeSet::from([3_i32, 7_i32])
        );
    }

    #[test]
    fn maps_street_exact_values_into_bool_num_and_enum_families() {
        let rows = build_street_feature_rows(&[StreetFeatureFacts {
            hand_id: Uuid::nil(),
            seat_no: 7,
            street: "turn".to_string(),
            participant: StreetFeatureParticipant::Hero,
            starter_hand_class: None,
            best_hand_class: Some("pair".to_string()),
            best_hand_rank_value: Some(1),
            made_hand_category: Some("top_pair".to_string()),
            draw_category: Some("flush_draw".to_string()),
            overcards_count: Some(1),
            has_air: Some(false),
            missed_flush_draw: Some(false),
            missed_straight_draw: Some(false),
            certainty_state: "exact".to_string(),
        }]);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].enum_values["best_hand_class"], "pair");
        assert_eq!(rows[0].enum_values["made_hand_category"], "top_pair");
        assert_eq!(rows[0].enum_values["draw_category"], "flush_draw");
        assert_eq!(rows[0].enum_values["certainty_state"], "exact");
        assert_eq!(rows[0].num_values["best_hand_rank_value"], Some(1.0));
        assert_eq!(rows[0].num_values["overcards_count"], Some(1.0));
        assert!(!rows[0].bool_values["has_air"]);
        assert!(!rows[0].bool_values["missed_flush_draw"]);
        assert!(!rows[0].bool_values["missed_straight_draw"]);
    }

    #[test]
    fn materializes_preflop_rows_as_enum_only_surface() {
        let rows = build_street_feature_rows(&[StreetFeatureFacts {
            hand_id: Uuid::nil(),
            seat_no: 7,
            street: "preflop".to_string(),
            participant: StreetFeatureParticipant::Hero,
            starter_hand_class: Some("AA".to_string()),
            best_hand_class: None,
            best_hand_rank_value: None,
            made_hand_category: None,
            draw_category: None,
            overcards_count: None,
            has_air: None,
            missed_flush_draw: None,
            missed_straight_draw: None,
            certainty_state: "exact".to_string(),
        }]);

        assert_eq!(rows.len(), 1);
        assert!(rows[0].bool_values.is_empty());
        assert!(rows[0].num_values.is_empty());
        assert_eq!(rows[0].enum_values["starter_hand_class"], "AA");
        assert_eq!(rows[0].enum_values["certainty_state"], "exact");
    }

    #[test]
    fn counts_actual_mixed_street_feature_rows_for_reporting() {
        let report = summarize_street_feature_rows(&[
            MaterializedStreetFeatures {
                hand_id: Uuid::nil(),
                seat_no: 7,
                street: "preflop".to_string(),
                bool_values: BTreeMap::new(),
                num_values: BTreeMap::new(),
                enum_values: BTreeMap::from([
                    ("starter_hand_class".to_string(), "AA".to_string()),
                    ("certainty_state".to_string(), "exact".to_string()),
                ]),
            },
            MaterializedStreetFeatures {
                hand_id: Uuid::nil(),
                seat_no: 7,
                street: "flop".to_string(),
                bool_values: BTreeMap::from([
                    ("has_air".to_string(), false),
                    ("missed_flush_draw".to_string(), false),
                    ("missed_straight_draw".to_string(), false),
                ]),
                num_values: BTreeMap::from([
                    ("best_hand_rank_value".to_string(), Some(1.0)),
                    ("overcards_count".to_string(), Some(0.0)),
                ]),
                enum_values: BTreeMap::from([
                    ("best_hand_class".to_string(), "pair".to_string()),
                    ("made_hand_category".to_string(), "overpair".to_string()),
                    ("draw_category".to_string(), "none".to_string()),
                    ("certainty_state".to_string(), "exact".to_string()),
                ]),
            },
        ]);

        assert_eq!(report.street_row_count, 2);
        assert_eq!(report.street_bool_rows, 3);
        assert_eq!(report.street_num_rows, 2);
        assert_eq!(report.street_enum_rows, 6);
    }
}
