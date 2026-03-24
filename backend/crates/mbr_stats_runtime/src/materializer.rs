use std::collections::BTreeMap;

use anyhow::Result;
use postgres::{GenericClient, Row};
use uuid::Uuid;

use crate::{
    models::{HandFeatureFacts, MaterializationReport, MaterializedHandFeatures},
    registry::{FEATURE_VERSION, feature_registry, ft_stage_bucket},
};

pub fn materialize_player_hand_features(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<MaterializationReport> {
    delete_existing_feature_rows(client, organization_id, player_profile_id)?;
    let facts = load_hand_feature_facts(client, organization_id, player_profile_id)?;
    let rows = build_feature_rows(&facts);
    persist_feature_rows(client, organization_id, player_profile_id, &rows)?;

    Ok(MaterializationReport {
        hand_count: rows.len() as u64,
        bool_rows: (rows.len() * 4) as u64,
        num_rows: (rows.len() * 4) as u64,
        enum_rows: rows.len() as u64,
    })
}

fn delete_existing_feature_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<()> {
    client.execute(
        "DELETE FROM analytics.player_hand_bool_features
         WHERE organization_id = $1
           AND player_profile_id = $2
           AND feature_version = $3",
        &[&organization_id, &player_profile_id, &FEATURE_VERSION],
    )?;
    client.execute(
        "DELETE FROM analytics.player_hand_num_features
         WHERE organization_id = $1
           AND player_profile_id = $2
           AND feature_version = $3",
        &[&organization_id, &player_profile_id, &FEATURE_VERSION],
    )?;
    client.execute(
        "DELETE FROM analytics.player_hand_enum_features
         WHERE organization_id = $1
           AND player_profile_id = $2
           AND feature_version = $3",
        &[&organization_id, &player_profile_id, &FEATURE_VERSION],
    )?;
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
            msr.ft_table_size
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
        exact_ko_count: row.get::<_, i64>(4) as u32,
        split_ko_count: row.get::<_, i64>(5) as u32,
        sidepot_ko_count: row.get::<_, i64>(6) as u32,
    }
}

pub(crate) fn build_feature_rows(facts: &[HandFeatureFacts]) -> Vec<MaterializedHandFeatures> {
    facts
        .iter()
        .map(|fact| {
            let mut bool_values = BTreeMap::new();
            bool_values.insert("played_ft_hand".to_string(), fact.played_ft_hand);
            bool_values.insert("has_exact_ko".to_string(), fact.exact_ko_count > 0);
            bool_values.insert("has_split_ko".to_string(), fact.split_ko_count > 0);
            bool_values.insert("has_sidepot_ko".to_string(), fact.sidepot_ko_count > 0);

            let mut num_values = BTreeMap::new();
            num_values.insert(
                "ft_table_size".to_string(),
                fact.ft_table_size.unwrap_or_default() as f64,
            );
            num_values.insert(
                "hero_exact_ko_count".to_string(),
                fact.exact_ko_count as f64,
            );
            num_values.insert(
                "hero_split_ko_count".to_string(),
                fact.split_ko_count as f64,
            );
            num_values.insert(
                "hero_sidepot_ko_count".to_string(),
                fact.sidepot_ko_count as f64,
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

fn persist_feature_rows(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    rows: &[MaterializedHandFeatures],
) -> Result<()> {
    for row in rows {
        for feature in feature_registry() {
            match feature.table_family {
                crate::registry::FeatureTableFamily::Bool => {
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
                crate::registry::FeatureTableFamily::Num => {
                    let value = row.num_values[feature.key];
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
                crate::registry::FeatureTableFamily::Enum => {
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

#[cfg(test)]
mod tests {
    use super::build_feature_rows;
    use crate::{models::HandFeatureFacts, registry::FtStageBucket};
    use uuid::Uuid;

    #[test]
    fn emits_dense_default_rows_for_non_ft_hand_without_ko() {
        let rows = build_feature_rows(&[HandFeatureFacts {
            hand_id: Uuid::nil(),
            tournament_id: Uuid::nil(),
            played_ft_hand: false,
            ft_table_size: None,
            exact_ko_count: 0,
            split_ko_count: 0,
            sidepot_ko_count: 0,
        }]);

        assert_eq!(rows.len(), 1);
        assert!(!rows[0].bool_values["played_ft_hand"]);
        assert!(!rows[0].bool_values["has_exact_ko"]);
        assert_eq!(rows[0].num_values["hero_exact_ko_count"], 0.0);
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
            exact_ko_count: 2,
            split_ko_count: 1,
            sidepot_ko_count: 1,
        }]);

        assert!(rows[0].bool_values["played_ft_hand"]);
        assert!(rows[0].bool_values["has_exact_ko"]);
        assert!(rows[0].bool_values["has_split_ko"]);
        assert!(rows[0].bool_values["has_sidepot_ko"]);
        assert_eq!(rows[0].num_values["hero_exact_ko_count"], 2.0);
        assert_eq!(rows[0].num_values["hero_split_ko_count"], 1.0);
        assert_eq!(rows[0].num_values["hero_sidepot_ko_count"], 1.0);
        assert_eq!(
            rows[0].enum_values["ft_stage_bucket"],
            FtStageBucket::Ft56.as_str()
        );
    }
}
