pub const FEATURE_VERSION: &str = "mbr_runtime_v1";

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

const FEATURE_REGISTRY: [FeatureSpec; 26] = [
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
        key: "ft_stage_bucket",
        table_family: FeatureTableFamily::Enum,
        grain: FeatureGrain::Hand,
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
        Some(7..=9) => FtStageBucket::Ft79,
        Some(5..=6) => FtStageBucket::Ft56,
        Some(3..=4) => FtStageBucket::Ft34,
        Some(2) => FtStageBucket::Ft23,
        _ => FtStageBucket::NotFt,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FEATURE_VERSION, FeatureGrain, FeatureTableFamily, FtStageBucket, feature_registry,
        ft_stage_bucket,
    };

    #[test]
    fn freezes_feature_version_and_registry_mapping() {
        let registry = feature_registry();
        assert_eq!(FEATURE_VERSION, "mbr_runtime_v1");
        assert_eq!(registry.len(), 26);

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
}
