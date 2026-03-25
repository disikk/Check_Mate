# KO Split Bounty Rounding Policy

## Purpose

This document freezes the `F2-T2` split-bounty money policy for ugly-cent KO
splits. Its job is to keep valid split cases out of fake `Infeasible` states
without inventing an exact platform rounding rule that the current HH/TS data
cannot prove.

## Non-Negotiable Rules

1. Event share and money share are different surfaces.
2. A split KO is exact only when the money projection lands on integral cents
   without any rounding choice.
3. If the platform's odd-cent allocation rule is not proven, the runtime must
   keep a conservative interval instead of manufacturing fake exact money.
4. `big_ko` may use the interval as a feasibility adapter, but this does not
   turn the helper into a posterior decoder.

## Projection Contract

Given:

- `envelope_cents`
- `share_fraction`

the runtime computes the rational money projection:

- `expected_cents = envelope_cents * share_fraction`

### Case 1: Exact Integral Split

If `expected_cents` is already an integer cent value, the result is:

- `kind = exact_integral`
- `exact_cents = expected_cents`
- `min_cents = max_cents = expected_cents`
- `candidate_cents = [expected_cents]`

Example:

- `10500 * 0.5 = 5250` cents

### Case 2: Rounding-Sensitive Split

If `expected_cents` is not an integer cent value, the result is:

- `kind = estimated_floor_ceil_interval`
- `exact_cents = null`
- `min_cents = floor(expected_cents)`
- `max_cents = ceil(expected_cents)`
- `candidate_cents = [floor(expected_cents), ceil(expected_cents)]`

Example:

- `1000 * 0.333333 = 333.333` cents
- interval becomes `[333, 334]`

This is intentionally conservative. The current system does not claim to know
which winner receives the odd cent, or whether the room applies a stronger
platform-specific tie-break rule.

## Current Runtime Use

- `backend/crates/mbr_stats_runtime/src/split_bounty.rs` owns the adapter
  `project_split_bounty_share`.
- `backend/crates/mbr_stats_runtime/src/big_ko.rs` consumes
  `candidate_cents` instead of rejecting non-integral splits outright.
- Therefore a valid ugly-cent split may now stay:
  - feasible and exact after reconciliation with an exact observed tournament
    total, when only one candidate path survives;
  - ambiguous, when multiple floor/ceil combinations remain possible.

Important: this exactness is about the surviving allocation path under the
known total, not about proving a universal platform rounding law.

## Explicit Non-Goals

`F2-T2` does not implement:

- posterior weighting or ranked allocation mass;
- platform-specific odd-cent priority rules;
- split-case public `money_share` persistence as exact values;
- a public `posterior_big_ko` stat surface.

Those remain deferred to `F3`.
