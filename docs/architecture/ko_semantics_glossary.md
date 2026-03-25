# KO Semantics Glossary

## Purpose

Этот glossary замораживает термины для `F0-T2` и запрещает неявное смешение:

- KO event semantics;
- KO money semantics;
- uncertainty semantics;
- boundary/stage semantics.

До завершения `F1 -> F3` код и документация должны опираться на эти определения, а не на интуитивные трактовки.

## Non-Negotiable Rules

1. `ko_event` и `ko_money` никогда не делят один и тот же output key.
2. `hero_share_fraction` и похожие доли описывают event attribution, а не деньги, если это не оговорено отдельно.
3. `boundary_ko` не может считаться exact, пока `boundary_resolution_state` не exact.
4. `ft_stage_bucket` допустим только как вспомогательный bucket/debug surface, но не как canonical stage language.
5. `mystery_money_total` на уровне турнира и `posterior_big_ko` на уровне envelope allocation — разные сущности.

## Event Semantics

| Term | Definition | Not This | Current availability | Next dependency |
|---|---|---|---|---|
| `ko_event` | Один факт вылета одного busted player, в котором Hero получает положительную exact-proven долю KO attribution. | Не деньги, не multiplier, не envelope tier. | Частично доступно через `derived.hand_eliminations` и runtime feature `hero_exact_ko_event_count`. | `F2-T1` для чистого event-vs-money split. |
| `exact_ko_event` | `ko_event`, доказанный exact source-of-truth rows без ambiguity. | Не estimated KO и не boundary guess. | Доступно в узком foundation-layer. | Сохранить, но переименовать в explicit event family. |
| `split_ko_event` | `ko_event`, где elimination reward event делится между несколькими winners на уровне события. | Не деление денег по envelope tiers. | Частично доступно как split/event marker. | `F2-T2` для rounding policy и `F2-T1` для safe naming. |
| `sidepot_ko_event` | `ko_event`, где elimination attribution идёт через side pot, а не только main pot. | Не money-share по bounty. | Частично доступно как elimination provenance. | `F2-T1` для отдельной money semantics. |
| `event_count` | Счётчик KO events, а не payout money. | Не сумма денег и не доля mystery total. | Доступно как safe target family уже сейчас. | Может идти в first honest tranche после naming freeze. |
| `share_fraction_provenance` | Provenance поля доли события, например доля Hero в split KO event. | Не денежная доля bounty по умолчанию. | Есть foundation-level аналоги (`hero_share_fraction`). | `F2-T1` должен явно развести event-share и money-share. |

## Money Semantics

| Term | Definition | Not This | Current availability | Next dependency |
|---|---|---|---|---|
| `ko_money` | Денежная ценность KO reward, принадлежащая Hero. | Не KO event count. | В production-grade виде ещё не доступно. | `F2-T1`, `F2-T2`, `F3-T1`, `F3-T2`. |
| `regular_prize_money` | Exact money from the regular payout ladder, без mystery/KO части. | Не общий payout и не KO money. | Уже materialize-ится на tournament-entry уровне. | Может использоваться в first honest tranche сразу после freeze. |
| `mystery_money_total` | Exact tournament-level mystery payout total for Hero: `total_payout_money - regular_prize_money`. | Не per-event KO attribution и не envelope distribution. | Уже materialize-ится на tournament-entry уровне. | Может использоваться как tournament-level KO winnings proxy в first honest tranche. |
| `ko_money_realized` | Деньги, причинно привязанные к KO events без posterior inference. | Не `mystery_money_total` целиком, если event attribution не доказан. | Пока blocked. | `F2-T1` и `F2-T2`. |
| `ko_money_estimated` | Money surface, полученный через явную estimated/posterior model. | Не exact money. | Пока blocked. | `F3-T1` и `F3-T2`. |
| `money_share` | Денежная доля KO reward, выделенная конкретному event или winner. | Не event share fraction без rounding policy. | Split-case public persistence пока blocked, но `F2-T2` уже разрешает conservative `floor/ceil` interval adapter для Big KO feasibility. | `F3-T1` и `F3-T2`. |
| `big_ko` | Семейство stat-ов про распределение крупных KO rewards по multiplier buckets. | Не точный envelope path без posterior model. | Только исследовательский foundation helper. | `F3-T1`. |
| `posterior_big_ko` | Вероятностное распределение mass по envelope buckets для KO-money event. | Не greedy feasible-only decoder. | Пока blocked для public stats. | `F3-T1` и `F3-T2`. |

## Uncertainty Semantics

| Term | Definition | Not This | Rule |
|---|---|---|---|
| `exact` | Значение доказано directly from current source-of-truth rows. | Не “кажется безопасным” и не “эвристика обычно работает”. | Может питать exact-safe stats. |
| `estimated` | Значение вычислено через явно объявленную inference/prediction/pointer model. | Не hidden heuristic и не fake exact. | Должно иметь provenance и explanation surface. |
| `uncertain` | Система не может доказать exact и не имеет утверждённой estimate-model для честного числа. | Не 0 и не NULL “на удачу”. | Must block or surface uncertainty explicitly. |
| `coverage_limited_exact` | Exact on the covered subset, but denominator and scope ограничены доступным coverage. | Не full-history exact. | Coverage contract обязан быть виден пользователю/API. |

## Boundary and Stage Semantics

| Term | Definition | Not This | Current availability | Next dependency |
|---|---|---|---|---|
| `boundary_ko` | KO-related stat/event in the transition window between last rush hands and first exact FT hand. | Не final-table KO автоматически и не pre-FT KO автоматически. | Current `boundary_ko_*` fields are point-estimate placeholders only. | `F1-T1`, `F1-T2`, `F1-T3`, then `F2`. |
| `is_boundary_hand` | Formal predicate for a hand that belongs to the boundary-resolution set. | Не “последняя 5-max hand” by rule. | Пока отсутствует. | `F1-T1` и `F1-T3`. |
| `pre_ft` | Hand or event provably before the first exact FT hand and outside unresolved boundary ambiguity. | Не просто “not 9-max”. | Пока blocked for stats. | `F1-T1` и `F1-T2`. |
| `early_ft` | Exact FT state with `ft_players_remaining_exact in 6..9`, если не указано более узкое 7..9 правило. | Не generic FT bucket. | Пока blocked for stats. | `F1-T3`. |
| `stage_7_9`, `stage_6_9`, `stage_5_6`, `stage_4_5`, `stage_3_4`, `stage_2_3` | Formal stage predicates at hand grain, defined through exact player counts. | Не coarse `ft_stage_bucket`. | Пока blocked for canonical stats. | `F1-T3`. |

## Current Runtime Alignment

| Current term | Frozen meaning | Status |
|---|---|---|
| `hero_exact_ko_event_count` | Temporary per-hand proxy for KO event counts only. | `proxy` |
| `hero_split_ko_event_count` | Temporary per-hand proxy for split KO event counts only. | `proxy` |
| `hero_sidepot_ko_event_count` | Temporary per-hand proxy for sidepot KO event counts only. | `proxy` |
| `roi_pct` | Canonical ROI percent over summary-covered tournaments. | `mapped` |
| `avg_finish_place` | Canonical average finish place over summary-covered tournaments. | `mapped` |
| `final_table_reach_percent` | Proxy for the future tournament-helper FT reach metric. | `proxy` |
| `ft_stage_bucket` | Auxiliary bucket only; not canonical stage language. | `legacy_only` |
| `boundary_ko_min / ev / max` | Placeholder boundary v1 point-estimate outputs, not a valid uncertainty model. | `blocked` |

## Naming Policy

- Count metrics must carry event-oriented names when they describe events: `*_event_count`, `*_events_per_tournament`.
- Money metrics must carry money-oriented names when they describe payout value: `*_money_total`, `*_money_delta`, `*_percent`.
- Legacy bundle names may remain in mapping docs, but public-facing canonical keys should not hide whether the metric is event-count, money, ratio, or estimate.

## Consequences for Next Phases

- `F1` may introduce stage predicates and boundary helpers, but cannot redefine event vs money meanings.
- `F2` may split event and money surfaces in code, but must preserve these glossary definitions.
- `F2-T2` freezes the ugly-cent split adapter in `docs/architecture/ko_split_bounty_rounding_policy.md`; later phases may refine ranking, but not silently collapse interval money into fake exactness.
- `F3` may introduce posterior KO-money modeling, but must keep `mystery_money_total` exact and separate from inferred envelope allocations.
