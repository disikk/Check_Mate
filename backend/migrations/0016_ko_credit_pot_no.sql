-- F1-T1: Добавить ko_credit_pot_no в derived.hand_eliminations.
-- Правило GG MBR: bounty делится между winners последнего (highest) side pot,
-- содержащего chips busted player. ko_credit_pot_no фиксирует именно этот pot.
-- resolved_by_pot_nos остаётся как диагностический след всех задействованных pot'ов.

ALTER TABLE derived.hand_eliminations
    ADD COLUMN IF NOT EXISTS ko_credit_pot_no INT;

COMMENT ON COLUMN derived.hand_eliminations.ko_credit_pot_no IS
    'Highest pot_no from resolved_by_pot_nos — the pot whose winners receive KO-credit per GG MBR rules.';
