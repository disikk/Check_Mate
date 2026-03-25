use std::collections::BTreeMap;

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandFeatureFacts {
    pub hand_id: Uuid,
    pub tournament_id: Uuid,
    pub played_ft_hand: bool,
    pub ft_table_size: Option<i32>,
    pub is_boundary_hand: bool,
    pub exact_ko_count: u32,
    pub split_ko_count: u32,
    pub sidepot_ko_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedHandFeatures {
    pub hand_id: Uuid,
    pub tournament_id: Uuid,
    pub bool_values: BTreeMap<String, bool>,
    pub num_values: BTreeMap<String, Option<f64>>,
    pub enum_values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreetFeatureParticipant {
    Hero,
    ShowdownKnownOpponent,
    UnknownOpponent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreetFeatureFacts {
    pub hand_id: Uuid,
    pub seat_no: i32,
    pub street: String,
    pub participant: StreetFeatureParticipant,
    pub best_hand_class: String,
    pub best_hand_rank_value: Option<i64>,
    pub made_hand_category: String,
    pub draw_category: String,
    pub overcards_count: i32,
    pub has_air: bool,
    pub missed_flush_draw: bool,
    pub missed_straight_draw: bool,
    pub certainty_state: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedStreetFeatures {
    pub hand_id: Uuid,
    pub seat_no: i32,
    pub street: String,
    pub bool_values: BTreeMap<String, bool>,
    pub num_values: BTreeMap<String, Option<f64>>,
    pub enum_values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializationReport {
    pub hand_count: u64,
    pub bool_rows: u64,
    pub num_rows: u64,
    pub enum_rows: u64,
    pub street_row_count: u64,
    pub street_bool_rows: u64,
    pub street_num_rows: u64,
    pub street_enum_rows: u64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalStatState {
    Value,
    Null,
    Blocked,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CanonicalStatNumericValue {
    Integer(u64),
    Float(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalStatPoint {
    pub state: CanonicalStatState,
    pub value: Option<CanonicalStatNumericValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalStatSnapshot {
    pub coverage: SeedStatCoverage,
    pub values: BTreeMap<String, CanonicalStatPoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeedStatSnapshot {
    pub coverage: SeedStatCoverage,
    pub roi_pct: Option<f64>,
    pub avg_finish_place: Option<f64>,
    pub final_table_reach_percent: Option<f64>,
    pub total_ko_event_count: u64,
    pub avg_ko_event_per_tournament: Option<f64>,
    pub early_ft_ko_event_count: u64,
    pub early_ft_ko_event_per_tournament: Option<f64>,
}

impl SeedStatSnapshot {
    pub fn to_canonical_snapshot(&self) -> CanonicalStatSnapshot {
        let mut values = BTreeMap::new();
        values.insert(
            "roi_pct".to_string(),
            CanonicalStatPoint::from_optional_float(self.roi_pct),
        );
        values.insert(
            "avg_finish_place".to_string(),
            CanonicalStatPoint::from_optional_float(self.avg_finish_place),
        );
        values.insert(
            "final_table_reach_percent".to_string(),
            CanonicalStatPoint::from_optional_float(self.final_table_reach_percent),
        );
        values.insert(
            "total_ko_event_count".to_string(),
            CanonicalStatPoint::from_integer(self.total_ko_event_count),
        );
        values.insert(
            "avg_ko_event_per_tournament".to_string(),
            CanonicalStatPoint::from_optional_float(self.avg_ko_event_per_tournament),
        );
        values.insert(
            "early_ft_ko_event_count".to_string(),
            CanonicalStatPoint::from_integer(self.early_ft_ko_event_count),
        );
        values.insert(
            "early_ft_ko_event_per_tournament".to_string(),
            CanonicalStatPoint::from_optional_float(self.early_ft_ko_event_per_tournament),
        );

        CanonicalStatSnapshot {
            coverage: self.coverage.clone(),
            values,
        }
    }
}

impl CanonicalStatPoint {
    pub(crate) fn from_integer(value: u64) -> Self {
        Self {
            state: CanonicalStatState::Value,
            value: Some(CanonicalStatNumericValue::Integer(value)),
        }
    }

    pub(crate) fn from_optional_float(value: Option<f64>) -> Self {
        match value {
            Some(value) => Self {
                state: CanonicalStatState::Value,
                value: Some(CanonicalStatNumericValue::Float(value)),
            },
            None => Self {
                state: CanonicalStatState::Null,
                value: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CanonicalStatNumericValue, CanonicalStatState, HandFeatureFacts, SeedStatCoverage,
        SeedStatSnapshot, StreetFeatureFacts, StreetFeatureParticipant,
    };
    use uuid::Uuid;

    #[test]
    fn exposes_hand_feature_and_seed_stat_models() {
        let hand = HandFeatureFacts {
            hand_id: Uuid::nil(),
            tournament_id: Uuid::nil(),
            played_ft_hand: false,
            ft_table_size: None,
            is_boundary_hand: false,
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
            total_ko_event_count: 2,
            avg_ko_event_per_tournament: Some(2.0),
            early_ft_ko_event_count: 1,
            early_ft_ko_event_per_tournament: Some(1.0),
        };

        let street = StreetFeatureFacts {
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
        };

        assert_eq!(hand.exact_ko_count, 0);
        assert_eq!(snapshot.total_ko_event_count, 2);
        assert_eq!(snapshot.early_ft_ko_event_count, 1);
        assert_eq!(street.seat_no, 7);
    }

    #[test]
    fn seed_stat_snapshot_projects_into_general_canonical_surface() {
        let snapshot = SeedStatSnapshot {
            coverage: SeedStatCoverage {
                summary_tournament_count: 4,
                hand_tournament_count: 2,
            },
            roi_pct: Some(30.0),
            avg_finish_place: Some(3.5),
            final_table_reach_percent: Some(50.0),
            total_ko_event_count: 3,
            avg_ko_event_per_tournament: Some(1.5),
            early_ft_ko_event_count: 1,
            early_ft_ko_event_per_tournament: Some(1.0),
        };

        let canonical = snapshot.to_canonical_snapshot();

        assert_eq!(canonical.coverage.summary_tournament_count, 4);
        assert_eq!(canonical.values["roi_pct"].state, CanonicalStatState::Value);
        assert_eq!(
            canonical.values["roi_pct"].value,
            Some(CanonicalStatNumericValue::Float(30.0))
        );
        assert_eq!(
            canonical.values["total_ko_event_count"].value,
            Some(CanonicalStatNumericValue::Integer(3))
        );
    }
}
