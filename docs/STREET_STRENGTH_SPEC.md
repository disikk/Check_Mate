# STREET_STRENGTH_SPEC

## Status

This document is the active unversioned runtime contract for `derived.street_hand_strength`.

`docs/STREET_STRENGTH_V2_SPEC.md` remains in the repository only as a historical Phase 1 proof artifact. It is not the active persisted contract anymore.

## Scope

This contract covers only the persisted exact descriptor layer written into `derived.street_hand_strength`.

Included:
- canonical persisted columns and their meanings;
- made-hand and draw category vocabularies;
- split river missed-draw semantics;
- explicit deferred policy for nut fields.

Excluded:
- analytics feature registry integration;
- filter/runtime bucket layer;
- public UI wording;
- future nut-policy computation.

## Row materialization policy

Rows are materialized for:
- Hero on each reached postflop street;
- opponents whose hole cards are exact-known by showdown, backfilled across reached streets.

`street_strength` remains an exact descriptor layer. It does not invent guessed rows for unknown hole cards.

## Canonical persisted contract

### `best_hand_class`

Exact hand-rank class on the current street, produced by the made-hand evaluator (`high_card`, `pair`, `two_pair`, `trips`, `straight`, `flush`, `full_house`, `quads`, `straight_flush`).

### `best_hand_rank_value`

Exact rank ordering value within `best_hand_class`, used for deterministic ordering inside the same class.

### `made_hand_category`

Board-aware / hole-card-aware canonical made-hand category:

- `high_card`
- `board_pair_only`
- `underpair`
- `third_pair`
- `second_pair`
- `top_pair_weak`
- `top_pair_good`
- `top_pair_top`
- `overpair`
- `two_pair`
- `set`
- `trips`
- `straight`
- `flush`
- `full_house`
- `quads`
- `straight_flush`

### `draw_category`

Canonical strongest live draw category on the street:

- `combo_draw`
- `flush_draw`
- `double_gutshot`
- `open_ended`
- `gutshot`
- `backdoor_flush_only`
- `none`

Board-only straight completions do not qualify as player-specific draws and therefore do not enter this category layer.

### `overcards_count`

Count of live hole-card overcards versus the current board top card:

- `0`
- `1`
- `2`

### `has_air`

`true` only when the hand has no made-hand strength beyond `high_card`, no live canonical draw, and no meaningful overcard case that should be represented separately.

### `missed_flush_draw`

River-only marker for a previously live frontdoor flush draw that did not complete into `flush` or `straight_flush`, subject to the strong-made-hand suppression rule already frozen in Phase 1.

### `missed_straight_draw`

River-only marker for a previously live player-specific straight draw that did not complete into `straight` or `straight_flush`, subject to the same strong-made-hand suppression rule.

### `is_nut_hand`

Reserved canonical field for future nut-policy work. In the current contract it remains unavailable and is persisted as `NULL`.

### `is_nut_draw`

Reserved canonical field for future nut-policy work. In the current contract it remains unavailable and is persisted as `NULL`.

### `certainty_state`

Exactness state emitted by the descriptor evaluator. Current street-strength rows are expected to remain exact-known rows, but callers must read this field explicitly instead of inferring certainty from presence alone.

## Deferred nut-policy contract

`is_nut_hand` and `is_nut_draw` are intentionally deferred.

Contract:
- `NULL` means unavailable, not computed `false`;
- callers must not treat these fields as resolved booleans;
- future nut-policy work must define formal board-state rules before changing this behavior.

## Removed legacy surface

The active runtime contract no longer includes:

- `pair_strength`
- `has_flush_draw`
- `has_backdoor_flush_draw`
- `has_open_ended`
- `has_gutshot`
- `has_double_gutshot`
- `has_pair_plus_draw`
- `has_overcards`
- `has_missed_draw_by_river`
- `descriptor_version`

Any SQL, importer code, or downstream tooling must read the canonical fields above instead of these removed legacy columns.
