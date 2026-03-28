pub const FEATURE_VERSION: &str = "mbr_runtime_v2";

/// GG MBR: финальный стол начинается, когда max_players == 9.
/// Единая константа для FT detection — заменяет разбросанные по коду магические `9`.
pub const GG_MBR_FT_MAX_PLAYERS: i32 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureTableFamily {
    Bool,
    Num,
    Enum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureGrain {
    Hand,
    Street,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtStageBucket {
    NotFt,
    Ft79,
    Ft56,
    Ft34,
    Ft23,
}

impl FtStageBucket {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFt => "not_ft",
            Self::Ft79 => "ft_7_9",
            Self::Ft56 => "ft_5_6",
            Self::Ft34 => "ft_3_4",
            Self::Ft23 => "ft_2_3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureSpec {
    pub key: &'static str,
    pub table_family: FeatureTableFamily,
    pub grain: FeatureGrain,
}

const FEATURE_REGISTRY: [FeatureSpec; 31] = [
    FeatureSpec {
        key: "played_ft_hand",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "is_ft_hand",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "is_stage_2",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "is_stage_3_4",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "is_stage_4_5",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "is_stage_5_6",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "is_stage_6_9",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "is_boundary_hand",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "has_exact_ko_event",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "has_split_ko_event",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "has_sidepot_ko_event",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "has_ko_attempt",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "has_ko_opportunity",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "ft_table_size",
        table_family: FeatureTableFamily::Num,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "ft_players_remaining_exact",
        table_family: FeatureTableFamily::Num,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "hero_exact_ko_event_count",
        table_family: FeatureTableFamily::Num,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "hero_split_ko_event_count",
        table_family: FeatureTableFamily::Num,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "hero_sidepot_ko_event_count",
        table_family: FeatureTableFamily::Num,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "hero_ko_attempt_count",
        table_family: FeatureTableFamily::Num,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "hero_ko_opportunity_count",
        table_family: FeatureTableFamily::Num,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "ft_stage_bucket",
        table_family: FeatureTableFamily::Enum,
        grain: FeatureGrain::Hand,
    },
    FeatureSpec {
        key: "starter_hand_class",
        table_family: FeatureTableFamily::Enum,
        grain: FeatureGrain::Street,
    },
    FeatureSpec {
        key: "best_hand_class",
        table_family: FeatureTableFamily::Enum,
        grain: FeatureGrain::Street,
    },
    FeatureSpec {
        key: "best_hand_rank_value",
        table_family: FeatureTableFamily::Num,
        grain: FeatureGrain::Street,
    },
    FeatureSpec {
        key: "made_hand_category",
        table_family: FeatureTableFamily::Enum,
        grain: FeatureGrain::Street,
    },
    FeatureSpec {
        key: "draw_category",
        table_family: FeatureTableFamily::Enum,
        grain: FeatureGrain::Street,
    },
    FeatureSpec {
        key: "overcards_count",
        table_family: FeatureTableFamily::Num,
        grain: FeatureGrain::Street,
    },
    FeatureSpec {
        key: "has_air",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Street,
    },
    FeatureSpec {
        key: "missed_flush_draw",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Street,
    },
    FeatureSpec {
        key: "missed_straight_draw",
        table_family: FeatureTableFamily::Bool,
        grain: FeatureGrain::Street,
    },
    FeatureSpec {
        key: "certainty_state",
        table_family: FeatureTableFamily::Enum,
        grain: FeatureGrain::Street,
    },
];

pub fn feature_registry() -> &'static [FeatureSpec] {
    &FEATURE_REGISTRY
}

pub fn ft_stage_bucket(played_ft_hand: bool, ft_table_size: Option<i32>) -> FtStageBucket {
    if !played_ft_hand {
        return FtStageBucket::NotFt;
    }

    match ft_table_size {
        Some(7..=GG_MBR_FT_MAX_PLAYERS) => FtStageBucket::Ft79,
        Some(5..=6) => FtStageBucket::Ft56,
        Some(3..=4) => FtStageBucket::Ft34,
        Some(2) => FtStageBucket::Ft23,
        _ => FtStageBucket::NotFt,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FEATURE_VERSION, FeatureGrain, FeatureTableFamily, FtStageBucket, GG_MBR_FT_MAX_PLAYERS,
        feature_registry, ft_stage_bucket,
    };

    #[test]
    fn freezes_feature_version_and_registry_mapping() {
        let registry = feature_registry();
        assert_eq!(FEATURE_VERSION, "mbr_runtime_v2");
        assert_eq!(registry.len(), 31);

        let hand_bool_keys = registry
            .iter()
            .filter(|feature| {
                feature.grain == FeatureGrain::Hand
                    && feature.table_family == FeatureTableFamily::Bool
            })
            .map(|feature| feature.key)
            .collect::<Vec<_>>();
        assert_eq!(
            hand_bool_keys,
            vec![
                "played_ft_hand",
                "is_ft_hand",
                "is_stage_2",
                "is_stage_3_4",
                "is_stage_4_5",
                "is_stage_5_6",
                "is_stage_6_9",
                "is_boundary_hand",
                "has_exact_ko_event",
                "has_split_ko_event",
                "has_sidepot_ko_event",
                "has_ko_attempt",
                "has_ko_opportunity",
            ]
        );

        let hand_num_keys = registry
            .iter()
            .filter(|feature| {
                feature.grain == FeatureGrain::Hand
                    && feature.table_family == FeatureTableFamily::Num
            })
            .map(|feature| feature.key)
            .collect::<Vec<_>>();
        assert_eq!(
            hand_num_keys,
            vec![
                "ft_table_size",
                "ft_players_remaining_exact",
                "hero_exact_ko_event_count",
                "hero_split_ko_event_count",
                "hero_sidepot_ko_event_count",
                "hero_ko_attempt_count",
                "hero_ko_opportunity_count",
            ]
        );

        let hand_enum_keys = registry
            .iter()
            .filter(|feature| {
                feature.grain == FeatureGrain::Hand
                    && feature.table_family == FeatureTableFamily::Enum
            })
            .map(|feature| feature.key)
            .collect::<Vec<_>>();
        assert_eq!(hand_enum_keys, vec!["ft_stage_bucket"]);

        let street_keys = registry
            .iter()
            .filter(|feature| feature.grain == FeatureGrain::Street)
            .map(|feature| feature.key)
            .collect::<Vec<_>>();
        assert_eq!(
            street_keys,
            vec![
                "starter_hand_class",
                "best_hand_class",
                "best_hand_rank_value",
                "made_hand_category",
                "draw_category",
                "overcards_count",
                "has_air",
                "missed_flush_draw",
                "missed_straight_draw",
                "certainty_state",
            ]
        );
    }

    #[test]
    fn maps_ft_stage_buckets_from_exact_table_sizes() {
        assert_eq!(ft_stage_bucket(false, None), FtStageBucket::NotFt);
        assert_eq!(ft_stage_bucket(true, Some(9)), FtStageBucket::Ft79);
        assert_eq!(ft_stage_bucket(true, Some(6)), FtStageBucket::Ft56);
        assert_eq!(ft_stage_bucket(true, Some(3)), FtStageBucket::Ft34);
        assert_eq!(ft_stage_bucket(true, Some(2)), FtStageBucket::Ft23);
    }

    // --- F3-T1: Synthetic edge-case tests for FT detection and stage buckets ---

    #[test]
    fn ft_stage_bucket_covers_all_seat_count_boundaries() {
        // Exhaustive boundary test: every seat count from 1 to 10
        assert_eq!(ft_stage_bucket(true, Some(1)), FtStageBucket::NotFt);
        assert_eq!(ft_stage_bucket(true, Some(2)), FtStageBucket::Ft23);
        assert_eq!(ft_stage_bucket(true, Some(3)), FtStageBucket::Ft34);
        assert_eq!(ft_stage_bucket(true, Some(4)), FtStageBucket::Ft34);
        assert_eq!(ft_stage_bucket(true, Some(5)), FtStageBucket::Ft56);
        assert_eq!(ft_stage_bucket(true, Some(6)), FtStageBucket::Ft56);
        assert_eq!(ft_stage_bucket(true, Some(7)), FtStageBucket::Ft79);
        assert_eq!(ft_stage_bucket(true, Some(8)), FtStageBucket::Ft79);
        assert_eq!(
            ft_stage_bucket(true, Some(GG_MBR_FT_MAX_PLAYERS)),
            FtStageBucket::Ft79
        );
        assert_eq!(ft_stage_bucket(true, Some(10)), FtStageBucket::NotFt);
    }

    #[test]
    fn ft_stage_bucket_not_ft_when_played_ft_hand_is_false_regardless_of_table_size() {
        // Even if table_size is 9, played_ft_hand=false means NotFt
        assert_eq!(ft_stage_bucket(false, Some(9)), FtStageBucket::NotFt);
        assert_eq!(ft_stage_bucket(false, Some(2)), FtStageBucket::NotFt);
        assert_eq!(ft_stage_bucket(false, Some(5)), FtStageBucket::NotFt);
    }

    #[test]
    fn ft_stage_bucket_not_ft_when_table_size_is_none() {
        assert_eq!(ft_stage_bucket(true, None), FtStageBucket::NotFt);
    }

    #[test]
    fn ft_stage_bucket_as_str_roundtrips_all_variants() {
        let variants = [
            FtStageBucket::NotFt,
            FtStageBucket::Ft79,
            FtStageBucket::Ft56,
            FtStageBucket::Ft34,
            FtStageBucket::Ft23,
        ];
        let expected_strs = ["not_ft", "ft_7_9", "ft_5_6", "ft_3_4", "ft_2_3"];
        for (variant, expected) in variants.iter().zip(expected_strs.iter()) {
            assert_eq!(variant.as_str(), *expected);
        }
    }

    #[test]
    fn gg_mbr_ft_max_players_is_nine() {
        // Freeze the FT detection constant — changing this should be an explicit decision
        assert_eq!(GG_MBR_FT_MAX_PLAYERS, 9);
    }
}
