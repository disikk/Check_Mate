use std::collections::BTreeMap;

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandFeatureFacts {
    pub hand_id: Uuid,
    pub tournament_id: Uuid,
    pub played_ft_hand: bool,
    pub ft_table_size: Option<i32>,
    pub exact_ko_count: u32,
    pub split_ko_count: u32,
    pub sidepot_ko_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedHandFeatures {
    pub hand_id: Uuid,
    pub tournament_id: Uuid,
    pub bool_values: BTreeMap<String, bool>,
    pub num_values: BTreeMap<String, f64>,
    pub enum_values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializationReport {
    pub hand_count: u64,
    pub bool_rows: u64,
    pub num_rows: u64,
    pub enum_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedStatsFilters {
    pub organization_id: Uuid,
    pub player_profile_id: Uuid,
    pub buyin_total_cents: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedStatCoverage {
    pub summary_tournament_count: u64,
    pub hand_tournament_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeedStatSnapshot {
    pub coverage: SeedStatCoverage,
    pub roi_pct: Option<f64>,
    pub avg_finish_place: Option<f64>,
    pub final_table_reach_percent: Option<f64>,
    pub total_ko: u64,
    pub avg_ko_per_tournament: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::{HandFeatureFacts, SeedStatCoverage, SeedStatSnapshot};
    use uuid::Uuid;

    #[test]
    fn exposes_hand_feature_and_seed_stat_models() {
        let hand = HandFeatureFacts {
            hand_id: Uuid::nil(),
            tournament_id: Uuid::nil(),
            played_ft_hand: false,
            ft_table_size: None,
            exact_ko_count: 0,
            split_ko_count: 0,
            sidepot_ko_count: 0,
        };

        let snapshot = SeedStatSnapshot {
            coverage: SeedStatCoverage {
                summary_tournament_count: 2,
                hand_tournament_count: 1,
            },
            roi_pct: Some(12.5),
            avg_finish_place: Some(3.5),
            final_table_reach_percent: Some(100.0),
            total_ko: 2,
            avg_ko_per_tournament: Some(2.0),
        };

        assert_eq!(hand.exact_ko_count, 0);
        assert_eq!(snapshot.coverage.hand_tournament_count, 1);
    }
}
