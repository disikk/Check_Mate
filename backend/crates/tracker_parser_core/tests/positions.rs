use tracker_parser_core::positions::{PositionSeatInput, compute_position_facts};

#[test]
fn computes_position_facts_for_two_to_nine_active_seats() {
    let cases = [
        (
            2_u8,
            vec![(1_u8, "BTN", 1_u8, 2_u8), (2_u8, "BB", 2_u8, 1_u8)],
        ),
        (
            3_u8,
            vec![
                (1_u8, "BTN", 1_u8, 3_u8),
                (2_u8, "SB", 2_u8, 1_u8),
                (3_u8, "BB", 3_u8, 2_u8),
            ],
        ),
        (
            4_u8,
            vec![
                (1_u8, "BTN", 2_u8, 4_u8),
                (2_u8, "SB", 3_u8, 1_u8),
                (3_u8, "BB", 4_u8, 2_u8),
                (4_u8, "CO", 1_u8, 3_u8),
            ],
        ),
        (
            5_u8,
            vec![
                (1_u8, "BTN", 3_u8, 5_u8),
                (2_u8, "SB", 4_u8, 1_u8),
                (3_u8, "BB", 5_u8, 2_u8),
                (4_u8, "HJ", 1_u8, 3_u8),
                (5_u8, "CO", 2_u8, 4_u8),
            ],
        ),
        (
            6_u8,
            vec![
                (1_u8, "BTN", 4_u8, 6_u8),
                (2_u8, "SB", 5_u8, 1_u8),
                (3_u8, "BB", 6_u8, 2_u8),
                (4_u8, "LJ", 1_u8, 3_u8),
                (5_u8, "HJ", 2_u8, 4_u8),
                (6_u8, "CO", 3_u8, 5_u8),
            ],
        ),
        (
            7_u8,
            vec![
                (1_u8, "BTN", 5_u8, 7_u8),
                (2_u8, "SB", 6_u8, 1_u8),
                (3_u8, "BB", 7_u8, 2_u8),
                (4_u8, "MP", 1_u8, 3_u8),
                (5_u8, "LJ", 2_u8, 4_u8),
                (6_u8, "HJ", 3_u8, 5_u8),
                (7_u8, "CO", 4_u8, 6_u8),
            ],
        ),
        (
            8_u8,
            vec![
                (1_u8, "BTN", 6_u8, 8_u8),
                (2_u8, "SB", 7_u8, 1_u8),
                (3_u8, "BB", 8_u8, 2_u8),
                (4_u8, "UTG+1", 1_u8, 3_u8),
                (5_u8, "MP", 2_u8, 4_u8),
                (6_u8, "LJ", 3_u8, 5_u8),
                (7_u8, "HJ", 4_u8, 6_u8),
                (8_u8, "CO", 5_u8, 7_u8),
            ],
        ),
        (
            9_u8,
            vec![
                (1_u8, "BTN", 7_u8, 9_u8),
                (2_u8, "SB", 8_u8, 1_u8),
                (3_u8, "BB", 9_u8, 2_u8),
                (4_u8, "UTG", 1_u8, 3_u8),
                (5_u8, "UTG+1", 2_u8, 4_u8),
                (6_u8, "MP", 3_u8, 5_u8),
                (7_u8, "LJ", 4_u8, 6_u8),
                (8_u8, "HJ", 5_u8, 7_u8),
                (9_u8, "CO", 6_u8, 8_u8),
            ],
        ),
    ];

    for (active_count, expected_rows) in cases {
        let seats = (1_u8..=9)
            .map(|seat_no| PositionSeatInput {
                seat_no,
                is_active: seat_no <= active_count,
            })
            .collect::<Vec<_>>();

        let facts = compute_position_facts(9, 1, &seats).expect("position engine must resolve");
        assert_eq!(
            facts.len(),
            expected_rows.len(),
            "active_count={active_count}"
        );

        for (actual, expected) in facts.iter().zip(expected_rows) {
            assert_eq!(
                (
                    actual.seat_no,
                    actual.position_code.as_str(),
                    actual.preflop_act_order_index,
                    actual.postflop_act_order_index,
                ),
                expected,
                "active_count={active_count}"
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
        facts
            .iter()
            .map(|fact| (fact.seat_no, fact.position_code.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (2, "HJ"),
            (4, "CO"),
            (5, "BTN"),
            (7, "SB"),
            (8, "BB"),
            (9, "LJ"),
        ]
    );
}
