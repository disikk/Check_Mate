#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MoneyMutationFailure {
    ActionAmountExceedsStack {
        available_stack: i64,
        attempted_amount: i64,
    },
    RefundExceedsCommitted {
        committed_total: i64,
        attempted_refund: i64,
    },
    RefundExceedsBettingRoundContrib {
        betting_round_contrib: i64,
        attempted_refund: i64,
    },
}

pub(crate) fn apply_debit(stack_current: &mut i64, delta: i64) -> Result<(), MoneyMutationFailure> {
    if delta <= 0 {
        return Ok(());
    }
    if delta > *stack_current {
        return Err(MoneyMutationFailure::ActionAmountExceedsStack {
            available_stack: *stack_current,
            attempted_amount: delta,
        });
    }

    *stack_current -= delta;
    Ok(())
}

pub(crate) fn validate_refund(
    committed_total: Option<i64>,
    betting_round_contrib: i64,
    refund: i64,
) -> Vec<MoneyMutationFailure> {
    if refund <= 0 {
        return Vec::new();
    }

    let mut failures = Vec::new();

    if let Some(committed_total) = committed_total
        && refund > committed_total
    {
        failures.push(MoneyMutationFailure::RefundExceedsCommitted {
            committed_total,
            attempted_refund: refund,
        });
    }
    if refund > betting_round_contrib {
        failures.push(MoneyMutationFailure::RefundExceedsBettingRoundContrib {
            betting_round_contrib,
            attempted_refund: refund,
        });
    }

    failures
}

pub(crate) fn apply_refund(
    stack_current: &mut i64,
    committed_total: Option<&mut i64>,
    committed_by_street: Option<&mut i64>,
    betting_round_contrib: &mut i64,
    refund: i64,
) -> Result<(), Vec<MoneyMutationFailure>> {
    let failures = validate_refund(
        committed_total.as_ref().map(|value| **value),
        *betting_round_contrib,
        refund,
    );
    if !failures.is_empty() {
        return Err(failures);
    }

    *stack_current += refund;
    if let Some(committed_total) = committed_total {
        *committed_total -= refund;
    }
    if let Some(committed_by_street) = committed_by_street {
        *committed_by_street -= refund;
    }
    *betting_round_contrib -= refund;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MoneyMutationFailure, apply_debit, apply_refund};

    #[test]
    fn rejects_debit_above_stack_without_mutating_balance() {
        let mut stack_current = 20;

        let failure = apply_debit(&mut stack_current, 100).unwrap_err();

        assert_eq!(
            failure,
            MoneyMutationFailure::ActionAmountExceedsStack {
                available_stack: 20,
                attempted_amount: 100,
            }
        );
        assert_eq!(stack_current, 20);
    }

    #[test]
    fn rejects_refund_above_committed_and_round_without_mutating_counters() {
        let mut stack_current = 900;
        let mut committed_total = 100;
        let mut committed_by_street = 100;
        let mut betting_round_contrib = 100;

        let failures = apply_refund(
            &mut stack_current,
            Some(&mut committed_total),
            Some(&mut committed_by_street),
            &mut betting_round_contrib,
            200,
        )
        .unwrap_err();

        assert_eq!(
            failures,
            vec![
                MoneyMutationFailure::RefundExceedsCommitted {
                    committed_total: 100,
                    attempted_refund: 200,
                },
                MoneyMutationFailure::RefundExceedsBettingRoundContrib {
                    betting_round_contrib: 100,
                    attempted_refund: 200,
                },
            ]
        );
        assert_eq!(stack_current, 900);
        assert_eq!(committed_total, 100);
        assert_eq!(committed_by_street, 100);
        assert_eq!(betting_round_contrib, 100);
    }

    #[test]
    fn rejects_refund_above_round_contrib_without_optional_committed_guards() {
        let mut stack_current = 900;
        let mut betting_round_contrib = 50;

        let failures = apply_refund(
            &mut stack_current,
            None,
            None,
            &mut betting_round_contrib,
            200,
        )
        .unwrap_err();

        assert_eq!(
            failures,
            vec![MoneyMutationFailure::RefundExceedsBettingRoundContrib {
                betting_round_contrib: 50,
                attempted_refund: 200,
            }]
        );
        assert_eq!(stack_current, 900);
        assert_eq!(betting_round_contrib, 50);
    }
}
