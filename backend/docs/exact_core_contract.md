# Exact Core Contract

## Статус

Этот документ freeze-ит текущий exact-core контракт `tracker_parser_core` по состоянию на 2026-03-27. Он описывает уже существующее поведение parser/normalizer/pot-resolution слоя и не считается redesign-документом для будущих фаз.

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
- typed parser issue surface;
- normalized hand surface;
- chip/pot invariants;
- pot slicing / eligibility / winner resolution;
- actor-order / legality;
- forced all-in и return-uncalled;
- terminal snapshot surface;
- текущую `KO semantics v2`;
- uncertainty / inconsistent contract.

Не входят в этот freeze:
- полный `summary seat-result` grammar hardening;
- `position_index` / `position_label` split;
- pot-level evidence graph;
- `KO semantics v2`;
- typed uncertainty reasons вне уже materialized invariant/settlement issue enums.

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
- `parse_issues`

`TournamentSummary` тоже использует тот же parser-level issue surface:
- `parse_issues`

`parse_issues` является typed parser/import-boundary contract:
- `severity`
- стабильный machine-readable `code`
- human-readable `message`
- optional `raw_line`
- optional structured `payload`

Parser-layer issues не поднимаются в `NormalizedHand`; normalizer работает поверх facts из `CanonicalParsedHand`, а не поверх дублированного warning-list.

### Выход

`NormalizedHand` является exact-core replay result. Ключевые поля:
- `snapshot`
- `settlement`
- `returns`
- `actual`
- `eliminations`
- `invariants`

`settlement` является новым canonical surface для pot-resolution:
- `certainty_state`
- `issues`
- `evidence`
- `pots[*].contributions`
- `pots[*].eligibilities`
- `pots[*].candidate_allocations`
- `pots[*].selected_allocation`
- `pots[*].issues`

## Инварианты

### 1. Chip conservation

Правило:
- сумма стартовых стеков должна совпадать с суммой финальных стеков `stacks_after_actual`;
- при нарушении `chip_conservation_ok = false`;
- причина materialize-ится в `invariants.issues` с code `chip_conservation_mismatch`.

Защищающие тесты:
- `captures_terminal_all_in_snapshot_with_exact_pot_and_stacks`
- `handles_uncalled_return_without_creating_fake_snapshot`
- `keeps_full_pack_invariants_green_for_all_committed_hands`

### 2. Pot conservation

Правило:
- `sum(committed_total_by_player) == sum(winner_collections) + rake_amount`;
- `winner_collections` materialize-ятся из best-effort observed payouts: сначала `collect`, а если их нет — из summary `won/collected` amounts;
- mismatch не маскируется и уходит в `invariants.issues` с code `pot_conservation_mismatch`;
- отдельный mismatch `summary_total_pot` против `collected + rake` уходит в `invariants.issues` с code `summary_total_pot_mismatch`.

Защищающие тесты:
- `captures_terminal_all_in_snapshot_with_exact_pot_and_stacks`
- `handles_uncalled_return_without_creating_fake_snapshot`
- `surfaces_unsatisfied_collect_mapping_as_invariant_error_without_guessing_winners`
- `keeps_full_pack_invariants_green_for_all_committed_hands`

### 3. Money-state safety

Правило:
- impossible debit/refund не имеет права мутировать money-state ни в parser annotation, ни в legality replay, ни в normalizer replay;
- для impossible debit materialize-ится `action_amount_exceeds_stack`;
- для impossible refund materialize-ятся `refund_exceeds_committed` и/или `refund_exceeds_betting_round_contrib`;
- если refund surface конфликтует с allowed overage, сохраняются существующие `uncalled_return_actor_mismatch` / `uncalled_return_amount_mismatch`, но money-state всё равно не мутируется;
- любая такая fail-safe ситуация переводит settlement в `Inconsistent` с code `replay_state_invalid` и убирает exact `selected_allocation`.

Защищающие тесты:
- `malformed_money_surface_enters_fail_safe_without_negative_outputs`
- `money_state::tests::rejects_debit_above_stack_without_mutating_balance`
- `money_state::tests::rejects_refund_above_committed_and_round_without_mutating_counters`
- `money_state::tests::rejects_refund_above_round_contrib_without_optional_committed_guards`

### 4. Pot slicing

Правило:
- банки строятся по лестнице distinct positive commitment levels;
- каждый новый pot равен `increment * number_of_contributors_at_level`;
- первый bank всегда `is_main = true`, остальные side pots;
- `settlement.pots[*].contributions` хранят вклад по срезу, а не только общий вклад игрока.

Защищающие тесты:
- `resolves_sidepot_ko_without_marking_hero_involved`
- `resolves_split_main_and_single_winner_side_from_showdown_ranks`
- `resolves_joint_ko_across_main_and_side_pots_with_different_winners`
- `resolves_odd_chip_split_from_collect_totals_without_guessing_bonus_chip`

### 5. Eligibility

Правило:
- eligible player обязан быть contributor соответствующего pot slice;
- folded player не может быть `settlement.pots[*].eligibilities`, но его ранее вложенные фишки остаются в `settlement.pots[*].contributions`;
- sit-out / eliminated seats не участвуют в active-order и не становятся live-eligibility участниками.

Защищающие тесты:
- `keeps_folded_contributor_in_pot_contributions_but_out_of_eligibility`
- `resolves_split_main_and_single_winner_side_from_showdown_ranks`
- `excludes_inactive_and_sitting_out_seats_from_position_facts`
- `excludes_sitting_out_seats_from_active_order`

### 6. Actor order и legality

Правило:
- position engine работает только по active seats;
- допустим active-count от 2 до 9;
- в HU preflop первым действует `BTN`, postflop первым действует `BB`;
- в multiway preflop opener считается по computed preflop order, postflop по computed postflop order;
- illegal actor order, non-reopen после short all-in, premature street close и fail-safe money guards surface-ятся как invariant issues, а не исправляются молча.

Защищающие тесты:
- `computes_position_facts_for_two_to_nine_active_seats`
- `excludes_inactive_and_sitting_out_seats_from_position_facts`
- `surfaces_illegal_heads_up_preflop_actor_order`
- `surfaces_illegal_heads_up_postflop_actor_order`
- `surfaces_non_reopening_short_all_in_reraise`
- `allows_reraise_after_full_raise_reopens_action`
- `surfaces_premature_street_close_when_pending_actor_is_skipped`

### 7. Forced all-in semantics

Правило:
- canonical action surface хранит `is_all_in`, `all_in_reason`, `forced_all_in_preflop`;
- exhausted ante/blind all-ins не зависят только от буквального текста `and is all-in`, а подтверждаются через стек/forced-post semantics;
- parser all-in annotation использует тот же safe debit/refund layer и не имеет права выводить exhausted-stack из rejected mutation;
- forced all-in является parser-level фактом, а не downstream догадкой.

Защищающие тесты:
- `annotates_forced_all_in_reasons_for_ante_and_blind_exhaustion`
- `handles_blind_exhausted_all_in_without_legality_errors`
- `handles_ante_exhausted_all_in_without_legality_errors`

### 8. Return-uncalled semantics

Правило:
- `ReturnUncalled` уменьшает `committed_total` и round contribution того же игрока;
- возврат возвращает фишки в стек и materialize-ится как `HandReturn { reason = "uncalled" }`;
- refund применяется только если одновременно проходят surface-guard по allowed overage и money-guard по committed/round contribution;
- при fail-safe refund остаётся наблюдаемым action/return surface, но money-state не мутируется;
- если contested terminal all-in node уже был зафиксирован до refund, последующий `ReturnUncalled` не отменяет этот snapshot;
- uncalled return не должен создавать terminal all-in snapshot "задним числом".

Защищающие тесты:
- `handles_uncalled_return_without_creating_fake_snapshot`
- `accepts_uncalled_return_after_failed_call_chain_without_legality_errors`

### 9. Terminal snapshot semantics

Правило:
- `snapshot` materialize-ится только для terminal all-in node;
- terminal node определяется state-based после применения текущего action, а не по типу самого action;
- snapshot возможен только если после action в contest остается минимум два игрока со статусами `Live | AllIn`;
- среди contestants должен быть хотя бы один `AllIn`, и на текущей улице не должно оставаться pending `Live` actor'ов;
- это покрывает both `final fold closes all-in contest` и `all contestants become all-in on closure action`;
- snapshot хранит известную доску на момент capture, число будущих board cards и per-player node state;
- обычный uncontested `bet -> fold -> return uncalled` snapshot не получает;
- отсутствие snapshot на non-terminal hand является нормальным exact поведением;

Защищающие тесты:
- `captures_terminal_all_in_snapshot_with_exact_pot_and_stacks`
- `fold_close_all_in_contest_captures_snapshot`
- `all_in_only_closure_captures_snapshot`
- `handles_uncalled_return_without_creating_fake_snapshot`
- `simple_bet_fold_does_not_create_snapshot`

### 10. Winner resolution, uncertainty и inconsistency

Правило:
- exact winners materialize-ятся только если settlement можно доказать по текущему evidence surface;
- observed payout evidence берется из aggregate `collect` totals или, если `collect` отсутствуют, из summary `won/collected` amounts;
- если `collect` и summary payout totals одновременно присутствуют, они обязаны совпадать; конфликт таких observed payouts считается `inconsistent`;
- ambiguous hidden-showdown cases не угадываются и уходят в `settlement.pots[*].issues`;
- contradictory observed payout distributions не угадываются и уходят в `settlement.issues`;
- odd-chip не опирается на room-level GG rule: observed odd-chip payout делает settlement `exact`, а недоказуемый odd-chip recipient оставляет settlement `uncertain`;
- exact winners materialize-ятся только через `settlement.pots[*].selected_allocation`; для `uncertain` и `inconsistent` случаев `selected_allocation = null`.

Семантика состояний:
- `Exact`: settlement доказан текущим surface.
- `Uncertain`: есть допустимые ambiguity branches, exact settlement не доказан.
- `Inconsistent`: факты противоречат друг другу, нарушают арифметику или были принудительно переведены в fail-safe через `replay_state_invalid`.
- `Estimated`: зарезервирован enum-level, но для текущего hand-level settlement contract не является рабочим happy-path состоянием.

Защищающие тесты:
- `resolves_pot_winners_even_when_collect_lines_are_not_grouped_by_pot`
- `keeps_hidden_showdown_side_pot_ambiguity_uncertain_without_guessing_winners`
- `surfaces_collect_distribution_conflict_with_showdown_as_inconsistent`
- `surfaces_unsatisfied_collect_mapping_as_invariant_error_without_guessing_winners`
- `resolves_odd_chip_split_from_collect_totals_without_guessing_bonus_chip`
- `resolves_odd_chip_split_from_summary_amounts_when_collect_lines_are_absent`
- `treats_conflicting_odd_chip_collect_and_summary_evidence_as_inconsistent`
- `leaves_odd_chip_aggregate_ambiguity_unresolved_when_multiple_exact_allocations_fit`

### 11. KO semantics v2

`HandElimination` теперь является действующим canonical KO/elimination contract.

Что фиксируется сейчас:
- elimination создается для seat с положительным стартовым стеком и финальным стеком `0`;
- `pots_participated_by_busted` хранит все pot-ы, куда busted seat внес chips;
- `pots_causing_bust` хранит только те eligible lost pot-ы, после которых busted seat фактически приходит к стеку `0`;
- `last_busting_pot_no` — последний pot из `pots_causing_bust` и единственный источник KO-credit attribution;
- `ko_winner_set` хранит winners именно `last_busting_pot_no`;
- `ko_share_fraction_by_winner` хранит per-winner fractions относительно полного amount busting pot;
- `elimination_certainty_state` и `ko_certainty_state` разведены: elimination может быть exact даже когда KO exact не доказан.

Что этот контракт сознательно обещает:
- участие в pot и KO-credit не смешиваются;
- split KO делится пропорционально фактическим `share_amount`, а не по неявной эвристике;
- ambiguous/inconsistent settlement не стирает elimination, но и не заставляет угадывать `ko_winner_set`.

Защищающие тесты:
- `captures_terminal_all_in_snapshot_with_exact_pot_and_stacks`
- `resolves_split_ko_from_last_busting_pot_with_proportional_shares`
- `separates_participation_pots_from_bust_causing_pots_for_sidepot_ko`
- `uses_only_last_busting_pot_for_multi_pot_ko_credit`
- `keeps_hidden_showdown_side_pot_ambiguity_uncertain_without_guessing_winners`
- `surfaces_collect_distribution_conflict_with_showdown_as_inconsistent`

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
- `settlement.issues` / `settlement.pots[*].issues` не равны arithmetic inconsistency;
- downstream обязан учитывать difference между `Exact`, `Uncertain` и `Inconsistent`.

## Правило изменения контракта

Любое изменение parser/normalizer/pot-resolution semantics должно одновременно обновлять:
- этот документ;
- защищающие тесты;
- `CLAUDE.md`, если меняется архитектурный reference surface.
