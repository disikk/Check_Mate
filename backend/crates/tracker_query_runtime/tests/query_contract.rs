use std::collections::BTreeMap;

use tracker_query_runtime::{
    FeatureRef, FilterCondition, FilterError, FilterOperator, FilterValue, HandFilterContext,
    HandQueryRequest, HandQueryResult, StreetFilterRow, collect_matching_hand_ids,
    evaluate_hand_query_request,
};
use uuid::Uuid;

fn request(
    hero_filters: Vec<FilterCondition>,
    opponent_filters: Vec<FilterCondition>,
) -> HandQueryRequest {
    HandQueryRequest {
        organization_id: Uuid::nil(),
        player_profile_id: Uuid::nil(),
        hero_filters,
        opponent_filters,
    }
}

fn hand_bool(feature_key: &str, value: bool) -> FilterCondition {
    FilterCondition {
        feature: FeatureRef::Hand {
            feature_key: feature_key.to_string(),
        },
        operator: FilterOperator::Eq,
        value: FilterValue::Bool(value),
    }
}

fn street_bool(street: &str, feature_key: &str, value: bool) -> FilterCondition {
    FilterCondition {
        feature: FeatureRef::Street {
            street: street.to_string(),
            feature_key: feature_key.to_string(),
        },
        operator: FilterOperator::Eq,
        value: FilterValue::Bool(value),
    }
}

fn street_enum(street: &str, feature_key: &str, value: &str) -> FilterCondition {
    FilterCondition {
        feature: FeatureRef::Street {
            street: street.to_string(),
            feature_key: feature_key.to_string(),
        },
        operator: FilterOperator::Eq,
        value: FilterValue::Enum(value.to_string()),
    }
}

fn street_enum_in(street: &str, feature_key: &str, values: &[&str]) -> FilterCondition {
    FilterCondition {
        feature: FeatureRef::Street {
            street: street.to_string(),
            feature_key: feature_key.to_string(),
        },
        operator: FilterOperator::In,
        value: FilterValue::EnumList(values.iter().map(|value| (*value).to_string()).collect()),
    }
}

#[test]
fn supports_nut_predicates_as_regular_hero_filters() {
    let context = HandFilterContext {
        hand_id: Uuid::nil(),
        street_rows: vec![
            StreetFilterRow {
                seat_no: 7,
                street: "turn".to_string(),
                is_hero: true,
                bool_values: BTreeMap::from([("is_nut_draw".to_string(), true)]),
                ..StreetFilterRow::default()
            },
            StreetFilterRow {
                seat_no: 7,
                street: "river".to_string(),
                is_hero: true,
                bool_values: BTreeMap::from([("is_nut_hand".to_string(), true)]),
                ..StreetFilterRow::default()
            },
        ],
        ..HandFilterContext::default()
    };

    let query = request(
        vec![
            street_bool("turn", "is_nut_draw", true),
            street_bool("river", "is_nut_hand", true),
        ],
        vec![],
    );

    assert_eq!(evaluate_hand_query_request(&context, &query), Ok(true));
}

#[test]
fn opponent_group_requires_one_known_opponent_to_satisfy_full_group() {
    let context = HandFilterContext {
        hand_id: Uuid::nil(),
        street_rows: vec![
            StreetFilterRow {
                seat_no: 3,
                street: "flop".to_string(),
                is_hero: false,
                enum_values: BTreeMap::from([(
                    "made_hand_category".to_string(),
                    "top_pair".to_string(),
                )]),
                ..StreetFilterRow::default()
            },
            StreetFilterRow {
                seat_no: 3,
                street: "turn".to_string(),
                is_hero: false,
                enum_values: BTreeMap::from([("draw_category".to_string(), "none".to_string())]),
                ..StreetFilterRow::default()
            },
            StreetFilterRow {
                seat_no: 5,
                street: "flop".to_string(),
                is_hero: false,
                enum_values: BTreeMap::from([(
                    "made_hand_category".to_string(),
                    "none".to_string(),
                )]),
                ..StreetFilterRow::default()
            },
            StreetFilterRow {
                seat_no: 5,
                street: "turn".to_string(),
                is_hero: false,
                enum_values: BTreeMap::from([(
                    "draw_category".to_string(),
                    "flush_draw".to_string(),
                )]),
                ..StreetFilterRow::default()
            },
        ],
        ..HandFilterContext::default()
    };

    let query = request(
        vec![],
        vec![
            street_enum("flop", "made_hand_category", "top_pair"),
            street_enum("turn", "draw_category", "flush_draw"),
        ],
    );

    assert_eq!(evaluate_hand_query_request(&context, &query), Ok(false));
}

#[test]
fn collect_matching_hand_ids_returns_stably_sorted_result() {
    let higher = Uuid::parse_str("ffffffff-ffff-ffff-ffff-ffffffffffff").unwrap();
    let lower = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let contexts = vec![
        HandFilterContext {
            hand_id: higher,
            hand_bool_values: BTreeMap::from([("played_ft_hand".to_string(), true)]),
            ..HandFilterContext::default()
        },
        HandFilterContext {
            hand_id: lower,
            hand_bool_values: BTreeMap::from([("played_ft_hand".to_string(), true)]),
            ..HandFilterContext::default()
        },
    ];
    let query = request(vec![hand_bool("played_ft_hand", true)], vec![]);

    assert_eq!(
        collect_matching_hand_ids(&contexts, &query),
        Ok(HandQueryResult {
            hand_ids: vec![lower, higher],
        })
    );
}

#[test]
fn rejects_unsupported_features_and_invalid_comparisons_hard() {
    let context = HandFilterContext {
        hand_id: Uuid::nil(),
        street_rows: vec![StreetFilterRow {
            seat_no: 7,
            street: "flop".to_string(),
            is_hero: true,
            enum_values: BTreeMap::from([(
                "made_hand_category".to_string(),
                "top_pair".to_string(),
            )]),
            ..StreetFilterRow::default()
        }],
        ..HandFilterContext::default()
    };

    let unsupported = request(vec![hand_bool("imaginary_feature", true)], vec![]);
    assert_eq!(
        evaluate_hand_query_request(&context, &unsupported),
        Err(FilterError::UnsupportedFeature(
            "imaginary_feature".to_string()
        ))
    );

    let invalid_comparison = request(
        vec![FilterCondition {
            feature: FeatureRef::Street {
                street: "flop".to_string(),
                feature_key: "made_hand_category".to_string(),
            },
            operator: FilterOperator::Gte,
            value: FilterValue::Enum("top_pair".to_string()),
        }],
        vec![],
    );
    assert_eq!(
        evaluate_hand_query_request(&context, &invalid_comparison),
        Err(FilterError::InvalidComparison("enum feature".to_string()))
    );
}

#[test]
fn supports_preflop_starter_hand_whitelist_for_hero() {
    let context = HandFilterContext {
        hand_id: Uuid::nil(),
        street_rows: vec![StreetFilterRow {
            seat_no: 7,
            street: "preflop".to_string(),
            is_hero: true,
            enum_values: BTreeMap::from([("starter_hand_class".to_string(), "AKs".to_string())]),
            ..StreetFilterRow::default()
        }],
        ..HandFilterContext::default()
    };

    let query = request(
        vec![street_enum_in(
            "preflop",
            "starter_hand_class",
            &["AA", "AKs"],
        )],
        vec![],
    );

    assert_eq!(evaluate_hand_query_request(&context, &query), Ok(true));
}

#[test]
fn opponent_group_requires_same_seat_to_match_preflop_and_postflop_filters() {
    let context = HandFilterContext {
        hand_id: Uuid::nil(),
        street_rows: vec![
            StreetFilterRow {
                seat_no: 3,
                street: "preflop".to_string(),
                is_hero: false,
                enum_values: BTreeMap::from([("starter_hand_class".to_string(), "AA".to_string())]),
                ..StreetFilterRow::default()
            },
            StreetFilterRow {
                seat_no: 3,
                street: "flop".to_string(),
                is_hero: false,
                enum_values: BTreeMap::from([(
                    "made_hand_category".to_string(),
                    "high_card".to_string(),
                )]),
                ..StreetFilterRow::default()
            },
            StreetFilterRow {
                seat_no: 5,
                street: "preflop".to_string(),
                is_hero: false,
                enum_values: BTreeMap::from([(
                    "starter_hand_class".to_string(),
                    "QJo".to_string(),
                )]),
                ..StreetFilterRow::default()
            },
            StreetFilterRow {
                seat_no: 5,
                street: "flop".to_string(),
                is_hero: false,
                enum_values: BTreeMap::from([(
                    "made_hand_category".to_string(),
                    "top_pair".to_string(),
                )]),
                ..StreetFilterRow::default()
            },
        ],
        ..HandFilterContext::default()
    };

    let query = request(
        vec![],
        vec![
            street_enum_in("preflop", "starter_hand_class", &["AA"]),
            street_enum("flop", "made_hand_category", "top_pair"),
        ],
    );

    assert_eq!(evaluate_hand_query_request(&context, &query), Ok(false));
}

#[test]
fn rejects_empty_or_non_enum_whitelists() {
    let context = HandFilterContext {
        hand_id: Uuid::nil(),
        street_rows: vec![StreetFilterRow {
            seat_no: 7,
            street: "preflop".to_string(),
            is_hero: true,
            enum_values: BTreeMap::from([("starter_hand_class".to_string(), "AA".to_string())]),
            ..StreetFilterRow::default()
        }],
        ..HandFilterContext::default()
    };

    let empty_whitelist = request(
        vec![FilterCondition {
            feature: FeatureRef::Street {
                street: "preflop".to_string(),
                feature_key: "starter_hand_class".to_string(),
            },
            operator: FilterOperator::In,
            value: FilterValue::EnumList(vec![]),
        }],
        vec![],
    );
    assert_eq!(
        evaluate_hand_query_request(&context, &empty_whitelist),
        Err(FilterError::InvalidComparison("enum feature".to_string()))
    );

    let bool_whitelist = request(
        vec![FilterCondition {
            feature: FeatureRef::Street {
                street: "preflop".to_string(),
                feature_key: "starter_hand_class".to_string(),
            },
            operator: FilterOperator::In,
            value: FilterValue::Bool(true),
        }],
        vec![],
    );
    assert_eq!(
        evaluate_hand_query_request(&context, &bool_whitelist),
        Err(FilterError::InvalidComparison("enum feature".to_string()))
    );
}
