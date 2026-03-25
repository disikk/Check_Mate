ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS ko_pot_resolution_type TEXT NOT NULL DEFAULT 'unresolved'
    CHECK (
        ko_pot_resolution_type IN (
            'unresolved',
            'main_pot_only',
            'side_pot_only',
            'multi_pot_joint'
        )
    );

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS money_share_model_state TEXT NOT NULL DEFAULT 'blocked_uncertain_event'
    CHECK (
        money_share_model_state IN (
            'not_applicable',
            'exact_single_winner',
            'blocked_split_policy',
            'blocked_uncertain_event'
        )
    );

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS money_share_exact_fraction NUMERIC(12, 6);

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS money_share_estimated_min_fraction NUMERIC(12, 6);

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS money_share_estimated_ev_fraction NUMERIC(12, 6);

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS money_share_estimated_max_fraction NUMERIC(12, 6);

UPDATE derived.hand_eliminations
SET
    ko_pot_resolution_type = CASE
        WHEN COALESCE(array_length(resolved_by_pot_nos, 1), 0) = 0 THEN 'unresolved'
        WHEN COALESCE(array_length(resolved_by_pot_nos, 1), 0) > 1 THEN 'multi_pot_joint'
        WHEN COALESCE(resolved_by_pot_no, 0) > 1 THEN 'side_pot_only'
        ELSE 'main_pot_only'
    END,
    money_share_model_state = CASE
        WHEN hero_involved IS NOT TRUE THEN 'not_applicable'
        WHEN certainty_state <> 'exact' THEN 'blocked_uncertain_event'
        WHEN COALESCE(ko_involved_winner_count, 0) = 1 THEN 'exact_single_winner'
        ELSE 'blocked_split_policy'
    END,
    money_share_exact_fraction = CASE
        WHEN hero_involved IS TRUE
         AND certainty_state = 'exact'
         AND COALESCE(ko_involved_winner_count, 0) = 1
        THEN COALESCE(hero_ko_share_total, hero_share_fraction)
        ELSE NULL
    END,
    money_share_estimated_min_fraction = NULL,
    money_share_estimated_ev_fraction = NULL,
    money_share_estimated_max_fraction = NULL;
