use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use postgres::GenericClient;
use uuid::Uuid;

pub const FEATURE_VERSION: &str = "mbr_runtime_v1";

const HAND_BOOL_FEATURES: &[&str] = &[
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
];

const HAND_NUM_FEATURES: &[&str] = &[
    "ft_table_size",
    "ft_players_remaining_exact",
    "hero_exact_ko_event_count",
    "hero_split_ko_event_count",
    "hero_sidepot_ko_event_count",
];

const HAND_ENUM_FEATURES: &[&str] = &["ft_stage_bucket"];

const STREET_BOOL_FEATURES: &[&str] = &[
    "has_air",
    "missed_flush_draw",
    "missed_straight_draw",
    "is_nut_hand",
    "is_nut_draw",
    "forced_all_in_preflop",
    "summary_has_shown_cards",
    "is_exact_ko_participant",
    "is_exact_ko_eliminated",
];

const STREET_NUM_FEATURES: &[&str] = &[
    "best_hand_rank_value",
    "overcards_count",
    "position_index",
    "preflop_act_order_index",
    "postflop_act_order_index",
    "summary_won_amount",
];

const STREET_ENUM_FEATURES: &[&str] = &[
    "starter_hand_class",
    "best_hand_class",
    "made_hand_category",
    "draw_category",
    "certainty_state",
    "position_label",
    "summary_outcome_kind",
    "summary_position_marker",
    "summary_folded_street",
    "summary_hand_class",
];

const HAND_DYNAMIC_BOOL_PREFIXES: &[&str] = &[
    "has_uncertain_reason_code:",
    "has_invariant_error_code:",
    "has_action_legality_issue:",
];

const STREET_DYNAMIC_BOOL_PREFIXES: &[&str] = &["has_all_in_reason:"];

const EXACT_CORE_SEAT_SURFACE: &str = "seat";

#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue {
    Bool(bool),
    Num(f64),
    Enum(String),
    EnumList(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOperator {
    Eq,
    In,
    Gte,
    Lte,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureRef {
    Hand { feature_key: String },
    Street { street: String, feature_key: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilterCondition {
    pub feature: FeatureRef,
    pub operator: FilterOperator,
    pub value: FilterValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandQueryRequest {
    pub organization_id: Uuid,
    pub player_profile_id: Uuid,
    pub hero_filters: Vec<FilterCondition>,
    pub opponent_filters: Vec<FilterCondition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandQueryResult {
    pub hand_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StreetFilterRow {
    pub seat_no: i32,
    pub street: String,
    pub is_hero: bool,
    pub bool_values: BTreeMap<String, bool>,
    pub num_values: BTreeMap<String, f64>,
    pub enum_values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HandFilterContext {
    pub hand_id: Uuid,
    pub hand_bool_values: BTreeMap<String, bool>,
    pub hand_num_values: BTreeMap<String, f64>,
    pub hand_enum_values: BTreeMap<String, String>,
    pub street_rows: Vec<StreetFilterRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterError {
    UnsupportedFeature(String),
    MissingFeature(String),
    InvalidComparison(String),
}

pub fn evaluate_hand_query_request(
    context: &HandFilterContext,
    query: &HandQueryRequest,
) -> std::result::Result<bool, FilterError> {
    let hero_matches = evaluate_group(context, &query.hero_filters, true)?;
    if !hero_matches {
        return Ok(false);
    }

    evaluate_group(context, &query.opponent_filters, false)
}

pub fn collect_matching_hand_ids(
    contexts: &[HandFilterContext],
    query: &HandQueryRequest,
) -> std::result::Result<HandQueryResult, FilterError> {
    let mut hand_ids = Vec::new();
    for context in contexts {
        if evaluate_hand_query_request(context, query)? {
            hand_ids.push(context.hand_id);
        }
    }
    hand_ids.sort_unstable();
    Ok(HandQueryResult { hand_ids })
}

pub fn query_matching_hand_ids(
    client: &mut impl GenericClient,
    query: &HandQueryRequest,
) -> Result<HandQueryResult> {
    let contexts = load_filter_contexts(client, query.organization_id, query.player_profile_id)?;
    collect_matching_hand_ids(&contexts, query)
        .map_err(|error| anyhow!("runtime filter evaluation failed: {error:?}"))
}

fn evaluate_group(
    context: &HandFilterContext,
    filters: &[FilterCondition],
    hero_group: bool,
) -> std::result::Result<bool, FilterError> {
    if filters.is_empty() {
        return Ok(true);
    }

    let (hand_filters, street_filters): (Vec<_>, Vec<_>) = filters
        .iter()
        .cloned()
        .partition(|condition| matches!(condition.feature, FeatureRef::Hand { .. }));

    validate_conditions(&hand_filters)?;
    validate_conditions(&street_filters)?;

    for condition in &hand_filters {
        if !evaluate_hand_condition(context, condition)? {
            return Ok(false);
        }
    }

    let candidate_rows = context
        .street_rows
        .iter()
        .filter(|row| row.is_hero == hero_group)
        .cloned()
        .collect::<Vec<_>>();

    if hero_group {
        return evaluate_hero_street_conditions(&candidate_rows, &street_filters);
    }

    evaluate_opponent_street_conditions(&candidate_rows, &street_filters)
}

fn validate_conditions(filters: &[FilterCondition]) -> std::result::Result<(), FilterError> {
    for condition in filters {
        match &condition.feature {
            FeatureRef::Hand { feature_key } => {
                if !is_supported_hand_feature(feature_key) {
                    return Err(FilterError::UnsupportedFeature(feature_key.clone()));
                }
            }
            FeatureRef::Street { feature_key, .. } => {
                if !is_supported_street_feature(feature_key) {
                    return Err(FilterError::UnsupportedFeature(feature_key.clone()));
                }
            }
        }
    }

    Ok(())
}

fn evaluate_hero_street_conditions(
    rows: &[StreetFilterRow],
    street_filters: &[FilterCondition],
) -> std::result::Result<bool, FilterError> {
    for condition in street_filters {
        let FeatureRef::Street { street, .. } = &condition.feature else {
            continue;
        };
        let Some(row) = rows.iter().find(|row| &row.street == street) else {
            return Ok(false);
        };
        if !evaluate_street_condition(row, condition)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn evaluate_opponent_street_conditions(
    rows: &[StreetFilterRow],
    street_filters: &[FilterCondition],
) -> std::result::Result<bool, FilterError> {
    if rows.is_empty() {
        return Ok(false);
    }

    if street_filters.is_empty() {
        return Ok(true);
    }

    let mut by_seat = BTreeMap::<i32, Vec<StreetFilterRow>>::new();
    for row in rows {
        by_seat.entry(row.seat_no).or_default().push(row.clone());
    }

    for seat_rows in by_seat.values() {
        let mut seat_matches = true;
        for condition in street_filters {
            let FeatureRef::Street { street, .. } = &condition.feature else {
                continue;
            };
            let Some(row) = seat_rows.iter().find(|row| &row.street == street) else {
                seat_matches = false;
                break;
            };
            if !evaluate_street_condition(row, condition)? {
                seat_matches = false;
                break;
            }
        }
        if seat_matches {
            return Ok(true);
        }
    }

    Ok(false)
}

fn evaluate_hand_condition(
    context: &HandFilterContext,
    condition: &FilterCondition,
) -> std::result::Result<bool, FilterError> {
    let FeatureRef::Hand { feature_key } = &condition.feature else {
        return Ok(false);
    };

    if let Some(value) = context.hand_bool_values.get(feature_key) {
        return compare_bool(*value, &condition.operator, &condition.value);
    }
    if let Some(value) = context.hand_num_values.get(feature_key) {
        return compare_num(*value, &condition.operator, &condition.value);
    }
    if let Some(value) = context.hand_enum_values.get(feature_key) {
        return compare_enum(value, &condition.operator, &condition.value);
    }

    if is_sparse_exact_core_hand_feature(feature_key) {
        return Ok(false);
    }

    Err(FilterError::MissingFeature(feature_key.clone()))
}

fn evaluate_street_condition(
    row: &StreetFilterRow,
    condition: &FilterCondition,
) -> std::result::Result<bool, FilterError> {
    let FeatureRef::Street { feature_key, .. } = &condition.feature else {
        return Ok(false);
    };

    if let Some(value) = row.bool_values.get(feature_key) {
        return compare_bool(*value, &condition.operator, &condition.value);
    }
    if let Some(value) = row.num_values.get(feature_key) {
        return compare_num(*value, &condition.operator, &condition.value);
    }
    if let Some(value) = row.enum_values.get(feature_key) {
        return compare_enum(value, &condition.operator, &condition.value);
    }

    if row.street == EXACT_CORE_SEAT_SURFACE {
        return Ok(false);
    }

    Err(FilterError::MissingFeature(feature_key.clone()))
}

fn compare_bool(
    actual: bool,
    operator: &FilterOperator,
    expected: &FilterValue,
) -> std::result::Result<bool, FilterError> {
    let FilterValue::Bool(expected) = expected else {
        return Err(FilterError::InvalidComparison("bool feature".to_string()));
    };
    if *operator != FilterOperator::Eq {
        return Err(FilterError::InvalidComparison("bool feature".to_string()));
    }
    Ok(actual == *expected)
}

fn compare_num(
    actual: f64,
    operator: &FilterOperator,
    expected: &FilterValue,
) -> std::result::Result<bool, FilterError> {
    let FilterValue::Num(expected) = expected else {
        return Err(FilterError::InvalidComparison("num feature".to_string()));
    };
    Ok(match operator {
        FilterOperator::Eq => (actual - *expected).abs() < f64::EPSILON,
        FilterOperator::In => {
            return Err(FilterError::InvalidComparison("num feature".to_string()));
        }
        FilterOperator::Gte => actual >= *expected,
        FilterOperator::Lte => actual <= *expected,
    })
}

fn compare_enum(
    actual: &str,
    operator: &FilterOperator,
    expected: &FilterValue,
) -> std::result::Result<bool, FilterError> {
    match operator {
        FilterOperator::Eq => {
            let FilterValue::Enum(expected) = expected else {
                return Err(FilterError::InvalidComparison("enum feature".to_string()));
            };
            Ok(actual == expected)
        }
        FilterOperator::In => {
            let FilterValue::EnumList(expected) = expected else {
                return Err(FilterError::InvalidComparison("enum feature".to_string()));
            };
            if expected.is_empty() {
                return Err(FilterError::InvalidComparison("enum feature".to_string()));
            }
            Ok(expected.iter().any(|value| value == actual))
        }
        FilterOperator::Gte | FilterOperator::Lte => {
            Err(FilterError::InvalidComparison("enum feature".to_string()))
        }
    }
}

fn is_supported_hand_feature(feature_key: &str) -> bool {
    HAND_BOOL_FEATURES.contains(&feature_key)
        || HAND_NUM_FEATURES.contains(&feature_key)
        || HAND_ENUM_FEATURES.contains(&feature_key)
        || HAND_DYNAMIC_BOOL_PREFIXES
            .iter()
            .any(|prefix| feature_key.starts_with(prefix))
}

fn is_supported_street_feature(feature_key: &str) -> bool {
    STREET_BOOL_FEATURES.contains(&feature_key)
        || STREET_NUM_FEATURES.contains(&feature_key)
        || STREET_ENUM_FEATURES.contains(&feature_key)
        || STREET_DYNAMIC_BOOL_PREFIXES
            .iter()
            .any(|prefix| feature_key.starts_with(prefix))
}

fn is_sparse_exact_core_hand_feature(feature_key: &str) -> bool {
    HAND_DYNAMIC_BOOL_PREFIXES
        .iter()
        .any(|prefix| feature_key.starts_with(prefix))
}

fn load_filter_contexts(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<Vec<HandFilterContext>> {
    let mut contexts = BTreeMap::<Uuid, HandFilterContext>::new();
    for hand_id_row in client.query(
        "SELECT id
         FROM core.hands
         WHERE organization_id = $1
           AND player_profile_id = $2
         ORDER BY id",
        &[&organization_id, &player_profile_id],
    )? {
        let hand_id: Uuid = hand_id_row.get(0);
        contexts.insert(
            hand_id,
            HandFilterContext {
                hand_id,
                ..HandFilterContext::default()
            },
        );
    }

    for row in client.query(
        "SELECT hand_id, feature_key, value
         FROM analytics.player_hand_bool_features
         WHERE organization_id = $1
           AND player_profile_id = $2
           AND feature_version = $3",
        &[&organization_id, &player_profile_id, &FEATURE_VERSION],
    )? {
        let hand_id: Uuid = row.get(0);
        if let Some(context) = contexts.get_mut(&hand_id) {
            context.hand_bool_values.insert(row.get(1), row.get(2));
        }
    }

    for row in client.query(
        "SELECT hand_id, feature_key, value::double precision
         FROM analytics.player_hand_num_features
         WHERE organization_id = $1
           AND player_profile_id = $2
           AND feature_version = $3",
        &[&organization_id, &player_profile_id, &FEATURE_VERSION],
    )? {
        let hand_id: Uuid = row.get(0);
        if let Some(context) = contexts.get_mut(&hand_id) {
            context.hand_num_values.insert(row.get(1), row.get(2));
        }
    }

    for row in client.query(
        "SELECT hand_id, feature_key, value
         FROM analytics.player_hand_enum_features
         WHERE organization_id = $1
           AND player_profile_id = $2
           AND feature_version = $3",
        &[&organization_id, &player_profile_id, &FEATURE_VERSION],
    )? {
        let hand_id: Uuid = row.get(0);
        if let Some(context) = contexts.get_mut(&hand_id) {
            context.hand_enum_values.insert(row.get(1), row.get(2));
        }
    }

    for row in client.query(
        "SELECT latest.hand_id, issue_codes.code, issue_codes.is_pot_issue
         FROM (
             SELECT DISTINCT ON (hsr.hand_id)
                 hsr.hand_id,
                 hsr.settlement
             FROM derived.hand_state_resolutions hsr
             INNER JOIN core.hands h
               ON h.id = hsr.hand_id
             WHERE h.organization_id = $1
               AND h.player_profile_id = $2
             ORDER BY hsr.hand_id, hsr.created_at DESC
         ) AS latest
         CROSS JOIN LATERAL (
             SELECT issue->>'code' AS code, FALSE AS is_pot_issue
             FROM jsonb_array_elements(COALESCE(latest.settlement->'issues', '[]'::jsonb)) issue
             UNION ALL
             SELECT pot_issue->>'code' AS code, TRUE AS is_pot_issue
             FROM jsonb_array_elements(COALESCE(latest.settlement->'pots', '[]'::jsonb)) pot,
                  jsonb_array_elements(COALESCE(pot->'issues', '[]'::jsonb)) pot_issue
         ) AS issue_codes",
        &[&organization_id, &player_profile_id],
    )? {
        let hand_id: Uuid = row.get(0);
        if let Some(context) = contexts.get_mut(&hand_id) {
            let raw_code: String = row.get(1);
            let is_pot_issue: bool = row.get(2);
            let reason_code = canonical_settlement_issue_code(&raw_code, is_pot_issue);
            context
                .hand_bool_values
                .insert(format!("has_uncertain_reason_code:{reason_code}"), true);
        }
    }

    for row in client.query(
        "SELECT latest.hand_id, issue->>'code'
         FROM (
             SELECT DISTINCT ON (hsr.hand_id)
                 hsr.hand_id,
                 hsr.invariant_issues
             FROM derived.hand_state_resolutions hsr
             INNER JOIN core.hands h
               ON h.id = hsr.hand_id
             WHERE h.organization_id = $1
               AND h.player_profile_id = $2
             ORDER BY hsr.hand_id, hsr.created_at DESC
         ) AS latest
         CROSS JOIN LATERAL jsonb_array_elements(COALESCE(latest.invariant_issues, '[]'::jsonb)) issue",
        &[&organization_id, &player_profile_id],
    )? {
        let hand_id: Uuid = row.get(0);
        if let Some(context) = contexts.get_mut(&hand_id) {
            let raw_code: String = row.get(1);
            let issue_code = canonical_invariant_issue_code(&raw_code);
            context
                .hand_bool_values
                .insert(format!("has_invariant_error_code:{issue_code}"), true);
            if is_action_legality_issue_code(&issue_code) {
                context
                    .hand_bool_values
                    .insert(format!("has_action_legality_issue:{issue_code}"), true);
            }
        }
    }

    let mut street_rows = BTreeMap::<(Uuid, i32, String), StreetFilterRow>::new();
    for row in client.query(
        "SELECT
             hp.hand_id,
             hp.seat_no,
             $3::text AS street,
             hs.is_hero,
             hp.position_label,
             hp.position_index,
             hp.preflop_act_order_index,
             hp.postflop_act_order_index
         FROM core.hand_positions hp
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = hp.hand_id
          AND hs.seat_no = hp.seat_no
         INNER JOIN core.hands h
           ON h.id = hp.hand_id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2",
        &[
            &organization_id,
            &player_profile_id,
            &EXACT_CORE_SEAT_SURFACE,
        ],
    )? {
        let key = street_key(&row);
        let entry = street_rows
            .entry(key)
            .or_insert_with(|| base_street_row(&row));
        entry
            .enum_values
            .insert("position_label".to_string(), row.get(4));
        entry.num_values.insert(
            "position_index".to_string(),
            f64::from(row.get::<_, i32>(5)),
        );
        entry.num_values.insert(
            "preflop_act_order_index".to_string(),
            f64::from(row.get::<_, i32>(6)),
        );
        entry.num_values.insert(
            "postflop_act_order_index".to_string(),
            f64::from(row.get::<_, i32>(7)),
        );
    }

    for row in client.query(
        "SELECT
             ha.hand_id,
             ha.seat_no,
             $3::text AS street,
             hs.is_hero,
             ha.all_in_reason,
             ha.forced_all_in_preflop
         FROM core.hand_actions ha
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = ha.hand_id
          AND hs.seat_no = ha.seat_no
         INNER JOIN core.hands h
           ON h.id = ha.hand_id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2
           AND ha.seat_no IS NOT NULL
           AND (
               ha.all_in_reason IS NOT NULL
               OR ha.forced_all_in_preflop
           )",
        &[
            &organization_id,
            &player_profile_id,
            &EXACT_CORE_SEAT_SURFACE,
        ],
    )? {
        let key = street_key(&row);
        let entry = street_rows
            .entry(key)
            .or_insert_with(|| base_street_row(&row));
        if let Some(reason) = row.get::<_, Option<String>>(4) {
            entry
                .bool_values
                .insert(format!("has_all_in_reason:{reason}"), true);
        }
        if row.get::<_, bool>(5) {
            entry
                .bool_values
                .insert("forced_all_in_preflop".to_string(), true);
        }
    }

    for row in client.query(
        "SELECT
             hsr.hand_id,
             hsr.seat_no,
             $3::text AS street,
             hs.is_hero,
             hsr.outcome_kind,
             hsr.position_marker,
             hsr.folded_street,
             hsr.shown_cards,
             hsr.won_amount::double precision,
             hsr.hand_class
         FROM core.hand_summary_results hsr
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = hsr.hand_id
          AND hs.seat_no = hsr.seat_no
         INNER JOIN core.hands h
           ON h.id = hsr.hand_id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2",
        &[
            &organization_id,
            &player_profile_id,
            &EXACT_CORE_SEAT_SURFACE,
        ],
    )? {
        let key = street_key(&row);
        let entry = street_rows
            .entry(key)
            .or_insert_with(|| base_street_row(&row));
        entry
            .enum_values
            .insert("summary_outcome_kind".to_string(), row.get(4));
        if let Some(position_marker) = row.get::<_, Option<String>>(5) {
            entry
                .enum_values
                .insert("summary_position_marker".to_string(), position_marker);
        }
        if let Some(folded_street) = row.get::<_, Option<String>>(6) {
            entry
                .enum_values
                .insert("summary_folded_street".to_string(), folded_street);
        }
        if let Some(shown_cards) = row.get::<_, Option<Vec<String>>>(7)
            && !shown_cards.is_empty()
        {
            entry
                .bool_values
                .insert("summary_has_shown_cards".to_string(), true);
        }
        if let Some(won_amount) = row.get::<_, Option<f64>>(8) {
            entry
                .num_values
                .insert("summary_won_amount".to_string(), won_amount);
        }
        if let Some(hand_class) = row.get::<_, Option<String>>(9) {
            entry
                .enum_values
                .insert("summary_hand_class".to_string(), hand_class);
        }
    }

    for row in client.query(
        "SELECT
             hs.hand_id,
             hs.seat_no,
             $3::text AS street,
             hs.is_hero
         FROM derived.hand_eliminations he
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = he.hand_id
          AND hs.player_name = ANY(he.ko_winner_set)
         INNER JOIN core.hands h
           ON h.id = he.hand_id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2
           AND he.ko_certainty_state = 'exact'",
        &[
            &organization_id,
            &player_profile_id,
            &EXACT_CORE_SEAT_SURFACE,
        ],
    )? {
        let key = street_key(&row);
        let entry = street_rows
            .entry(key)
            .or_insert_with(|| base_street_row(&row));
        entry
            .bool_values
            .insert("is_exact_ko_participant".to_string(), true);
    }

    for row in client.query(
        "SELECT
             hs.hand_id,
             hs.seat_no,
             $3::text AS street,
             hs.is_hero
         FROM derived.hand_eliminations he
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = he.hand_id
          AND hs.seat_no = he.eliminated_seat_no
         INNER JOIN core.hands h
           ON h.id = he.hand_id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2
           AND he.elimination_certainty_state = 'exact'",
        &[
            &organization_id,
            &player_profile_id,
            &EXACT_CORE_SEAT_SURFACE,
        ],
    )? {
        let key = street_key(&row);
        let entry = street_rows
            .entry(key)
            .or_insert_with(|| base_street_row(&row));
        entry
            .bool_values
            .insert("is_exact_ko_eliminated".to_string(), true);
    }

    for row in client.query(
        "SELECT sf.hand_id, sf.seat_no, sf.street, hs.is_hero, sf.feature_key, sf.value
         FROM analytics.player_street_bool_features sf
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = sf.hand_id
          AND hs.seat_no = sf.seat_no
         WHERE sf.organization_id = $1
           AND sf.player_profile_id = $2
           AND sf.feature_version = $3",
        &[&organization_id, &player_profile_id, &FEATURE_VERSION],
    )? {
        let key = street_key(&row);
        let entry = street_rows
            .entry(key)
            .or_insert_with(|| base_street_row(&row));
        entry.bool_values.insert(row.get(4), row.get(5));
    }

    for row in client.query(
        "SELECT sf.hand_id, sf.seat_no, sf.street, hs.is_hero, sf.feature_key, sf.value::double precision
         FROM analytics.player_street_num_features sf
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = sf.hand_id
          AND hs.seat_no = sf.seat_no
         WHERE sf.organization_id = $1
           AND sf.player_profile_id = $2
           AND sf.feature_version = $3",
        &[&organization_id, &player_profile_id, &FEATURE_VERSION],
    )? {
        let key = street_key(&row);
        let entry = street_rows
            .entry(key)
            .or_insert_with(|| base_street_row(&row));
        entry.num_values.insert(row.get(4), row.get(5));
    }

    for row in client.query(
        "SELECT sf.hand_id, sf.seat_no, sf.street, hs.is_hero, sf.feature_key, sf.value
         FROM analytics.player_street_enum_features sf
         INNER JOIN core.hand_seats hs
           ON hs.hand_id = sf.hand_id
          AND hs.seat_no = sf.seat_no
         WHERE sf.organization_id = $1
           AND sf.player_profile_id = $2
           AND sf.feature_version = $3",
        &[&organization_id, &player_profile_id, &FEATURE_VERSION],
    )? {
        let key = street_key(&row);
        let entry = street_rows
            .entry(key)
            .or_insert_with(|| base_street_row(&row));
        entry.enum_values.insert(row.get(4), row.get(5));
    }

    for ((hand_id, _, _), row) in street_rows {
        if let Some(context) = contexts.get_mut(&hand_id) {
            context.street_rows.push(row);
        }
    }

    Ok(contexts.into_values().collect())
}

fn street_key(row: &postgres::Row) -> (Uuid, i32, String) {
    (row.get(0), row.get(1), row.get(2))
}

fn base_street_row(row: &postgres::Row) -> StreetFilterRow {
    StreetFilterRow {
        seat_no: row.get(1),
        street: row.get(2),
        is_hero: row.get(3),
        ..StreetFilterRow::default()
    }
}

fn is_action_legality_issue_code(code: &str) -> bool {
    matches!(
        code,
        "premature_street_close"
            | "illegal_actor_order"
            | "illegal_small_blind_actor"
            | "illegal_big_blind_actor"
            | "illegal_check"
            | "illegal_call_amount"
            | "undercall_inconsistency"
            | "overcall_inconsistency"
            | "illegal_bet_facing_open_bet"
            | "action_not_reopened_after_short_all_in"
            | "incomplete_raise"
            | "uncalled_return_actor_mismatch"
            | "uncalled_return_amount_mismatch"
    )
}

fn canonical_settlement_issue_code(raw_code: &str, is_pot_issue: bool) -> String {
    match (is_pot_issue, raw_code) {
        (true, "ambiguous_hidden_showdown") => {
            "pot_settlement_ambiguous_hidden_showdown".to_string()
        }
        (true, "ambiguous_partial_reveal") => "pot_settlement_ambiguous_partial_reveal".to_string(),
        (false, "collect_events_without_pots") => "collect_events_without_pots".to_string(),
        (false, "missing_collections") => "pot_winners_missing_collections".to_string(),
        (false, "multiple_exact_allocations") => {
            "pot_settlement_multiple_exact_allocations".to_string()
        }
        (false, "collect_conflict_no_exact_settlement_matches_collected_amounts") => {
            "pot_settlement_collect_conflict".to_string()
        }
        _ => raw_code.to_string(),
    }
}

fn canonical_invariant_issue_code(raw_code: &str) -> String {
    match raw_code {
        "incomplete_raise_to_call" | "incomplete_raise_size" => "incomplete_raise".to_string(),
        _ => raw_code.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FeatureRef, FilterCondition, FilterOperator, FilterValue, HandFilterContext,
        HandQueryRequest, StreetFilterRow, canonical_invariant_issue_code,
        canonical_settlement_issue_code, evaluate_hand_query_request,
    };
    use std::collections::BTreeMap;
    use uuid::Uuid;

    fn request(hero_filters: Vec<FilterCondition>) -> HandQueryRequest {
        HandQueryRequest {
            organization_id: Uuid::nil(),
            player_profile_id: Uuid::nil(),
            hero_filters,
            opponent_filters: vec![],
        }
    }

    #[test]
    fn canonicalizes_typed_settlement_issue_codes_for_sparse_features() {
        assert_eq!(
            canonical_settlement_issue_code("ambiguous_hidden_showdown", true),
            "pot_settlement_ambiguous_hidden_showdown"
        );
        assert_eq!(
            canonical_settlement_issue_code(
                "collect_conflict_no_exact_settlement_matches_collected_amounts",
                false
            ),
            "pot_settlement_collect_conflict"
        );
    }

    #[test]
    fn canonicalizes_typed_invariant_issue_codes_for_sparse_features() {
        assert_eq!(
            canonical_invariant_issue_code("incomplete_raise_to_call"),
            "incomplete_raise"
        );
        assert_eq!(
            canonical_invariant_issue_code("illegal_actor_order"),
            "illegal_actor_order"
        );
    }

    #[test]
    fn missing_exact_core_hand_presence_feature_evaluates_to_false() {
        let context = HandFilterContext {
            hand_id: Uuid::nil(),
            ..HandFilterContext::default()
        };
        let query = request(vec![FilterCondition {
            feature: FeatureRef::Hand {
                feature_key: "has_uncertain_reason_code:pot_settlement_ambiguous_hidden_showdown"
                    .to_string(),
            },
            operator: FilterOperator::Eq,
            value: FilterValue::Bool(true),
        }]);

        assert_eq!(evaluate_hand_query_request(&context, &query), Ok(false));
    }

    #[test]
    fn missing_exact_core_seat_surface_feature_evaluates_to_false() {
        let context = HandFilterContext {
            hand_id: Uuid::nil(),
            hand_bool_values: BTreeMap::new(),
            hand_num_values: BTreeMap::new(),
            hand_enum_values: BTreeMap::new(),
            street_rows: vec![StreetFilterRow {
                seat_no: 7,
                street: "seat".to_string(),
                is_hero: true,
                enum_values: BTreeMap::from([("position_label".to_string(), "BB".to_string())]),
                ..StreetFilterRow::default()
            }],
        };
        let query = request(vec![
            FilterCondition {
                feature: FeatureRef::Street {
                    street: "seat".to_string(),
                    feature_key: "position_label".to_string(),
                },
                operator: FilterOperator::Eq,
                value: FilterValue::Enum("BB".to_string()),
            },
            FilterCondition {
                feature: FeatureRef::Street {
                    street: "seat".to_string(),
                    feature_key: "summary_outcome_kind".to_string(),
                },
                operator: FilterOperator::Eq,
                value: FilterValue::Enum("showed_won".to_string()),
            },
        ]);

        assert_eq!(evaluate_hand_query_request(&context, &query), Ok(false));
    }
}
