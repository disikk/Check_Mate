use std::collections::BTreeMap;

use anyhow::Result;
use postgres::{GenericClient, Row};
use uuid::Uuid;

use crate::{
    models::{
        HandFeatureFacts, MaterializationReport, MaterializedHandFeatures,
        MaterializedStreetFeatures, StreetFeatureFacts, StreetFeatureParticipant,
    },
    registry::{
        FEATURE_VERSION, GG_MBR_FT_MAX_PLAYERS, FeatureGrain, FeatureTableFamily, feature_registry,
        ft_stage_bucket,
    },
};

pub fn materialize_player_hand_features(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<MaterializationReport> {
    delete_existing_feature_rows(client, organization_id, player_profile_id)?;

    let hand_facts = load_hand_feature_facts(client, organization_id, player_profile_id)?;
    let hand_rows = build_feature_rows(&hand_facts);
    persist_feature_rows(client, organization_id, player_profile_id, &hand_rows)?;

    let street_facts = load_street_feature_facts(client, organization_id, player_profile_id)?;
    let street_rows = build_street_feature_rows(&street_facts);
    persist_street_feature_rows(client, organization_id, player_profile_id, &street_rows)?;

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
        street_row_count: street_rows.len() as u64,
        street_bool_rows: (street_rows.len() * 3) as u64,
        street_num_rows: street_rows
            .iter()
            .flat_map(|row| row.num_values.values())
            .filter(|value| value.is_some())
            .count() as u64,
        street_enum_rows: (street_rows.len() * 4) as u64,
    })
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

fn load_hand_feature_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<HandFeatureFacts>> {
    let rows = client.query(
        "SELECT
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
                    WHEN he.hero_involved IS TRUE
                     AND he.certainty_state = 'exact'
                    THEN 1
                    ELSE 0
                END
            ), 0)::bigint AS exact_ko_count,
            COALESCE(SUM(
                CASE
                    WHEN he.hero_involved IS TRUE
                     AND he.certainty_state = 'exact'
                     AND he.is_split_ko IS TRUE
                    THEN 1
                    ELSE 0
                END
            ), 0)::bigint AS split_ko_count,
            COALESCE(SUM(
                CASE
                    WHEN he.hero_involved IS TRUE
                     AND he.certainty_state = 'exact'
                     AND he.is_sidepot_based IS TRUE
                    THEN 1
                    ELSE 0
                END
            ), 0)::bigint AS sidepot_ko_count
         FROM core.hands h
         LEFT JOIN derived.mbr_stage_resolution msr
           ON msr.hand_id = h.id
          AND msr.player_profile_id = h.player_profile_id
         LEFT JOIN derived.hand_eliminations he
           ON he.hand_id = h.id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2
         GROUP BY
            h.id,
            h.tournament_id,
            msr.played_ft_hand,
            msr.played_ft_hand_state,
            msr.ft_table_size,
            msr.is_boundary_hand
         ORDER BY h.id",
        &[&organization_id, &player_profile_id],
    )?;

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
    }
}

fn load_street_feature_facts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<StreetFeatureFacts>> {
    let rows = client.query(
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
           AND h.player_profile_id = $2
         ORDER BY shs.hand_id, shs.seat_no, shs.street",
        &[&organization_id, &player_profile_id],
    )?;

    Ok(rows.into_iter().map(row_to_street_feature_facts).collect())
}

fn row_to_street_feature_facts(row: Row) -> StreetFeatureFacts {
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
        best_hand_class: row.get(5),
        best_hand_rank_value: row.get(6),
        made_hand_category: row.get(7),
        draw_category: row.get(8),
        overcards_count: row.get(9),
        has_air: row.get(10),
        missed_flush_draw: row.get(11),
        missed_straight_draw: row.get(12),
        certainty_state: row.get(13),
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
            bool_values.insert("has_air".to_string(), fact.has_air);
            bool_values.insert("missed_flush_draw".to_string(), fact.missed_flush_draw);
            bool_values.insert(
                "missed_straight_draw".to_string(),
                fact.missed_straight_draw,
            );

            let mut num_values = BTreeMap::new();
            num_values.insert(
                "best_hand_rank_value".to_string(),
                fact.best_hand_rank_value.map(|value| value as f64),
            );
            num_values.insert(
                "overcards_count".to_string(),
                Some(fact.overcards_count as f64),
            );

            let mut enum_values = BTreeMap::new();
            enum_values.insert("best_hand_class".to_string(), fact.best_hand_class.clone());
            enum_values.insert(
                "made_hand_category".to_string(),
                fact.made_hand_category.clone(),
            );
            enum_values.insert("draw_category".to_string(), fact.draw_category.clone());
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

fn persist_feature_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[MaterializedHandFeatures],
) -> Result<()> {
    for row in rows {
        for feature in feature_registry() {
            if feature.grain != FeatureGrain::Hand {
                continue;
            }

            match feature.table_family {
                FeatureTableFamily::Bool => {
                    let value = row.bool_values[feature.key];
                    client.execute(
                        "INSERT INTO analytics.player_hand_bool_features (
                            organization_id,
                            player_profile_id,
                            hand_id,
                            feature_key,
                            feature_version,
                            value
                        )
                        VALUES ($1, $2, $3, $4, $5, $6)",
                        &[
                            &organization_id,
                            &player_profile_id,
                            &row.hand_id,
                            &feature.key,
                            &FEATURE_VERSION,
                            &value,
                        ],
                    )?;
                }
                FeatureTableFamily::Num => {
                    let Some(value) = row.num_values[feature.key] else {
                        continue;
                    };
                    client.execute(
                        "INSERT INTO analytics.player_hand_num_features (
                            organization_id,
                            player_profile_id,
                            hand_id,
                            feature_key,
                            feature_version,
                            value
                        )
                        VALUES ($1, $2, $3, $4, $5, ($6::double precision)::numeric(18,6))",
                        &[
                            &organization_id,
                            &player_profile_id,
                            &row.hand_id,
                            &feature.key,
                            &FEATURE_VERSION,
                            &value,
                        ],
                    )?;
                }
                FeatureTableFamily::Enum => {
                    let value = &row.enum_values[feature.key];
                    client.execute(
                        "INSERT INTO analytics.player_hand_enum_features (
                            organization_id,
                            player_profile_id,
                            hand_id,
                            feature_key,
                            feature_version,
                            value
                        )
                        VALUES ($1, $2, $3, $4, $5, $6)",
                        &[
                            &organization_id,
                            &player_profile_id,
                            &row.hand_id,
                            &feature.key,
                            &FEATURE_VERSION,
                            value,
                        ],
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn persist_street_feature_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[MaterializedStreetFeatures],
) -> Result<()> {
    for row in rows {
        for feature in feature_registry() {
            if feature.grain != FeatureGrain::Street {
                continue;
            }

            match feature.table_family {
                FeatureTableFamily::Bool => {
                    let value = row.bool_values[feature.key];
                    client.execute(
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
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                        &[
                            &organization_id,
                            &player_profile_id,
                            &row.hand_id,
                            &row.seat_no,
                            &row.street,
                            &feature.key,
                            &FEATURE_VERSION,
                            &value,
                        ],
                    )?;
                }
                FeatureTableFamily::Num => {
                    let Some(value) = row.num_values[feature.key] else {
                        continue;
                    };
                    client.execute(
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
                        VALUES ($1, $2, $3, $4, $5, $6, $7, ($8::double precision)::numeric(18,6))",
                        &[
                            &organization_id,
                            &player_profile_id,
                            &row.hand_id,
                            &row.seat_no,
                            &row.street,
                            &feature.key,
                            &FEATURE_VERSION,
                            &value,
                        ],
                    )?;
                }
                FeatureTableFamily::Enum => {
                    let value = &row.enum_values[feature.key];
                    client.execute(
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
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                        &[
                            &organization_id,
                            &player_profile_id,
                            &row.hand_id,
                            &row.seat_no,
                            &row.street,
                            &feature.key,
                            &FEATURE_VERSION,
                            value,
                        ],
                    )?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_feature_rows, build_street_feature_rows};
    use crate::{
        models::{HandFeatureFacts, StreetFeatureFacts, StreetFeatureParticipant},
        registry::{FeatureGrain, FtStageBucket, feature_registry},
    };
    use std::collections::BTreeSet;
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
    fn emits_street_rows_only_for_hero_and_showdown_known_opponents() {
        let rows = build_street_feature_rows(&[
            StreetFeatureFacts {
                hand_id: Uuid::nil(),
                seat_no: 7,
                street: "flop".to_string(),
                participant: StreetFeatureParticipant::Hero,
                best_hand_class: "pair".to_string(),
                best_hand_rank_value: Some(1),
                made_hand_category: "overpair".to_string(),
                draw_category: "none".to_string(),
                overcards_count: 0,
                has_air: false,
                missed_flush_draw: false,
                missed_straight_draw: false,
                certainty_state: "exact".to_string(),
            },
            StreetFeatureFacts {
                hand_id: Uuid::nil(),
                seat_no: 3,
                street: "flop".to_string(),
                participant: StreetFeatureParticipant::ShowdownKnownOpponent,
                best_hand_class: "two_pair".to_string(),
                best_hand_rank_value: Some(2),
                made_hand_category: "two_pair".to_string(),
                draw_category: "none".to_string(),
                overcards_count: 0,
                has_air: false,
                missed_flush_draw: false,
                missed_straight_draw: false,
                certainty_state: "exact".to_string(),
            },
            StreetFeatureFacts {
                hand_id: Uuid::nil(),
                seat_no: 5,
                street: "flop".to_string(),
                participant: StreetFeatureParticipant::UnknownOpponent,
                best_hand_class: "high_card".to_string(),
                best_hand_rank_value: None,
                made_hand_category: "none".to_string(),
                draw_category: "none".to_string(),
                overcards_count: 2,
                has_air: true,
                missed_flush_draw: false,
                missed_straight_draw: false,
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
            best_hand_class: "pair".to_string(),
            best_hand_rank_value: Some(1),
            made_hand_category: "top_pair".to_string(),
            draw_category: "flush_draw".to_string(),
            overcards_count: 1,
            has_air: false,
            missed_flush_draw: false,
            missed_straight_draw: false,
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
}
