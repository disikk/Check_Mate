pub const FEATURE_VERSION: &str = "mbr_runtime_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureTableFamily {
    Bool,
    Num,
    Enum,
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
}

const FEATURE_REGISTRY: [FeatureSpec; 9] = [
    FeatureSpec {
        key: "played_ft_hand",
        table_family: FeatureTableFamily::Bool,
    },
    FeatureSpec {
        key: "has_exact_ko",
        table_family: FeatureTableFamily::Bool,
    },
    FeatureSpec {
        key: "has_split_ko",
        table_family: FeatureTableFamily::Bool,
    },
    FeatureSpec {
        key: "has_sidepot_ko",
        table_family: FeatureTableFamily::Bool,
    },
    FeatureSpec {
        key: "ft_table_size",
        table_family: FeatureTableFamily::Num,
    },
    FeatureSpec {
        key: "hero_exact_ko_count",
        table_family: FeatureTableFamily::Num,
    },
    FeatureSpec {
        key: "hero_split_ko_count",
        table_family: FeatureTableFamily::Num,
    },
    FeatureSpec {
        key: "hero_sidepot_ko_count",
        table_family: FeatureTableFamily::Num,
    },
    FeatureSpec {
        key: "ft_stage_bucket",
        table_family: FeatureTableFamily::Enum,
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
        FEATURE_VERSION, FeatureTableFamily, FtStageBucket, feature_registry, ft_stage_bucket,
    };

    #[test]
    fn freezes_feature_version_and_registry_mapping() {
        let registry = feature_registry();
        assert_eq!(FEATURE_VERSION, "mbr_runtime_v1");
        assert_eq!(registry.len(), 9);
        assert_eq!(registry[0].key, "played_ft_hand");
        assert_eq!(registry[0].table_family, FeatureTableFamily::Bool);
        assert_eq!(registry[4].key, "ft_table_size");
        assert_eq!(registry[4].table_family, FeatureTableFamily::Num);
        assert_eq!(registry[8].key, "ft_stage_bucket");
        assert_eq!(registry[8].table_family, FeatureTableFamily::Enum);
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
