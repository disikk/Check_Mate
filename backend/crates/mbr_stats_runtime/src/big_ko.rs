use std::collections::{BTreeMap, BTreeSet};

const MAX_RECORDED_ALLOCATIONS: usize = 64;

use crate::split_bounty::project_split_bounty_share;

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

pub fn expected_hero_mystery_cents(
    share_micros: i64,
    mystery_envelopes: &[MysteryEnvelope],
) -> Option<f64> {
    let total_frequency = total_frequency_weight(mystery_envelopes)?;
    let weighted_sum = mystery_envelopes
        .iter()
        .map(|envelope| {
            crate::split_bounty::project_split_bounty_share(envelope.payout_cents, share_micros)
                .expected_cents
                * envelope.frequency_per_100m as f64
        })
        .sum::<f64>();

    Some(weighted_sum / total_frequency as f64)
}

pub fn expected_big_ko_bucket_probabilities(
    mystery_envelopes: &[MysteryEnvelope],
) -> BTreeMap<String, f64> {
    let Some(total_frequency) = total_frequency_weight(mystery_envelopes) else {
        return BTreeMap::new();
    };

    let mut probabilities = BTreeMap::new();
    for envelope in mystery_envelopes {
        let Some(bucket_key) = big_ko_bucket_key(envelope.sort_order) else {
            continue;
        };
        *probabilities.entry(bucket_key.to_string()).or_insert(0.0) +=
            envelope.frequency_per_100m as f64 / total_frequency as f64;
    }

    probabilities
}

pub fn posterior_big_ko_bucket_counts(
    mystery_money_cents: i64,
    hero_ko_shares: &[HeroKoShare],
    mystery_envelopes: &[MysteryEnvelope],
) -> BTreeMap<String, f64> {
    let mut counts = zero_bucket_counts();
    let decode = decode_big_ko_allocations(mystery_money_cents, hero_ko_shares, mystery_envelopes);
    if matches!(
        decode.status,
        BigKoDecodeStatus::Infeasible | BigKoDecodeStatus::ZeroMystery
    ) {
        return counts;
    }

    let envelope_lookup = mystery_envelopes
        .iter()
        .filter_map(|envelope| {
            big_ko_bucket_key(envelope.sort_order).map(|bucket| {
                (
                    envelope.payout_cents,
                    (bucket.to_string(), envelope.frequency_per_100m as f64),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();

    let allocation_weights = decode
        .allocations
        .iter()
        .map(|allocation| {
            allocation
                .envelope_payout_cents
                .iter()
                .try_fold(1.0_f64, |weight, payout| {
                    envelope_lookup
                        .get(payout)
                        .map(|(_, frequency)| weight * *frequency)
                })
        })
        .collect::<Vec<_>>();
    let total_weight = allocation_weights
        .iter()
        .flatten()
        .copied()
        .sum::<f64>();
    if total_weight <= 0.0 {
        return counts;
    }

    for (allocation, weight) in decode.allocations.iter().zip(allocation_weights.into_iter()) {
        let Some(weight) = weight else {
            continue;
        };
        let normalized_weight = weight / total_weight;
        for (index, payout_cents) in allocation.envelope_payout_cents.iter().enumerate() {
            let Some((bucket_key, _)) = envelope_lookup.get(payout_cents) else {
                continue;
            };
            let share = hero_ko_shares
                .get(index)
                .map(|share| share.share_micros as f64 / 1_000_000.0)
                .unwrap_or(0.0);
            if let Some(total) = counts.get_mut(bucket_key) {
                *total += normalized_weight * share;
            }
        }
    }

    counts
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
        let share_outcome = project_split_bounty_share(
            envelope.payout_cents,
            hero_ko_shares[share_index].share_micros,
        );

        for hero_mystery_cents in share_outcome.candidate_cents {
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
}

fn total_frequency_weight(mystery_envelopes: &[MysteryEnvelope]) -> Option<i64> {
    let total = mystery_envelopes
        .iter()
        .map(|envelope| envelope.frequency_per_100m)
        .sum::<i64>();
    (total > 0).then_some(total)
}

fn big_ko_bucket_key(sort_order: i32) -> Option<&'static str> {
    match sort_order {
        1 => Some("big_ko_x10000_count"),
        2 => Some("big_ko_x1000_count"),
        3 => Some("big_ko_x100_count"),
        4 => Some("big_ko_x10_count"),
        5 => Some("big_ko_x2_count"),
        6 => Some("big_ko_x1_5_count"),
        _ => None,
    }
}

fn zero_bucket_counts() -> BTreeMap<String, f64> {
    BTreeMap::from([
        ("big_ko_x1_5_count".to_string(), 0.0),
        ("big_ko_x2_count".to_string(), 0.0),
        ("big_ko_x10_count".to_string(), 0.0),
        ("big_ko_x100_count".to_string(), 0.0),
        ("big_ko_x1000_count".to_string(), 0.0),
        ("big_ko_x10000_count".to_string(), 0.0),
    ])
}

#[cfg(test)]
mod tests {
    use super::{
        BigKoDecodeStatus, HeroKoShare, MysteryEnvelope, decode_big_ko_allocations,
        expected_big_ko_bucket_probabilities, expected_hero_mystery_cents,
        posterior_big_ko_bucket_counts,
    };

    #[test]
    fn decodes_single_exact_knockout() {
        let result = decode_big_ko_allocations(
            10_500,
            &[HeroKoShare {
                share_micros: 1_000_000,
            }],
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
            &[HeroKoShare {
                share_micros: 500_000,
            }],
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
                HeroKoShare {
                    share_micros: 1_000_000,
                },
                HeroKoShare {
                    share_micros: 1_000_000,
                },
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
            &[HeroKoShare {
                share_micros: 1_000_000,
            }],
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
    fn keeps_ugly_cent_single_split_out_of_infeasible() {
        let result = decode_big_ko_allocations(
            333,
            &[HeroKoShare {
                share_micros: 333_333,
            }],
            &[MysteryEnvelope {
                sort_order: 1,
                payout_cents: 1_000,
                frequency_per_100m: 1,
            }],
        );

        assert_ne!(result.status, BigKoDecodeStatus::Infeasible);
        assert!(
            result
                .allocations
                .iter()
                .any(|allocation| allocation.hero_mystery_cents == vec![333])
        );
    }

    #[test]
    fn keeps_ugly_cent_three_way_paths_ambiguous_instead_of_infeasible() {
        let result = decode_big_ko_allocations(
            1_000,
            &[
                HeroKoShare {
                    share_micros: 333_333,
                },
                HeroKoShare {
                    share_micros: 333_333,
                },
                HeroKoShare {
                    share_micros: 333_333,
                },
            ],
            &[MysteryEnvelope {
                sort_order: 1,
                payout_cents: 1_000,
                frequency_per_100m: 1,
            }],
        );

        assert_eq!(result.status, BigKoDecodeStatus::Ambiguous);
        assert!(!result.allocations.is_empty());
    }

    #[test]
    fn treats_zero_mystery_without_knockouts_as_zero_case() {
        let result = decode_big_ko_allocations(0, &[], &[]);

        assert_eq!(result.status, BigKoDecodeStatus::ZeroMystery);
        assert!(result.allocations.is_empty());
    }

    #[test]
    fn computes_expected_hero_mystery_from_weighted_envelopes() {
        let expected = expected_hero_mystery_cents(
            1_000_000,
            &[
                MysteryEnvelope {
                    sort_order: 4,
                    payout_cents: 10_000,
                    frequency_per_100m: 1,
                },
                MysteryEnvelope {
                    sort_order: 5,
                    payout_cents: 2_000,
                    frequency_per_100m: 3,
                },
            ],
        );

        assert_eq!(expected, Some(4_000.0));
    }

    #[test]
    fn maps_big_ko_bucket_weights_from_reference_frequencies() {
        let probabilities = expected_big_ko_bucket_probabilities(&[
            MysteryEnvelope {
                sort_order: 4,
                payout_cents: 10_000,
                frequency_per_100m: 1,
            },
            MysteryEnvelope {
                sort_order: 5,
                payout_cents: 2_000,
                frequency_per_100m: 3,
            },
            MysteryEnvelope {
                sort_order: 8,
                payout_cents: 750,
                frequency_per_100m: 2,
            },
        ]);

        assert_eq!(probabilities["big_ko_x10_count"], 0.16666666666666666);
        assert_eq!(probabilities["big_ko_x2_count"], 0.5);
        assert!(!probabilities.contains_key("big_ko_x1_5_count"));
    }

    #[test]
    fn posterior_big_ko_bucket_counts_uses_exact_total_mystery_when_single_path_exists() {
        let counts = posterior_big_ko_bucket_counts(
            7_000,
            &[
                HeroKoShare {
                    share_micros: 1_000_000,
                },
                HeroKoShare {
                    share_micros: 500_000,
                },
            ],
            &[
                MysteryEnvelope {
                    sort_order: 4,
                    payout_cents: 10_000,
                    frequency_per_100m: 1,
                },
                MysteryEnvelope {
                    sort_order: 5,
                    payout_cents: 2_000,
                    frequency_per_100m: 3,
                },
            ],
        );

        assert_eq!(counts["big_ko_x2_count"], 1.0);
        assert_eq!(counts["big_ko_x10_count"], 0.5);
        assert_eq!(counts["big_ko_x1_5_count"], 0.0);
    }

    #[test]
    fn posterior_big_ko_bucket_counts_weights_ambiguous_paths_by_envelope_frequencies() {
        let counts = posterior_big_ko_bucket_counts(
            5_000,
            &[
                HeroKoShare {
                    share_micros: 1_000_000,
                },
                HeroKoShare {
                    share_micros: 1_000_000,
                },
            ],
            &[
                MysteryEnvelope {
                    sort_order: 4,
                    payout_cents: 3_000,
                    frequency_per_100m: 1,
                },
                MysteryEnvelope {
                    sort_order: 5,
                    payout_cents: 2_000,
                    frequency_per_100m: 3,
                },
                MysteryEnvelope {
                    sort_order: 6,
                    payout_cents: 2_500,
                    frequency_per_100m: 2,
                },
            ],
        );

        assert_eq!(counts["big_ko_x10_count"], 0.6);
        assert_eq!(counts["big_ko_x2_count"], 0.6);
        assert_eq!(counts["big_ko_x1_5_count"], 0.8);
    }
}
