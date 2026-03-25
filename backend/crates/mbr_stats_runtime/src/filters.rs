use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use postgres::GenericClient;
use uuid::Uuid;

use crate::registry::FEATURE_VERSION;

const EXACT_CORE_SEAT_SURFACE: &str = "seat";

#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue {
    Bool(bool),
    Num(f64),
    Enum(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOperator {
    Eq,
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuntimeFilterSet {
    pub hero_filters: Vec<FilterCondition>,
    pub opponent_filters: Vec<FilterCondition>,
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

pub fn evaluate_runtime_filter_set(
    context: &HandFilterContext,
    filters: &RuntimeFilterSet,
) -> std::result::Result<bool, FilterError> {
    let hero_matches = evaluate_group(context, &filters.hero_filters, true)?;
    if !hero_matches {
        return Ok(false);
    }

    evaluate_group(context, &filters.opponent_filters, false)
}

pub fn query_matching_hand_ids(
    client: &mut impl GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
    filters: &RuntimeFilterSet,
) -> Result<Vec<Uuid>> {
    let contexts = load_filter_contexts(client, organization_id, player_profile_id)?;
    let mut matches = collect_matching_hand_ids(&contexts, filters)
        .map_err(|error| anyhow!("runtime filter evaluation failed: {error:?}"))?;
    matches.sort_unstable();
    Ok(matches)
}

fn collect_matching_hand_ids(
    contexts: &[HandFilterContext],
    filters: &RuntimeFilterSet,
) -> std::result::Result<Vec<Uuid>, FilterError> {
    let mut matches = Vec::new();
    for context in contexts {
        if evaluate_runtime_filter_set(context, filters)? {
            matches.push(context.hand_id);
        }
    }

    Ok(matches)
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
        "SELECT latest.hand_id, jsonb_array_elements_text(latest.uncertain_reason_codes)
         FROM (
             SELECT DISTINCT ON (hsr.hand_id)
                 hsr.hand_id,
                 hsr.uncertain_reason_codes
             FROM derived.hand_state_resolutions hsr
             INNER JOIN core.hands h
               ON h.id = hsr.hand_id
             WHERE h.organization_id = $1
               AND h.player_profile_id = $2
             ORDER BY hsr.hand_id, hsr.created_at DESC
         ) AS latest",
        &[&organization_id, &player_profile_id],
    )? {
        let hand_id: Uuid = row.get(0);
        if let Some(context) = contexts.get_mut(&hand_id) {
            let reason: String = row.get(1);
            let reason_code = extract_reason_code(&reason);
            context
                .hand_bool_values
                .insert(format!("has_uncertain_reason_code:{reason_code}"), true);
        }
    }

    for row in client.query(
        "SELECT latest.hand_id, jsonb_array_elements_text(latest.invariant_errors)
         FROM (
             SELECT DISTINCT ON (hsr.hand_id)
                 hsr.hand_id,
                 hsr.invariant_errors
             FROM derived.hand_state_resolutions hsr
             INNER JOIN core.hands h
               ON h.id = hsr.hand_id
             WHERE h.organization_id = $1
               AND h.player_profile_id = $2
             ORDER BY hsr.hand_id, hsr.created_at DESC
         ) AS latest",
        &[&organization_id, &player_profile_id],
    )? {
        let hand_id: Uuid = row.get(0);
        if let Some(context) = contexts.get_mut(&hand_id) {
            let issue: String = row.get(1);
            let issue_code = extract_reason_code(&issue);
            context
                .hand_bool_values
                .insert(format!("has_invariant_error_code:{issue_code}"), true);
            if is_action_legality_issue_code(issue_code) {
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
             hp.position_code,
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
            .insert("position_code".to_string(), row.get(4));
        entry.num_values.insert(
            "preflop_act_order_index".to_string(),
            f64::from(row.get::<_, i32>(5)),
        );
        entry.num_values.insert(
            "postflop_act_order_index".to_string(),
            f64::from(row.get::<_, i32>(6)),
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
          AND hs.player_name = ANY(he.ko_involved_winners)
         INNER JOIN core.hands h
           ON h.id = he.hand_id
         WHERE h.organization_id = $1
           AND h.player_profile_id = $2
           AND he.certainty_state = 'exact'",
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
           AND he.certainty_state = 'exact'",
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
        let entry = street_rows.entry(key).or_insert_with(|| base_street_row(&row));
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
            FeatureRef::Hand { feature_key } | FeatureRef::Street { feature_key, .. } => {
                reject_unsupported_feature(feature_key)?
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
    reject_unsupported_feature(feature_key)?;

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
    reject_unsupported_feature(feature_key)?;

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

fn reject_unsupported_feature(feature_key: &str) -> std::result::Result<(), FilterError> {
    if matches!(feature_key, "is_nut_hand" | "is_nut_draw") {
        return Err(FilterError::UnsupportedFeature(feature_key.to_string()));
    }

    Ok(())
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
        FilterOperator::Gte => actual >= *expected,
        FilterOperator::Lte => actual <= *expected,
    })
}

fn compare_enum(
    actual: &str,
    operator: &FilterOperator,
    expected: &FilterValue,
) -> std::result::Result<bool, FilterError> {
    let FilterValue::Enum(expected) = expected else {
        return Err(FilterError::InvalidComparison("enum feature".to_string()));
    };
    if *operator != FilterOperator::Eq {
        return Err(FilterError::InvalidComparison("enum feature".to_string()));
    }
    Ok(actual == expected)
}

fn extract_reason_code(reason: &str) -> &str {
    reason
        .split_once(':')
        .map(|(code, _)| code)
        .unwrap_or(reason)
        .trim()
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

fn is_sparse_exact_core_hand_feature(feature_key: &str) -> bool {
    feature_key.starts_with("has_uncertain_reason_code:")
        || feature_key.starts_with("has_invariant_error_code:")
        || feature_key.starts_with("has_action_legality_issue:")
}

#[cfg(test)]
mod tests {
    use super::{
        FeatureRef, FilterCondition, FilterError, FilterOperator, FilterValue, HandFilterContext,
        RuntimeFilterSet, StreetFilterRow, collect_matching_hand_ids, evaluate_runtime_filter_set,
    };
    use std::collections::BTreeMap;
    use uuid::Uuid;

    #[test]
    fn matches_combined_hero_and_opponent_groups_across_hand_and_street_features() {
        let context = HandFilterContext {
            hand_id: Uuid::nil(),
            hand_bool_values: BTreeMap::from([("played_ft_hand".to_string(), true)]),
            hand_num_values: BTreeMap::new(),
            hand_enum_values: BTreeMap::new(),
            street_rows: vec![
                StreetFilterRow {
                    seat_no: 7,
                    street: "flop".to_string(),
                    is_hero: true,
                    enum_values: BTreeMap::from([(
                        "made_hand_category".to_string(),
                        "overpair".to_string(),
                    )]),
                    ..StreetFilterRow::default()
                },
                StreetFilterRow {
                    seat_no: 3,
                    street: "turn".to_string(),
                    is_hero: false,
                    enum_values: BTreeMap::from([(
                        "draw_category".to_string(),
                        "flush_draw".to_string(),
                    )]),
                    ..StreetFilterRow::default()
                },
            ],
        };

        let filters = RuntimeFilterSet {
            hero_filters: vec![
                FilterCondition {
                    feature: FeatureRef::Hand {
                        feature_key: "played_ft_hand".to_string(),
                    },
                    operator: FilterOperator::Eq,
                    value: FilterValue::Bool(true),
                },
                FilterCondition {
                    feature: FeatureRef::Street {
                        street: "flop".to_string(),
                        feature_key: "made_hand_category".to_string(),
                    },
                    operator: FilterOperator::Eq,
                    value: FilterValue::Enum("overpair".to_string()),
                },
            ],
            opponent_filters: vec![FilterCondition {
                feature: FeatureRef::Street {
                    street: "turn".to_string(),
                    feature_key: "draw_category".to_string(),
                },
                operator: FilterOperator::Eq,
                value: FilterValue::Enum("flush_draw".to_string()),
            }],
        };

        assert_eq!(evaluate_runtime_filter_set(&context, &filters), Ok(true));
    }

    #[test]
    fn opponent_group_requires_single_opponent_to_match_all_street_predicates() {
        let context = HandFilterContext {
            hand_id: Uuid::nil(),
            hand_bool_values: BTreeMap::new(),
            hand_num_values: BTreeMap::new(),
            hand_enum_values: BTreeMap::new(),
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
                    enum_values: BTreeMap::from([(
                        "draw_category".to_string(),
                        "none".to_string(),
                    )]),
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
        };

        let filters = RuntimeFilterSet {
            hero_filters: vec![],
            opponent_filters: vec![
                FilterCondition {
                    feature: FeatureRef::Street {
                        street: "flop".to_string(),
                        feature_key: "made_hand_category".to_string(),
                    },
                    operator: FilterOperator::Eq,
                    value: FilterValue::Enum("top_pair".to_string()),
                },
                FilterCondition {
                    feature: FeatureRef::Street {
                        street: "turn".to_string(),
                        feature_key: "draw_category".to_string(),
                    },
                    operator: FilterOperator::Eq,
                    value: FilterValue::Enum("flush_draw".to_string()),
                },
            ],
        };

        assert_eq!(evaluate_runtime_filter_set(&context, &filters), Ok(false));
    }

    #[test]
    fn rejects_deferred_nut_predicates_honestly() {
        let context = HandFilterContext {
            hand_id: Uuid::nil(),
            ..HandFilterContext::default()
        };
        let filters = RuntimeFilterSet {
            hero_filters: vec![FilterCondition {
                feature: FeatureRef::Street {
                    street: "river".to_string(),
                    feature_key: "is_nut_hand".to_string(),
                },
                operator: FilterOperator::Eq,
                value: FilterValue::Bool(true),
            }],
            opponent_filters: vec![],
        };

        assert_eq!(
            evaluate_runtime_filter_set(&context, &filters),
            Err(FilterError::UnsupportedFeature("is_nut_hand".to_string()))
        );
    }

    #[test]
    fn query_collection_does_not_silence_filter_errors() {
        let contexts = vec![HandFilterContext {
            hand_id: Uuid::nil(),
            ..HandFilterContext::default()
        }];
        let filters = RuntimeFilterSet {
            hero_filters: vec![FilterCondition {
                feature: FeatureRef::Street {
                    street: "flop".to_string(),
                    feature_key: "is_nut_draw".to_string(),
                },
                operator: FilterOperator::Eq,
                value: FilterValue::Bool(true),
            }],
            opponent_filters: vec![],
        };

        assert_eq!(
            collect_matching_hand_ids(&contexts, &filters),
            Err(FilterError::UnsupportedFeature("is_nut_draw".to_string()))
        );
    }

    #[test]
    fn missing_exact_core_hand_presence_feature_evaluates_to_false() {
        let context = HandFilterContext {
            hand_id: Uuid::nil(),
            ..HandFilterContext::default()
        };
        let filters = RuntimeFilterSet {
            hero_filters: vec![FilterCondition {
                feature: FeatureRef::Hand {
                    feature_key:
                        "has_uncertain_reason_code:pot_settlement_ambiguous_hidden_showdown"
                            .to_string(),
                },
                operator: FilterOperator::Eq,
                value: FilterValue::Bool(true),
            }],
            opponent_filters: vec![],
        };

        assert_eq!(evaluate_runtime_filter_set(&context, &filters), Ok(false));
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
                enum_values: BTreeMap::from([("position_code".to_string(), "BB".to_string())]),
                ..StreetFilterRow::default()
            }],
        };
        let filters = RuntimeFilterSet {
            hero_filters: vec![
                FilterCondition {
                    feature: FeatureRef::Street {
                        street: "seat".to_string(),
                        feature_key: "position_code".to_string(),
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
            ],
            opponent_filters: vec![],
        };

        assert_eq!(evaluate_runtime_filter_set(&context, &filters), Ok(false));
    }
}
