const SHARE_SCALE: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitBountyShareKind {
    ExactIntegral,
    EstimatedFloorCeilInterval,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SplitBountyShareOutcome {
    pub kind: SplitBountyShareKind,
    pub exact_cents: Option<i64>,
    pub min_cents: i64,
    pub expected_cents: f64,
    pub max_cents: i64,
    pub candidate_cents: Vec<i64>,
}

pub fn project_split_bounty_share(
    envelope_cents: i64,
    share_micros: i64,
) -> SplitBountyShareOutcome {
    if envelope_cents <= 0 || share_micros <= 0 {
        return SplitBountyShareOutcome {
            kind: SplitBountyShareKind::ExactIntegral,
            exact_cents: Some(0),
            min_cents: 0,
            expected_cents: 0.0,
            max_cents: 0,
            candidate_cents: vec![0],
        };
    }

    let numerator = envelope_cents * share_micros;
    let min_cents = numerator.div_euclid(SHARE_SCALE);
    let remainder = numerator.rem_euclid(SHARE_SCALE);
    let expected_cents = numerator as f64 / SHARE_SCALE as f64;

    if remainder == 0 {
        return SplitBountyShareOutcome {
            kind: SplitBountyShareKind::ExactIntegral,
            exact_cents: Some(min_cents),
            min_cents,
            expected_cents,
            max_cents: min_cents,
            candidate_cents: vec![min_cents],
        };
    }

    SplitBountyShareOutcome {
        kind: SplitBountyShareKind::EstimatedFloorCeilInterval,
        exact_cents: None,
        min_cents,
        expected_cents,
        max_cents: min_cents + 1,
        candidate_cents: vec![min_cents, min_cents + 1],
    }
}

#[cfg(test)]
mod tests {
    use super::{SplitBountyShareKind, project_split_bounty_share};

    #[test]
    fn keeps_integral_split_exact() {
        let outcome = project_split_bounty_share(10_500, 500_000);

        assert_eq!(outcome.kind, SplitBountyShareKind::ExactIntegral);
        assert_eq!(outcome.exact_cents, Some(5_250));
        assert_eq!(outcome.candidate_cents, vec![5_250]);
    }

    #[test]
    fn builds_floor_ceil_interval_for_ugly_cent_split() {
        let outcome = project_split_bounty_share(1_000, 333_333);

        assert_eq!(
            outcome.kind,
            SplitBountyShareKind::EstimatedFloorCeilInterval
        );
        assert_eq!(outcome.exact_cents, None);
        assert_eq!(outcome.min_cents, 333);
        assert_eq!(outcome.max_cents, 334);
        assert_eq!(outcome.candidate_cents, vec![333, 334]);
        assert!(outcome.expected_cents > 333.0);
        assert!(outcome.expected_cents < 334.0);
    }
}
