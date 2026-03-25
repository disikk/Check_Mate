#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreetStrengthBucket {
    Best,
    Good,
    Weak,
    Trash,
}

impl StreetStrengthBucket {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Best => "best",
            Self::Good => "good",
            Self::Weak => "weak",
            Self::Trash => "trash",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreetBucketInput {
    pub street: String,
    pub best_hand_class: String,
    pub made_hand_category: String,
    pub draw_category: String,
    pub overcards_count: i32,
    pub has_air: bool,
    pub missed_flush_draw: bool,
    pub missed_straight_draw: bool,
}

pub fn project_street_bucket(input: &StreetBucketInput) -> StreetStrengthBucket {
    if matches!(
        input.made_hand_category.as_str(),
        "straight"
            | "flush"
            | "full_house"
            | "quads"
            | "straight_flush"
            | "set"
            | "trips"
            | "two_pair"
    ) {
        return StreetStrengthBucket::Best;
    }

    if matches!(
        input.made_hand_category.as_str(),
        "overpair" | "top_pair_top" | "top_pair_good"
    ) || matches!(
        input.draw_category.as_str(),
        "combo_draw" | "flush_draw" | "open_ended" | "double_gutshot"
    ) {
        return StreetStrengthBucket::Good;
    }

    if matches!(
        input.made_hand_category.as_str(),
        "board_pair_only" | "underpair" | "third_pair" | "second_pair" | "top_pair_weak"
    ) || matches!(
        input.draw_category.as_str(),
        "gutshot" | "backdoor_flush_only"
    ) || (input.street == "river"
        && input.made_hand_category == "high_card"
        && (input.missed_flush_draw || input.missed_straight_draw))
    {
        return StreetStrengthBucket::Weak;
    }

    if input.has_air || input.made_hand_category == "high_card" || input.draw_category == "none" {
        return StreetStrengthBucket::Trash;
    }

    StreetStrengthBucket::Trash
}

#[cfg(test)]
mod tests {
    use super::{StreetBucketInput, StreetStrengthBucket, project_street_bucket};

    #[test]
    fn maps_very_strong_made_hands_to_best() {
        let bucket = project_street_bucket(&StreetBucketInput {
            street: "river".to_string(),
            best_hand_class: "flush".to_string(),
            made_hand_category: "flush".to_string(),
            draw_category: "none".to_string(),
            overcards_count: 0,
            has_air: false,
            missed_flush_draw: false,
            missed_straight_draw: false,
        });

        assert_eq!(bucket, StreetStrengthBucket::Best);
    }

    #[test]
    fn maps_normal_made_hands_and_strong_draws_to_good() {
        let top_pair_bucket = project_street_bucket(&StreetBucketInput {
            street: "flop".to_string(),
            best_hand_class: "pair".to_string(),
            made_hand_category: "top_pair_good".to_string(),
            draw_category: "none".to_string(),
            overcards_count: 0,
            has_air: false,
            missed_flush_draw: false,
            missed_straight_draw: false,
        });
        let combo_draw_bucket = project_street_bucket(&StreetBucketInput {
            street: "turn".to_string(),
            best_hand_class: "high_card".to_string(),
            made_hand_category: "high_card".to_string(),
            draw_category: "combo_draw".to_string(),
            overcards_count: 0,
            has_air: false,
            missed_flush_draw: false,
            missed_straight_draw: false,
        });

        assert_eq!(top_pair_bucket, StreetStrengthBucket::Good);
        assert_eq!(combo_draw_bucket, StreetStrengthBucket::Good);
    }

    #[test]
    fn maps_weak_pairs_and_weak_draws_to_weak() {
        let weak_pair_bucket = project_street_bucket(&StreetBucketInput {
            street: "flop".to_string(),
            best_hand_class: "pair".to_string(),
            made_hand_category: "third_pair".to_string(),
            draw_category: "none".to_string(),
            overcards_count: 0,
            has_air: false,
            missed_flush_draw: false,
            missed_straight_draw: false,
        });
        let gutshot_bucket = project_street_bucket(&StreetBucketInput {
            street: "turn".to_string(),
            best_hand_class: "high_card".to_string(),
            made_hand_category: "high_card".to_string(),
            draw_category: "gutshot".to_string(),
            overcards_count: 0,
            has_air: false,
            missed_flush_draw: false,
            missed_straight_draw: false,
        });

        assert_eq!(weak_pair_bucket, StreetStrengthBucket::Weak);
        assert_eq!(gutshot_bucket, StreetStrengthBucket::Weak);
    }

    #[test]
    fn maps_air_like_cases_to_trash() {
        let bucket = project_street_bucket(&StreetBucketInput {
            street: "flop".to_string(),
            best_hand_class: "high_card".to_string(),
            made_hand_category: "high_card".to_string(),
            draw_category: "none".to_string(),
            overcards_count: 0,
            has_air: true,
            missed_flush_draw: false,
            missed_straight_draw: false,
        });

        assert_eq!(bucket, StreetStrengthBucket::Trash);
    }

    #[test]
    fn keeps_overcards_only_out_of_weak_bucket() {
        let bucket = project_street_bucket(&StreetBucketInput {
            street: "flop".to_string(),
            best_hand_class: "high_card".to_string(),
            made_hand_category: "high_card".to_string(),
            draw_category: "none".to_string(),
            overcards_count: 2,
            has_air: false,
            missed_flush_draw: false,
            missed_straight_draw: false,
        });

        assert_eq!(bucket, StreetStrengthBucket::Trash);
    }

    #[test]
    fn maps_river_missed_draw_high_card_to_weak() {
        let bucket = project_street_bucket(&StreetBucketInput {
            street: "river".to_string(),
            best_hand_class: "high_card".to_string(),
            made_hand_category: "high_card".to_string(),
            draw_category: "none".to_string(),
            overcards_count: 0,
            has_air: false,
            missed_flush_draw: true,
            missed_straight_draw: false,
        });

        assert_eq!(bucket, StreetStrengthBucket::Weak);
    }
}
