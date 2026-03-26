# Exact Core Contract

## Статус

Этот документ freeze-ит текущий exact-core контракт `tracker_parser_core` по состоянию на 2026-03-25. Он описывает уже существующее поведение parser/normalizer/pot-resolution слоя и не считается redesign-документом для будущих фаз.

## Проблема

До этого момента exact-core semantics были размазаны по:
- `backend/crates/tracker_parser_core/src/normalizer.rs`
- `backend/crates/tracker_parser_core/src/pot_resolution.rs`
- `backend/crates/tracker_parser_core/src/positions.rs`
- `backend/crates/tracker_parser_core/src/betting_rules.rs`
- набору `tracker_parser_core` тестов

Из-за этого часть invariants существовала как неявная договоренность реализации.

## Цель

Сделать current exact-core contract:
- явным;
- привязанным к конкретным структурам и модулям;
- защищенным тестами;
- пригодным как baseline для следующих задач из `docs/par_nor.md`.

## Scope

Этот контракт покрывает:
- canonical parsed hand surface;
- normalized hand surface;
- chip/pot invariants;
- pot slicing / eligibility / winner resolution;
- actor-order / legality;
- forced all-in и return-uncalled;
- terminal snapshot surface;
- текущую `KO semantics v1`;
- uncertainty / inconsistent contract.

Не входят в этот freeze:
- полный `summary seat-result` grammar hardening;
- `position_index` / `position_label` split;
- pot-level evidence graph;
- `KO semantics v2`;
- typed parser issues / typed uncertainty reasons.

## Канонический surface

### Вход

`CanonicalParsedHand` является parser-level source of truth для exact-core normalizer слоя. Ключевые поля:
- `seats`
- `actions`
- `summary_seat_outcomes`
- `collected_amounts`
- `board_final` / `summary_board`
- `summary_total_pot`
- `summary_rake_amount`
- `parse_warnings`

### Выход

`NormalizedHand` является exact-core replay result. Ключевые поля:
- `snapshot`
- `final_pots`
- `pot_contributions`
- `pot_eligibilities`
- `pot_winners`
- `returns`
- `actual`
- `eliminations`
- `invariants`

## Инварианты

### 1. Chip conservation

Правило:
- сумма стартовых стеков должна совпадать с суммой финальных стеков `stacks_after_actual`;
- при нарушении `chip_conservation_ok = false`;
- причина добавляется в `invariant_errors` как `chip_conservation_mismatch:*`.

Защищающие тесты:
- `captures_terminal_all_in_snapshot_with_exact_pot_and_stacks`
- `handles_uncalled_return_without_creating_fake_snapshot`
- `keeps_full_pack_invariants_green_for_all_committed_hands`

### 2. Pot conservation

Правило:
- `sum(committed_total_by_player) == sum(winner_collections) + rake_amount`;
- mismatch не маскируется и уходит в `pot_conservation_mismatch:*`;
- отдельный mismatch `summary_total_pot` против `collected + rake` уходит в `summary_total_pot_mismatch:*`.

Защищающие тесты:
- `captures_terminal_all_in_snapshot_with_exact_pot_and_stacks`
- `handles_uncalled_return_without_creating_fake_snapshot`
- `surfaces_unsatisfied_collect_mapping_as_invariant_error_without_guessing_winners`
- `keeps_full_pack_invariants_green_for_all_committed_hands`

### 3. Pot slicing

Правило:
- банки строятся по лестнице distinct positive commitment levels;
- каждый новый pot равен `increment * number_of_contributors_at_level`;
- первый bank всегда `is_main = true`, остальные side pots;
- `pot_contributions` хранят вклад по срезу, а не только общий вклад игрока.

Защищающие тесты:
- `resolves_sidepot_ko_without_marking_hero_involved`
- `resolves_split_main_and_single_winner_side_from_showdown_ranks`
- `resolves_joint_ko_across_main_and_side_pots_with_different_winners`
- `resolves_odd_chip_split_from_collect_totals_without_guessing_bonus_chip`

### 4. Eligibility

Правило:
- eligible player обязан быть contributor соответствующего pot slice;
- folded player не может быть `pot_eligibility`, но его ранее вложенные фишки остаются в `pot_contributions`;
- sit-out / eliminated seats не участвуют в active-order и не становятся live-eligibility участниками.

Защищающие тесты:
- `keeps_folded_contributor_in_pot_contributions_but_out_of_eligibility`
- `resolves_split_main_and_single_winner_side_from_showdown_ranks`
- `excludes_inactive_and_sitting_out_seats_from_position_facts`
- `excludes_sitting_out_seats_from_active_order`

### 5. Actor order и legality

Правило:
- position engine работает только по active seats;
- допустим active-count от 2 до 9;
- в HU preflop первым действует `BTN`, postflop первым действует `BB`;
- в multiway preflop opener считается по computed preflop order, postflop по computed postflop order;
- illegal actor order, non-reopen после short all-in и premature street close surface-ятся как invariant issues, а не исправляются молча.

Защищающие тесты:
- `computes_position_facts_for_two_to_nine_active_seats`
- `excludes_inactive_and_sitting_out_seats_from_position_facts`
- `surfaces_illegal_heads_up_preflop_actor_order`
- `surfaces_illegal_heads_up_postflop_actor_order`
- `surfaces_non_reopening_short_all_in_reraise`
- `allows_reraise_after_full_raise_reopens_action`
- `surfaces_premature_street_close_when_pending_actor_is_skipped`

### 6. Forced all-in semantics

Правило:
- canonical action surface хранит `is_all_in`, `all_in_reason`, `forced_all_in_preflop`;
- exhausted ante/blind all-ins не зависят только от буквального текста `and is all-in`, а подтверждаются через стек/forced-post semantics;
- forced all-in является parser-level фактом, а не downstream догадкой.

Защищающие тесты:
- `annotates_forced_all_in_reasons_for_ante_and_blind_exhaustion`
- `handles_blind_exhausted_all_in_without_legality_errors`
- `handles_ante_exhausted_all_in_without_legality_errors`

### 7. Return-uncalled semantics

Правило:
- `ReturnUncalled` уменьшает `committed_total` и round contribution того же игрока;
- возврат возвращает фишки в стек и materialize-ится как `HandReturn { reason = "uncalled" }`;
- uncalled return не должен создавать terminal all-in snapshot "задним числом".

Защищающие тесты:
- `handles_uncalled_return_without_creating_fake_snapshot`
- `accepts_uncalled_return_after_failed_call_chain_without_legality_errors`

### 8. Terminal snapshot semantics

Правило:
- `snapshot` materialize-ится только для terminal all-in node;
- snapshot хранит известную доску на момент capture, число будущих board cards и per-player node state;
- отсутствие snapshot на non-terminal hand является нормальным exact поведением;
- текущий критерий snapshot freeze-ится как current behavior, но `docs/par_nor.md` уже фиксирует follow-up задачу `P1-01` на его state-based hardening.

Защищающие тесты:
- `captures_terminal_all_in_snapshot_with_exact_pot_and_stacks`
- `handles_uncalled_return_without_creating_fake_snapshot`

### 9. Winner resolution, uncertainty и inconsistency

Правило:
- exact winners materialize-ятся только если settlement можно доказать по текущему evidence surface;
- ambiguous hidden-showdown cases не угадываются и уходят в `uncertain_reason_codes`;
- contradictory collect distributions не угадываются и уходят в `invariant_errors`;
- `pot_winners` не materialize-ятся ни для `uncertain`, ни для `inconsistent` случаев.

Семантика состояний:
- `Exact`: settlement доказан текущим surface.
- `Uncertain`: есть допустимые ambiguity branches, exact settlement не доказан.
- `Inconsistent`: факты противоречат друг другу или нарушают арифметику.
- `Estimated`: зарезервирован enum-level, но для текущего hand-level settlement contract не является рабочим happy-path состоянием.

Защищающие тесты:
- `resolves_pot_winners_even_when_collect_lines_are_not_grouped_by_pot`
- `keeps_hidden_showdown_side_pot_ambiguity_uncertain_without_guessing_winners`
- `surfaces_collect_distribution_conflict_with_showdown_as_inconsistent`
- `surfaces_unsatisfied_collect_mapping_as_invariant_error_without_guessing_winners`
- `resolves_odd_chip_split_from_collect_totals_without_guessing_bonus_chip`

### 10. KO semantics v1

Текущий `HandElimination` контракт является exact-core `v1`, а не финальной product KO semantics.

Что фиксируется сейчас:
- elimination создается для seat с положительным стартовым стеком и финальным стеком `0`;
- `resolved_by_pot_nos` хранит все pot-ы, релевантные bust outcome по текущему resolved settlement;
- `ko_involved_winners` хранит winners этих bust-relevant pots;
- `hero_ko_share_total` / `hero_share_fraction` отражают долю Hero в bust-relevant settled pots по текущему v1 contract;
- `joint_ko`, `is_split_ko`, `split_n`, `is_sidepot_based` являются exact-core descriptors текущей pot-based attribution модели.

Что этот контракт сознательно не обещает:
- что `ko_involved_winners` уже равны финальному `ko_winner_set` будущей v2 semantics;
- что pot share автоматически равен KO money share;
- что multi-pot bust attribution уже является окончательной доменной моделью bounty/KO economics.

Защищающие тесты:
- `captures_terminal_all_in_snapshot_with_exact_pot_and_stacks`
- `resolves_split_ko_with_exact_hero_share_fraction`
- `resolves_sidepot_ko_without_marking_hero_involved`
- `resolves_joint_ko_across_main_and_side_pots_with_different_winners`

## KO semantics v2 target

Следующая версия контракта должна явно развести:
- `pots_participated_by_busted`
- `pots_causing_bust`
- `ko_winner_set`
- `ko_share_fraction`
- money-share semantics

До реализации `P1-03` и связанных задач:
- v2 считается целевым, но не действующим контрактом;
- downstream не должен трактовать v1 fields как финальную KO-money truth.

## Позиционный контракт: текущий surface

Текущий persisted surface использует `position_index`, `position_label`, `preflop_act_order_index`, `postflop_act_order_index`.

Freeze этого документа означает:
- `position_index` — это 1-based clockwise-from-button индекс только по active seats;
- `position_label` — это human-readable label, отделенный от machine-level index;
- label table зафиксирован для `2..=10` active players;
- в heads-up действует compact mapping `BTN` / `BB`: seat, который постит small blind, все равно имеет `position_label = BTN`;
- actor order (`preflop_act_order_index`, `postflop_act_order_index`) остается current source of truth для betting-order semantics и не заменяется `position_index`.

## Uncertainty contract

Current exact-core policy:
- лучше explicit uncertainty, чем guessed exactness;
- лучше explicit inconsistency, чем silent reconciliation;
- parser/normalizer может materialize-ить partial useful facts, но не должен дорисовывать exact winners без доказательства.

Практические следствия:
- `warnings` не равны invariant failure;
- `uncertain_reason_codes` не равны arithmetic inconsistency;
- downstream обязан учитывать difference между `Exact`, `Uncertain` и `Inconsistent`.

## Правило изменения контракта

Любое изменение parser/normalizer/pot-resolution semantics должно одновременно обновлять:
- этот документ;
- защищающие тесты;
- `CLAUDE.md`, если меняется архитектурный reference surface.
