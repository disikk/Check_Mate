# Committed Pack Syntax Catalog

## Scope
- Corpus: committed GG pack in `backend/fixtures/mbr` (`9 HH + 9 TS`).
- Goal: enumerate every currently observed line family and pin the expected parser-worker issue severity.

## Severity contract
- `none`: the line family is part of the accepted committed syntax surface and must not materialize a parse issue.
- `warning`: fallback classification for unexpected non-seat lines that survive parsing as `unparsed_line`.
- `error`: parser-worker structural reconciliation issue after parsing, used when a parsed entity cannot be attached to a seat row.

## Tournament Summary (`TS`)

| Line family | Observed shape | Handler | Expected issue severity |
| --- | --- | --- | --- |
| Title | `Tournament #271770266, Mystery Battle Royale $25, Hold'em No Limit` | `parse_tournament_summary` title split | `none` |
| Buy-in | `Buy-in: $12.5+$2+$10.5` / `Buy-in: $12.5 + $2 + $10.5` | `parse_tournament_summary` buy-in parser | `none` |
| Entrants | `18 Players` | `parse_tournament_summary` entrants line | `none` |
| Prize pool | `Total Prize Pool: $414` | `parse_tournament_summary` prize-pool line | `none` |
| Started timestamp | `Tournament started 2026/03/16 10:44:11` | `parse_tournament_summary` started line | `none` |
| Result | `1st : Hero, $205` | `parse_tournament_summary` result regex | `none` |
| Tail prose | `You finished the tournament in 1st place.` / `You received a total of $205.` | `parse_tournament_summary` confirmation parser | `none` |

## Hand History (`HH`)

| Line family | Observed shape | Handler | Expected issue severity |
| --- | --- | --- | --- |
| Hand header | `Poker Hand #BR1064992721: Tournament #...` | `parse_hand_header` header regex | `none` |
| Table header | `Table '52' 5-max Seat #1 is the button` | `parse_hand_header` table regex | `none` |
| Seat row | `Seat 7: Hero (16,008 in chips)` | `parse_seat_line` | `none` |
| Hole-card section marker | `*** HOLE CARDS ***` | `parse_canonical_hand` street state machine | `none` |
| Hero dealt line | `Dealt to Hero [Kc Ad]` | `parse_dealt_to_line` | `none` |
| Hidden dealt line | `Dealt to 5d455a01` | `parse_hidden_dealt_to_line` | `none` |
| Flop transition | `*** FLOP *** [7c 4s 3h]` | `parse_board_transition` | `none` |
| Turn transition | `*** TURN *** [7c 4s 3h] [Th]` | `parse_board_transition` | `none` |
| River transition | `*** RIVER *** [7c 4s 3h Th] [As]` | `parse_board_transition` | `none` |
| Showdown marker | `*** SHOWDOWN ***` | `parse_canonical_hand` street state machine | `none` |
| Summary marker | `*** SUMMARY ***` | `parse_canonical_hand` street state machine | `none` |
| Summary total | `Total pot 3,984 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0` | `parse_summary_total_line` | `none` |
| Summary board | `Board [7d 2s 8h 2c Kh]` | `parse_summary_board_line` | `none` |
| Summary seat outcome prose | `Seat 7: Hero (big blind) showed [Qh Kh] and lost with a pair of Kings` | `parse_summary_seat_outcome_line` -> `core.hand_summary_results` | `none` |
| Summary seat outcome prose, plain lost | `Seat 6: VillainE lost` | `parse_summary_seat_outcome_line` -> `core.hand_summary_results` | `none` |
| Summary seat outcome prose, plain mucked | `Seat 7: VillainF mucked` | `parse_summary_seat_outcome_line` -> `core.hand_summary_results` | `none` |
| Forced ante | `Hero: posts the ante 60` | `parse_player_action_line` -> `PostAnte` | `none` |
| Forced small blind | `Hero: posts small blind 100` | `parse_player_action_line` -> `PostSb` | `none` |
| Forced big blind | `Hero: posts big blind 200` | `parse_player_action_line` -> `PostBb` | `none` |
| Fold | `Hero: folds` | `parse_player_action_line` -> `Fold` | `none` |
| Check | `Hero: checks` | `parse_player_action_line` -> `Check` | `none` |
| Call | `Hero: calls 300` / `Hero: calls 379 and is all-in` | `parse_player_action_line` -> `Call` | `none` |
| Bet | `Hero: bets 73` / `Hero: bets 945 and is all-in` | `parse_player_action_line` -> `Bet` | `none` |
| Raise-to | `Hero: raises 1,512 to 1,912 and is all-in` | `parse_player_action_line` -> `RaiseTo` | `none` |
| Uncalled return | `Uncalled bet (521) returned to 24f4df94` | `parse_uncalled_return` | `none` |
| Show line | `Hero: shows [8h 8d] (three of a kind, Eights)` | `parse_show_line` | `none` |
| Show line, partial reveal | `43b06066: shows [5d] (a pair of Fives)` | `parse_show_line` + `partial_reveal_show_line` marker | `warning` |
| Collect line | `Hero collected 1,754 from pot` | `parse_collect_line` | `none` |

## Boundary classifications at parser-worker persistence

| Condition | Example | Classification |
| --- | --- | --- |
| Unknown non-seat line survives parsing | `unparsed_line: Dealer note: ...` | `severity=warning`, `code=unparsed_line` |
| Non-warning parser message survives parsing | implementation-defined free-form warning | `severity=warning`, `code=parser_warning` |
| Malformed summary seat line survives parsing | `unparsed_summary_seat_line: Seat 9: VillainX (button) ???` | `severity=warning`, `code=unparsed_summary_seat_line` |
| Partial reveal show line is tolerated but explicit | `partial_reveal_show_line: 43b06066: shows [5d] (a pair of Fives)` | `severity=warning`, `code=partial_reveal_show_line` |
| Partial reveal summary show surface is tolerated but explicit | parsed summary showed line with fewer than 2 cards | `severity=warning`, `code=partial_reveal_summary_show_surface` |
| Tournament summary tail finish conflicts with result line | result line says `1st`, tail says `2nd` | `severity=warning`, `code=ts_tail_finish_place_mismatch` |
| Tournament summary tail payout conflicts with result line | result line says `$205`, tail says `$204` | `severity=warning`, `code=ts_tail_total_received_mismatch` |
| Summary seat outcome seat mismatch | parsed summary outcome seat number maps to a different canonical seat player | `severity=error`, `code=summary_seat_outcome_seat_mismatch` |
| Summary seat outcome cannot attach to seat row | parsed summary outcome for unknown seat | `severity=error`, `code=summary_seat_outcome_missing_seat` |
| Hero hole cards reference missing seat | parsed hero cards with no matching seat row | `severity=error`, `code=hero_cards_missing_seat` |
| Showdown cards reference missing seat | parsed showdown hand with no matching seat row | `severity=error`, `code=showdown_player_missing_seat` |
| Action references missing seat | parsed action for unknown player | `severity=error`, `code=action_player_missing_seat` |
