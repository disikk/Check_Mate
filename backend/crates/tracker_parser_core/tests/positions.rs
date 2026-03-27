use serde_json::Value;
use tracker_parser_core::positions::{PositionSeatInput, compute_position_facts};

#[test]
fn computes_position_facts_for_two_to_ten_active_seats() {
    let cases = [
        (
            2_u8,
            vec![
                (1_u8, 1_u8, "BTN", 1_u8, 2_u8),
                (2_u8, 2_u8, "BB", 2_u8, 1_u8),
            ],
        ),
        (
            3_u8,
            vec![
                (1_u8, 1_u8, "BTN", 1_u8, 3_u8),
                (2_u8, 2_u8, "SB", 2_u8, 1_u8),
                (3_u8, 3_u8, "BB", 3_u8, 2_u8),
            ],
        ),
        (
            4_u8,
            vec![
                (1_u8, 1_u8, "BTN", 2_u8, 4_u8),
                (2_u8, 2_u8, "SB", 3_u8, 1_u8),
                (3_u8, 3_u8, "BB", 4_u8, 2_u8),
                (4_u8, 4_u8, "CO", 1_u8, 3_u8),
            ],
        ),
        (
            5_u8,
            vec![
                (1_u8, 1_u8, "BTN", 3_u8, 5_u8),
                (2_u8, 2_u8, "SB", 4_u8, 1_u8),
                (3_u8, 3_u8, "BB", 5_u8, 2_u8),
                (4_u8, 4_u8, "HJ", 1_u8, 3_u8),
                (5_u8, 5_u8, "CO", 2_u8, 4_u8),
            ],
        ),
        (
            6_u8,
            vec![
                (1_u8, 1_u8, "BTN", 4_u8, 6_u8),
                (2_u8, 2_u8, "SB", 5_u8, 1_u8),
                (3_u8, 3_u8, "BB", 6_u8, 2_u8),
                (4_u8, 4_u8, "LJ", 1_u8, 3_u8),
                (5_u8, 5_u8, "HJ", 2_u8, 4_u8),
                (6_u8, 6_u8, "CO", 3_u8, 5_u8),
            ],
        ),
        (
            7_u8,
            vec![
                (1_u8, 1_u8, "BTN", 5_u8, 7_u8),
                (2_u8, 2_u8, "SB", 6_u8, 1_u8),
                (3_u8, 3_u8, "BB", 7_u8, 2_u8),
                (4_u8, 4_u8, "MP", 1_u8, 3_u8),
                (5_u8, 5_u8, "LJ", 2_u8, 4_u8),
                (6_u8, 6_u8, "HJ", 3_u8, 5_u8),
                (7_u8, 7_u8, "CO", 4_u8, 6_u8),
            ],
        ),
        (
            8_u8,
            vec![
                (1_u8, 1_u8, "BTN", 6_u8, 8_u8),
                (2_u8, 2_u8, "SB", 7_u8, 1_u8),
                (3_u8, 3_u8, "BB", 8_u8, 2_u8),
                (4_u8, 4_u8, "UTG+1", 1_u8, 3_u8),
                (5_u8, 5_u8, "MP", 2_u8, 4_u8),
                (6_u8, 6_u8, "LJ", 3_u8, 5_u8),
                (7_u8, 7_u8, "HJ", 4_u8, 6_u8),
                (8_u8, 8_u8, "CO", 5_u8, 7_u8),
            ],
        ),
        (
            9_u8,
            vec![
                (1_u8, 1_u8, "BTN", 7_u8, 9_u8),
                (2_u8, 2_u8, "SB", 8_u8, 1_u8),
                (3_u8, 3_u8, "BB", 9_u8, 2_u8),
                (4_u8, 4_u8, "UTG", 1_u8, 3_u8),
                (5_u8, 5_u8, "UTG+1", 2_u8, 4_u8),
                (6_u8, 6_u8, "MP", 3_u8, 5_u8),
                (7_u8, 7_u8, "LJ", 4_u8, 6_u8),
                (8_u8, 8_u8, "HJ", 5_u8, 7_u8),
                (9_u8, 9_u8, "CO", 6_u8, 8_u8),
            ],
        ),
        (
            10_u8,
            vec![
                (1_u8, 1_u8, "BTN", 8_u8, 10_u8),
                (2_u8, 2_u8, "SB", 9_u8, 1_u8),
                (3_u8, 3_u8, "BB", 10_u8, 2_u8),
                (4_u8, 4_u8, "UTG", 1_u8, 3_u8),
                (5_u8, 5_u8, "UTG+1", 2_u8, 4_u8),
                (6_u8, 6_u8, "UTG+2", 3_u8, 5_u8),
                (7_u8, 7_u8, "MP", 4_u8, 6_u8),
                (8_u8, 8_u8, "MP+1", 5_u8, 7_u8),
                (9_u8, 9_u8, "HJ", 6_u8, 8_u8),
                (10_u8, 10_u8, "CO", 7_u8, 9_u8),
            ],
        ),
    ];

    for (active_count, expected_rows) in cases {
        let seats = (1_u8..=10)
            .map(|seat_no| PositionSeatInput {
                seat_no,
                is_active: seat_no <= active_count,
            })
            .collect::<Vec<_>>();

        let facts = compute_position_facts(10, 1, &seats).expect("position engine must resolve");
        assert_eq!(
            facts.len(),
            expected_rows.len(),
            "active_count={active_count}"
        );

        for (actual, expected) in facts.iter().zip(expected_rows) {
            assert_eq!(
                position_signature(actual),
                expected_signature(expected),
                "active_count={active_count}"
            );
            assert!(
                !serde_json::to_value(actual)
                    .unwrap()
                    .as_object()
                    .unwrap()
                    .contains_key("position_code"),
                "legacy position_code must be removed for active_count={active_count}"
            );
        }
    }
}

#[test]
fn excludes_inactive_and_sitting_out_seats_from_position_facts() {
    let seats = vec![
        PositionSeatInput {
            seat_no: 1,
            is_active: false,
        },
        PositionSeatInput {
            seat_no: 2,
            is_active: true,
        },
        PositionSeatInput {
            seat_no: 3,
            is_active: false,
        },
        PositionSeatInput {
            seat_no: 4,
            is_active: true,
        },
        PositionSeatInput {
            seat_no: 5,
            is_active: true,
        },
        PositionSeatInput {
            seat_no: 6,
            is_active: false,
        },
        PositionSeatInput {
            seat_no: 7,
            is_active: true,
        },
        PositionSeatInput {
            seat_no: 8,
            is_active: true,
        },
        PositionSeatInput {
            seat_no: 9,
            is_active: true,
        },
    ];

    let facts = compute_position_facts(9, 5, &seats).expect("position engine must resolve");

    assert_eq!(
        facts.iter().map(position_signature).collect::<Vec<_>>(),
        vec![
            (2, 5, "HJ", 2, 4),
            (4, 6, "CO", 3, 5),
            (5, 1, "BTN", 4, 6),
            (7, 2, "SB", 5, 1),
            (8, 3, "BB", 6, 2),
            (9, 4, "LJ", 1, 3),
        ]
        .into_iter()
        .map(expected_signature)
        .collect::<Vec<_>>()
    );

    assert!(facts.iter().all(|fact| {
        !serde_json::to_value(fact)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("position_code")
    }));
}

fn position_signature(position: &impl serde::Serialize) -> (u8, u8, String, u8, u8) {
    let value = serde_json::to_value(position).unwrap();
    (
        json_u8(&value, "seat_no"),
        json_u8(&value, "position_index"),
        json_str(&value, "position_label").to_string(),
        json_u8(&value, "preflop_act_order_index"),
        json_u8(&value, "postflop_act_order_index"),
    )
}

fn json_u8(value: &Value, field: &str) -> u8 {
    value[field]
        .as_u64()
        .unwrap_or_else(|| panic!("missing numeric field `{field}` in {value:?}")) as u8
}

fn json_str<'a>(value: &'a Value, field: &str) -> &'a str {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("missing string field `{field}` in {value:?}"))
}

fn expected_signature(expected: (u8, u8, &str, u8, u8)) -> (u8, u8, String, u8, u8) {
    (
        expected.0,
        expected.1,
        expected.2.to_string(),
        expected.3,
        expected.4,
    )
}
