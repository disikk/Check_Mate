use std::collections::BTreeSet;

const SHARE_SCALE: i64 = 1_000_000;
const MAX_RECORDED_ALLOCATIONS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MysteryEnvelope {
    pub sort_order: i32,
    pub payout_cents: i64,
    pub frequency_per_100m: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroKoShare {
    pub share_micros: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BigKoAllocation {
    pub envelope_payout_cents: Vec<i64>,
    pub hero_mystery_cents: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BigKoDecodeStatus {
    Exact,
    Ambiguous,
    Infeasible,
    ZeroMystery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigKoDecodeResult {
    pub status: BigKoDecodeStatus,
    pub mystery_money_cents: i64,
    pub allocations: Vec<BigKoAllocation>,
}

pub fn decode_big_ko_allocations(
    mystery_money_cents: i64,
    hero_ko_shares: &[HeroKoShare],
    mystery_envelopes: &[MysteryEnvelope],
) -> BigKoDecodeResult {
    let positive_shares = hero_ko_shares
        .iter()
        .filter(|share| share.share_micros > 0)
        .cloned()
        .collect::<Vec<_>>();

    if mystery_money_cents == 0 && positive_shares.is_empty() {
        return BigKoDecodeResult {
            status: BigKoDecodeStatus::ZeroMystery,
            mystery_money_cents,
            allocations: Vec::new(),
        };
    }

    if mystery_money_cents <= 0 || positive_shares.is_empty() || mystery_envelopes.is_empty() {
        return BigKoDecodeResult {
            status: BigKoDecodeStatus::Infeasible,
            mystery_money_cents,
            allocations: Vec::new(),
        };
    }

    let mut sorted_envelopes = mystery_envelopes.to_vec();
    sorted_envelopes.sort_by(|left, right| {
        right
            .payout_cents
            .cmp(&left.payout_cents)
            .then_with(|| left.sort_order.cmp(&right.sort_order))
    });

    let mut current_envelopes = Vec::with_capacity(positive_shares.len());
    let mut current_mystery = Vec::with_capacity(positive_shares.len());
    let mut allocations = BTreeSet::new();
    search_allocations(
        0,
        mystery_money_cents,
        &positive_shares,
        &sorted_envelopes,
        &mut current_envelopes,
        &mut current_mystery,
        &mut allocations,
    );

    let allocations = allocations.into_iter().collect::<Vec<_>>();
    let status = match allocations.len() {
        0 => BigKoDecodeStatus::Infeasible,
        1 => BigKoDecodeStatus::Exact,
        _ => BigKoDecodeStatus::Ambiguous,
    };

    BigKoDecodeResult {
        status,
        mystery_money_cents,
        allocations,
    }
}

fn search_allocations(
    share_index: usize,
    remaining_mystery_cents: i64,
    hero_ko_shares: &[HeroKoShare],
    mystery_envelopes: &[MysteryEnvelope],
    current_envelopes: &mut Vec<i64>,
    current_mystery: &mut Vec<i64>,
    allocations: &mut BTreeSet<BigKoAllocation>,
) {
    if allocations.len() >= MAX_RECORDED_ALLOCATIONS {
        return;
    }

    if share_index == hero_ko_shares.len() {
        if remaining_mystery_cents == 0 {
            allocations.insert(BigKoAllocation {
                envelope_payout_cents: current_envelopes.clone(),
                hero_mystery_cents: current_mystery.clone(),
            });
        }
        return;
    }

    for envelope in mystery_envelopes {
        let contribution_numerator = envelope.payout_cents * hero_ko_shares[share_index].share_micros;
        if contribution_numerator % SHARE_SCALE != 0 {
            continue;
        }

        let hero_mystery_cents = contribution_numerator / SHARE_SCALE;
        if hero_mystery_cents > remaining_mystery_cents {
            continue;
        }

        current_envelopes.push(envelope.payout_cents);
        current_mystery.push(hero_mystery_cents);
        search_allocations(
            share_index + 1,
            remaining_mystery_cents - hero_mystery_cents,
            hero_ko_shares,
            mystery_envelopes,
            current_envelopes,
            current_mystery,
            allocations,
        );
        current_envelopes.pop();
        current_mystery.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BigKoDecodeStatus, HeroKoShare, MysteryEnvelope, decode_big_ko_allocations,
    };

    #[test]
    fn decodes_single_exact_knockout() {
        let result = decode_big_ko_allocations(
            10_500,
            &[HeroKoShare { share_micros: 1_000_000 }],
            &[
                MysteryEnvelope {
                    sort_order: 1,
                    payout_cents: 10_500,
                    frequency_per_100m: 100,
                },
                MysteryEnvelope {
                    sort_order: 2,
                    payout_cents: 5_000,
                    frequency_per_100m: 3_600_000,
                },
            ],
        );

        assert_eq!(result.status, BigKoDecodeStatus::Exact);
        assert_eq!(result.allocations.len(), 1);
        assert_eq!(result.allocations[0].envelope_payout_cents, vec![10_500]);
        assert_eq!(result.allocations[0].hero_mystery_cents, vec![10_500]);
    }

    #[test]
    fn decodes_split_knockout_from_shared_envelope() {
        let result = decode_big_ko_allocations(
            5_250,
            &[HeroKoShare { share_micros: 500_000 }],
            &[MysteryEnvelope {
                sort_order: 1,
                payout_cents: 10_500,
                frequency_per_100m: 100,
            }],
        );

        assert_eq!(result.status, BigKoDecodeStatus::Exact);
        assert_eq!(result.allocations[0].hero_mystery_cents, vec![5_250]);
    }

    #[test]
    fn marks_multiple_valid_paths_as_ambiguous() {
        let result = decode_big_ko_allocations(
            5_000,
            &[
                HeroKoShare { share_micros: 1_000_000 },
                HeroKoShare { share_micros: 1_000_000 },
            ],
            &[
                MysteryEnvelope {
                    sort_order: 1,
                    payout_cents: 3_700,
                    frequency_per_100m: 3_800_000,
                },
                MysteryEnvelope {
                    sort_order: 2,
                    payout_cents: 2_500,
                    frequency_per_100m: 4_000_000,
                },
                MysteryEnvelope {
                    sort_order: 3,
                    payout_cents: 1_300,
                    frequency_per_100m: 33_618_140,
                },
            ],
        );

        assert_eq!(result.status, BigKoDecodeStatus::Ambiguous);
        assert!(result.allocations.len() >= 2);
    }

    #[test]
    fn marks_impossible_totals_as_infeasible() {
        let result = decode_big_ko_allocations(
            1_100,
            &[HeroKoShare { share_micros: 1_000_000 }],
            &[
                MysteryEnvelope {
                    sort_order: 1,
                    payout_cents: 600,
                    frequency_per_100m: 28_477_360,
                },
                MysteryEnvelope {
                    sort_order: 2,
                    payout_cents: 1_300,
                    frequency_per_100m: 33_618_140,
                },
            ],
        );

        assert_eq!(result.status, BigKoDecodeStatus::Infeasible);
        assert!(result.allocations.is_empty());
    }

    #[test]
    fn treats_zero_mystery_without_knockouts_as_zero_case() {
        let result = decode_big_ko_allocations(0, &[], &[]);

        assert_eq!(result.status, BigKoDecodeStatus::ZeroMystery);
        assert!(result.allocations.is_empty());
    }
}
