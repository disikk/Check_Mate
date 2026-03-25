# STREET_STRENGTH_V2_SPEC

## Scope

This document freezes only Phase 1 of `street_strength_v2` for `TASK-SH-001..005`.

Included:
- player-specific straight-draw rules;
- board-only completion rules;
- frontdoor vs backdoor flush handling relevant to missed-draw semantics;
- conceptual `missed_flush_draw` / `missed_straight_draw` rules;
- explicit deferred policy for nut fields.

Excluded:
- full `made_hand_category_v2`;
- full `draw_category_v2`;
- versioned persistence redesign;
- feature registry, filter engine, UI/public bucket layer.

## Formal definitions

## 1. Player-specific straight draw

A straight draw is player-specific only if there exists at least one completion rank such that, after adding that rank to the current board, there exists a completed straight that:

1. uses at least one of the player's hole cards;
2. was not already complete on the current street before the completion rank was added.

If every valid completion rank produces only a board straight that does not use any hole card, the hand does not have a player-specific straight draw.

## 2. Board-only straight completion

A board-only straight completion is a completion rank that makes a straight on the combined card set but every such straight is composed entirely of board ranks.

Phase 1 rule:
- board-only completions must not set `has_open_ended`;
- board-only completions must not set `has_gutshot`;
- board-only completions must not set `has_double_gutshot`.

Phase 1 does not require a persisted diagnostic board-only flag.

## 3. Backdoor flush handling

`has_backdoor_flush_draw` is a flop-only diagnostic signal. It is true only when:

1. the hand is on the flop;
2. the player has at least one hole card of the relevant suit;
3. the current suited count is exactly three cards total including hole cards;
4. there is no live frontdoor flush draw on that street.

Backdoor-only history is not considered a meaningful missed draw by itself on the river.

## 4. `missed_flush_draw`

Conceptual `missed_flush_draw` is true on the river only if all conditions hold:

1. on a prior street, the hand had a frontdoor flush draw (`has_flush_draw = true`);
2. the river hand is not a flush or straight flush;
3. the river hand is not suppressed by the strong-made-hand rule below.

## 5. `missed_straight_draw`

Conceptual `missed_straight_draw` is true on the river only if all conditions hold:

1. on a prior street, the hand had a player-specific straight draw (`has_open_ended`, `has_gutshot`, or `has_double_gutshot`);
2. the river hand is not a straight or straight flush;
3. the river hand is not suppressed by the strong-made-hand rule below.

## 6. Strong-made-hand suppression rule

For Phase 1, missed-draw labeling is suppressed when the river hand class is:

- `two_pair`
- `straight`
- `flush`
- `full_house`
- `quads`
- `straight_flush`

This rule is intentionally narrow. A frontdoor draw that ends as one pair may still remain a missed draw in Phase 1.

## 7. Compatibility rule for v1 persistence

The current persisted surface remains `gg_mbr_street_strength_v1`.

Phase 1 does not add new missed-draw columns yet. Instead:

- conceptual `missed_flush_draw` and `missed_straight_draw` are defined here;
- persisted `has_missed_draw_by_river` is the compatibility projection:
  - `has_missed_draw_by_river = missed_flush_draw OR missed_straight_draw`.

This compatibility projection must not be driven by backdoor-only flush history.

## 8. Nut fields policy

Phase 1 explicitly does not implement nut logic.

Contract:
- `is_nut_hand` is unavailable;
- `is_nut_draw` is unavailable;
- the code-level availability policy is `deferred`;
- callers must not interpret `NULL` as "false";
- callers must not treat these fields as computed exact descriptors.

Nut policy requires a dedicated future batch with formal board-state rules and is out of scope for `TASK-SH-001..005`.
