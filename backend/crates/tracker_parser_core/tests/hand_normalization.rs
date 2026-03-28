use std::{fs, path::PathBuf};

use serde_json::{Value, json};
use tracker_parser_core::{
    models::{
        CertaintyState, FinalPot, InvariantIssue, NormalizedHand, PlayerStatus, PotContribution,
        PotEligibility, PotSettlementIssue, PotWinner, SettlementAllocationSource, Street,
    },
    normalizer::normalize_hand,
    parsers::hand_history::{parse_canonical_hand, split_hand_history},
};

const HH_FT: &str =
    include_str!("../../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");
const SPLIT_KO_HAND: &str = r#"Poker Hand #BRSPLIT1: Tournament #999001, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:00:00
Table '1' 9-max Seat #1 is the button
Seat 1: VillainA (1,000 in chips)
Seat 2: Hero (1,000 in chips)
Seat 3: VillainB (1,000 in chips)
VillainA: posts small blind 50
Hero: posts big blind 100
*** HOLE CARDS ***
Dealt to VillainA
Dealt to Hero [Tc 9c]
Dealt to VillainB
VillainB: calls 100
VillainA: calls 50
Hero: checks
*** FLOP *** [Ah Kd Qs]
VillainA: bets 900 and is all-in
Hero: calls 900 and is all-in
VillainB: calls 900 and is all-in
*** TURN *** [Ah Kd Qs] [Jc]
*** RIVER *** [Ah Kd Qs Jc] [2h]
*** SHOWDOWN ***
VillainA: shows [Qs Qd]
Hero: shows [Tc 9c]
VillainB: shows [Td 8d]
Hero collected 1,500 from pot
VillainB collected 1,500 from pot
*** SUMMARY ***
Total pot 3,000 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [Ah Kd Qs Jc 2h]
Seat 1: VillainA (small blind) showed [Qs Qd] and lost
Seat 2: Hero (big blind) showed [Tc 9c] and collected (1,500)
Seat 3: VillainB showed [Td 8d] and collected (1,500)"#;
const SIDEPOT_KO_HAND: &str = r#"Poker Hand #BRSIDE1: Tournament #999002, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:05:00
Table '2' 9-max Seat #1 is the button
Seat 1: Shorty (500 in chips)
Seat 2: Hero (1,000 in chips)
Seat 3: Medium (1,000 in chips)
Seat 4: BigStack (1,500 in chips)
Shorty: posts small blind 50
Hero: posts big blind 100
*** HOLE CARDS ***
Dealt to Shorty
Dealt to Hero [Ac Qc]
Dealt to Medium
Dealt to BigStack
Medium: calls 100
BigStack: raises 400 to 500
Shorty: calls 450 and is all-in
Hero: folds
Medium: raises 500 to 1,000 and is all-in
BigStack: calls 500
*** FLOP *** [2h 7d Tc]
*** TURN *** [2h 7d Tc] [3s]
*** RIVER *** [2h 7d Tc 3s] [4d]
*** SHOWDOWN ***
Medium: shows [Jh Jc]
BigStack: shows [As Ad]
BigStack collected 400 from pot
BigStack collected 1,200 from pot
BigStack collected 1,000 from pot
*** SUMMARY ***
Total pot 2,600 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2h 7d Tc 3s 4d]
Seat 1: Shorty (small blind) lost
Seat 2: Hero (big blind) folded before Flop
Seat 3: Medium showed [Jh Jc] and lost
Seat 4: BigStack showed [As Ad] and collected (2,600)"#;
const REORDERED_COLLECT_SIDE_POT_HAND: &str = r#"Poker Hand #BRSIDE2: Tournament #999003, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:10:00
Table '2' 9-max Seat #1 is the button
Seat 1: Shorty (500 in chips)
Seat 2: Hero (1,000 in chips)
Seat 3: Medium (1,000 in chips)
Seat 4: BigStack (1,500 in chips)
Shorty: posts small blind 50
Hero: posts big blind 100
*** HOLE CARDS ***
Dealt to Shorty
Dealt to Hero [Ac Qc]
Dealt to Medium
Dealt to BigStack
Medium: calls 100
BigStack: raises 400 to 500
Shorty: calls 450 and is all-in
Hero: folds
Medium: raises 500 to 1,000 and is all-in
BigStack: calls 500
*** FLOP *** [2h 7d Tc]
*** TURN *** [2h 7d Tc] [3s]
*** RIVER *** [2h 7d Tc 3s] [4d]
*** SHOWDOWN ***
Medium: shows [Jh Jc]
BigStack: shows [As Ad]
BigStack collected 1,000 from pot
BigStack collected 400 from pot
BigStack collected 1,200 from pot
*** SUMMARY ***
Total pot 2,600 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2h 7d Tc 3s 4d]
Seat 1: Shorty (small blind) lost
Seat 2: Hero (big blind) folded before Flop
Seat 3: Medium showed [Jh Jc] and lost
Seat 4: BigStack showed [As Ad] and collected (2,600)"#;
const AMBIGUOUS_COLLECT_HAND: &str = r#"Poker Hand #BRAMBIG1: Tournament #999004, Mystery Battle Royale $25 Hold'em No Limit - Level1(0/0(100)) - 2026/03/16 12:15:00
Table '3' 4-max Seat #1 is the button
Seat 1: ShortyA (100 in chips)
Seat 2: ShortyB (100 in chips)
Seat 3: Hero (300 in chips)
Seat 4: Villain (300 in chips)
ShortyA: posts the ante 100
ShortyB: posts the ante 100
Hero: posts the ante 100
Villain: posts the ante 100
*** HOLE CARDS ***
Dealt to ShortyA
Dealt to ShortyB
Dealt to Hero [Ah Ad]
Dealt to Villain
Hero: bets 200 and is all-in
Villain: calls 200 and is all-in
Hero: shows [Ah Ad]
Villain: shows [Ks Kd]
*** SHOWDOWN ***
Hero collected 400 from pot
Villain collected 400 from pot
*** SUMMARY ***
Total pot 800 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: ShortyA lost
Seat 2: ShortyB lost
Seat 3: Hero showed [Ah Ad] and collected (400)
Seat 4: Villain showed [Ks Kd] and collected (400)"#;
const UNSATISFIED_COLLECT_HAND: &str = r#"Poker Hand #BRBROKEN1: Tournament #999005, Mystery Battle Royale $25 Hold'em No Limit - Level1(0/0(100)) - 2026/03/16 12:20:00
Table '4' 2-max Seat #1 is the button
Seat 1: Hero (100 in chips)
Seat 2: Villain (100 in chips)
Hero: posts the ante 100
Villain: posts the ante 100
*** HOLE CARDS ***
Dealt to Hero [As Ac]
Dealt to Villain
Hero: shows [As Ac]
Villain: shows [Kd Kh]
*** SHOWDOWN ***
Hero collected 250 from pot
*** SUMMARY ***
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 3d 4h 5s 6c]
Seat 1: Hero showed [As Ac] and collected (250)
Seat 2: Villain showed [Kd Kh] and lost"#;
const SPLIT_MAIN_SINGLE_SIDE_HAND: &str = r#"Poker Hand #BRCM0501: Tournament #999050, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 13:40:00
Table '15' 3-max Seat #1 is the button
Seat 1: Hero (500 in chips)
Seat 2: Shorty (300 in chips)
Seat 3: Villain (500 in chips)
Shorty: posts small blind 50
Villain: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Tc 9c]
Dealt to Shorty
Dealt to Villain
Hero: raises 200 to 300
Shorty: calls 250 and is all-in
Villain: calls 200
*** FLOP *** [Ah Kd Qs]
Villain: checks
Hero: bets 200 and is all-in
Villain: calls 200 and is all-in
*** TURN *** [Ah Kd Qs] [Jc]
*** RIVER *** [Ah Kd Qs Jc] [2h]
*** SHOWDOWN ***
Hero: shows [Tc 9c]
Shorty: shows [Td 8d]
Villain: shows [As Ad]
Hero collected 400 from pot
Shorty collected 450 from pot
Hero collected 450 from pot
*** SUMMARY ***
Total pot 1,300 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [Ah Kd Qs Jc 2h]
Seat 1: Hero (button) showed [Tc 9c] and collected (850)
Seat 2: Shorty (small blind) showed [Td 8d] and collected (450)
Seat 3: Villain (big blind) showed [As Ad] and lost"#;
const JOINT_KO_MULTI_POT_HAND: &str = r#"Poker Hand #BRCM0601: Tournament #999060, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 14:00:00
Table '18' 3-max Seat #1 is the button
Seat 1: Hero (1,500 in chips)
Seat 2: Shorty (500 in chips)
Seat 3: Medium (1,000 in chips)
Shorty: posts small blind 50
Medium: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Dealt to Shorty
Dealt to Medium
Hero: raises 400 to 500
Shorty: calls 450 and is all-in
Medium: calls 400
*** FLOP *** [2c 7d 9h]
Medium: bets 500 and is all-in
Hero: calls 500
*** TURN *** [2c 7d 9h] [Qs]
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
Hero: shows [Ah Ad]
Shorty: shows [2h 2d]
Medium: shows [Kc Qc]
Shorty collected 1,500 from pot
Hero collected 1,000 from pot
*** SUMMARY ***
Total pot 2,500 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and collected (1,000)
Seat 2: Shorty (small blind) showed [2h 2d] and collected (1,500)
Seat 3: Medium (big blind) showed [Kc Qc] and lost"#;
const HIDDEN_SHOWDOWN_AMBIGUITY_HAND: &str = r#"Poker Hand #BRCM0502: Tournament #999051, Mystery Battle Royale $25 Hold'em No Limit - Level1(0/0(100)) - 2026/03/16 13:45:00
Table '16' 4-max Seat #1 is the button
Seat 1: ShortyA (100 in chips)
Seat 2: ShortyB (100 in chips)
Seat 3: Hero (300 in chips)
Seat 4: Villain (300 in chips)
ShortyA: posts the ante 100
ShortyB: posts the ante 100
Hero: posts the ante 100
Villain: posts the ante 100
*** HOLE CARDS ***
Dealt to ShortyA
Dealt to ShortyB
Dealt to Hero [Ah Ad]
Dealt to Villain
Hero: bets 200 and is all-in
Villain: calls 200 and is all-in
Hero: shows [Ah Ad]
*** SHOWDOWN ***
Hero collected 400 from pot
Villain collected 400 from pot
*** SUMMARY ***
Total pot 800 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: ShortyA lost
Seat 2: ShortyB lost
Seat 3: Hero showed [Ah Ad] and collected (400)
Seat 4: Villain collected (400)"#;
const ODD_CHIP_SPLIT_HAND: &str = r#"Poker Hand #BRCM0503: Tournament #999052, Mystery Battle Royale $25 Hold'em No Limit - Level1(0/0(1)) - 2026/03/16 13:50:00
Table '17' 3-max Seat #1 is the button
Seat 1: Hero (200 in chips)
Seat 2: Villain (200 in chips)
Seat 3: DeadMoney (1 in chips)
Hero: posts the ante 1
Villain: posts the ante 1
DeadMoney: posts the ante 1
*** HOLE CARDS ***
Dealt to Hero [Ah Kd]
Dealt to Villain
Dealt to DeadMoney
Villain: bets 199 and is all-in
Hero: calls 199 and is all-in
*** FLOP *** [2c 3d 4h]
*** TURN *** [2c 3d 4h] [5s]
*** RIVER *** [2c 3d 4h 5s] [6c]
*** SHOWDOWN ***
Hero: shows [Ah Kd]
Villain: shows [As Qd]
Hero collected 201 from pot
Villain collected 200 from pot
*** SUMMARY ***
Total pot 401 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 3d 4h 5s 6c]
Seat 1: Hero (button) showed [Ah Kd] and collected (201)
Seat 2: Villain showed [As Qd] and collected (200)
Seat 3: DeadMoney lost"#;
const ODD_CHIP_SUMMARY_ONLY_HAND: &str = r#"Poker Hand #BRCM0504: Tournament #999053, Mystery Battle Royale $25 Hold'em No Limit - Level1(0/0(1)) - 2026/03/16 13:55:00
Table '18' 3-max Seat #1 is the button
Seat 1: Hero (200 in chips)
Seat 2: Villain (200 in chips)
Seat 3: DeadMoney (1 in chips)
Hero: posts the ante 1
Villain: posts the ante 1
DeadMoney: posts the ante 1
*** HOLE CARDS ***
Dealt to Hero [Ah Kd]
Dealt to Villain
Dealt to DeadMoney
Villain: bets 199 and is all-in
Hero: calls 199 and is all-in
*** FLOP *** [2c 3d 4h]
*** TURN *** [2c 3d 4h] [5s]
*** RIVER *** [2c 3d 4h 5s] [6c]
*** SHOWDOWN ***
Hero: shows [Ah Kd]
Villain: shows [As Qd]
*** SUMMARY ***
Total pot 401 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 3d 4h 5s 6c]
Seat 1: Hero (button) showed [Ah Kd] and collected (201)
Seat 2: Villain showed [As Qd] and collected (200)
Seat 3: DeadMoney lost"#;
const ODD_CHIP_CONFLICTING_EVIDENCE_HAND: &str = r#"Poker Hand #BRCM0505: Tournament #999054, Mystery Battle Royale $25 Hold'em No Limit - Level1(0/0(1)) - 2026/03/16 14:00:00
Table '19' 3-max Seat #1 is the button
Seat 1: Hero (200 in chips)
Seat 2: Villain (200 in chips)
Seat 3: DeadMoney (1 in chips)
Hero: posts the ante 1
Villain: posts the ante 1
DeadMoney: posts the ante 1
*** HOLE CARDS ***
Dealt to Hero [Ah Kd]
Dealt to Villain
Dealt to DeadMoney
Villain: bets 199 and is all-in
Hero: calls 199 and is all-in
*** FLOP *** [2c 3d 4h]
*** TURN *** [2c 3d 4h] [5s]
*** RIVER *** [2c 3d 4h 5s] [6c]
*** SHOWDOWN ***
Hero: shows [Ah Kd]
Villain: shows [As Qd]
Hero collected 201 from pot
Villain collected 200 from pot
*** SUMMARY ***
Total pot 401 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 3d 4h 5s 6c]
Seat 1: Hero (button) showed [Ah Kd] and collected (200)
Seat 2: Villain showed [As Qd] and collected (201)
Seat 3: DeadMoney lost"#;
const ODD_CHIP_AMBIGUOUS_AGGREGATE_HAND: &str = r#"Poker Hand #BRCM0506: Tournament #999055, Mystery Battle Royale $25 Hold'em No Limit - Level1(0/0(1)) - 2026/03/16 14:05:00
Table '20' 5-max Seat #1 is the button
Seat 1: Hero (200 in chips)
Seat 2: Villain (200 in chips)
Seat 3: DeadA (2 in chips)
Seat 4: DeadB (1 in chips)
Seat 5: DeadC (1 in chips)
Hero: posts the ante 1
Villain: posts the ante 1
DeadA: posts the ante 1
DeadB: posts the ante 1 and is all-in
DeadC: posts the ante 1 and is all-in
*** HOLE CARDS ***
Dealt to Hero [Ah Kd]
Dealt to Villain
Dealt to DeadA
Dealt to DeadB
Dealt to DeadC
Villain: bets 1
DeadA: calls 1 and is all-in
Hero: calls 1
*** FLOP *** [2c 3d 4h]
*** TURN *** [2c 3d 4h] [5s]
*** RIVER *** [2c 3d 4h 5s] [6c]
*** SHOWDOWN ***
Hero: shows [Ah Kd]
Villain: shows [As Qd]
Hero collected 4 from pot
Villain collected 4 from pot
*** SUMMARY ***
Total pot 8 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 3d 4h 5s 6c]
Seat 1: Hero (button) showed [Ah Kd] and collected (4)
Seat 2: Villain showed [As Qd] and collected (4)
Seat 3: DeadA lost
Seat 4: DeadB lost
Seat 5: DeadC lost"#;
const HEADS_UP_PREFLOP_ILLEGAL_ORDER_HAND: &str = r#"Poker Hand #BRLEGAL2: Tournament #999006, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:25:00
Table '5' 2-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: Villain (1,000 in chips)
Hero: posts small blind 50
Villain: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [As Ac]
Dealt to Villain
Villain: checks
Hero: calls 50
*** FLOP *** [2c 7d 9h]
Villain: checks
Hero: checks
*** TURN *** [2c 7d 9h] [Js]
Villain: checks
Hero: checks
*** RIVER *** [2c 7d 9h Js] [3c]
Villain: checks
Hero: checks
*** SHOWDOWN ***
Hero: shows [As Ac]
Villain: shows [Kd Kh]
Hero collected 200 from pot
*** SUMMARY ***
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Js 3c]
Seat 1: Hero (button) showed [As Ac] and won (200)
Seat 2: Villain (big blind) showed [Kd Kh] and lost"#;
const HEADS_UP_POSTFLOP_ILLEGAL_ORDER_HAND: &str = r#"Poker Hand #BRLEGAL3: Tournament #999007, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:30:00
Table '6' 2-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: Villain (1,000 in chips)
Hero: posts small blind 50
Villain: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [As Ac]
Dealt to Villain
Hero: calls 50
Villain: checks
*** FLOP *** [2c 7d 9h]
Hero: bets 100
Villain: folds
Uncalled bet (100) returned to Hero
Hero collected 200 from pot
*** SUMMARY ***
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h]
Seat 1: Hero (button) won (200)
Seat 2: Villain (big blind) folded on the Flop"#;
const SHORT_ALL_IN_NON_REOPEN_HAND: &str = r#"Poker Hand #BRLEGAL4: Tournament #999008, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:35:00
Table '7' 3-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainA (1,000 in chips)
Seat 3: Shorty (400 in chips)
VillainA: posts small blind 50
Shorty: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Dealt to VillainA
Dealt to Shorty
Hero: raises 200 to 300
VillainA: calls 250
Shorty: raises 300 to 400 and is all-in
Hero: raises 400 to 800
VillainA: folds
Uncalled bet (400) returned to Hero
*** FLOP *** [2c 7d 9h]
*** TURN *** [2c 7d 9h] [Qs]
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
Hero: shows [Ah Ad]
Shorty: shows [Kd Kh]
Hero collected 1,100 from pot
*** SUMMARY ***
Total pot 1,100 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and won (1,100)
Seat 2: VillainA (small blind) folded before Flop
Seat 3: Shorty (big blind) showed [Kd Kh] and lost"#;
const FULL_RAISE_REOPEN_HAND: &str = r#"Poker Hand #BRLEGAL5: Tournament #999009, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:40:00
Table '8' 3-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainA (1,000 in chips)
Seat 3: Shorty (500 in chips)
VillainA: posts small blind 50
Shorty: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Dealt to VillainA
Dealt to Shorty
Hero: raises 200 to 300
VillainA: calls 250
Shorty: raises 400 to 500 and is all-in
Hero: raises 500 to 1,000 and is all-in
VillainA: folds
Uncalled bet (500) returned to Hero
*** FLOP *** [2c 7d 9h]
*** TURN *** [2c 7d 9h] [Qs]
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
Hero: shows [Ah Ad]
Shorty: shows [Kd Kh]
Hero collected 1,300 from pot
*** SUMMARY ***
Total pot 1,300 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and won (1,300)
Seat 2: VillainA (small blind) folded before Flop
Seat 3: Shorty (big blind) showed [Kd Kh] and lost"#;
const PREMATURE_STREET_CLOSE_HAND: &str = r#"Poker Hand #BRLEGAL6: Tournament #999010, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:45:00
Table '9' 3-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainA (1,000 in chips)
Seat 3: VillainB (1,000 in chips)
VillainA: posts small blind 50
VillainB: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Dealt to VillainA
Dealt to VillainB
Hero: calls 100
VillainA: folds
*** FLOP *** [2c 7d 9h]
VillainB: checks
Hero: checks
*** TURN *** [2c 7d 9h] [Qs]
VillainB: checks
Hero: checks
*** RIVER *** [2c 7d 9h Qs] [3c]
VillainB: checks
Hero: checks
*** SHOWDOWN ***
Hero: shows [Ah Ad]
VillainB: shows [Kd Kh]
Hero collected 250 from pot
*** SUMMARY ***
Total pot 250 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and won (250)
Seat 2: VillainA (small blind) folded before Flop
Seat 3: VillainB (big blind) showed [Kd Kh] and lost"#;
const LIMP_RAISE_CALL_CHAIN_HAND: &str = r#"Poker Hand #BRLEGAL7: Tournament #999011, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:50:00
Table '10' 3-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainA (1,000 in chips)
Seat 3: VillainB (1,000 in chips)
VillainA: posts small blind 50
VillainB: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Dealt to VillainA
Dealt to VillainB
Hero: calls 100
VillainA: calls 50
VillainB: raises 200 to 300
Hero: calls 200
VillainA: calls 200
*** FLOP *** [2c 7d 9h]
VillainA: checks
VillainB: checks
Hero: checks
*** TURN *** [2c 7d 9h] [Qs]
VillainA: checks
VillainB: checks
Hero: checks
*** RIVER *** [2c 7d 9h Qs] [3c]
VillainA: checks
VillainB: checks
Hero: checks
*** SHOWDOWN ***
Hero: shows [Ah Ad]
VillainA: shows [Kd Kh]
VillainB: shows [Qc Qd]
VillainB collected 900 from pot
*** SUMMARY ***
Total pot 900 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and lost
Seat 2: VillainA (small blind) showed [Kd Kh] and lost
Seat 3: VillainB (big blind) showed [Qc Qd] and won (900)"#;
const FAILED_CALL_CHAIN_RETURN_HAND: &str = r#"Poker Hand #BRLEGAL8: Tournament #999012, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:55:00
Table '11' 3-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainA (300 in chips)
Seat 3: VillainB (1,000 in chips)
VillainA: posts small blind 50
VillainB: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Dealt to VillainA
Dealt to VillainB
Hero: raises 900 to 1,000 and is all-in
VillainA: calls 250 and is all-in
VillainB: folds
Uncalled bet (700) returned to Hero
*** FLOP *** [2c 7d 9h]
*** TURN *** [2c 7d 9h] [Qs]
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
Hero: shows [Ah Ad]
VillainA: shows [Kd Kh]
Hero collected 700 from pot
*** SUMMARY ***
Total pot 700 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and won (700)
Seat 2: VillainA (small blind) showed [Kd Kh] and lost
Seat 3: VillainB (big blind) folded before Flop"#;
const FOLD_CLOSES_ALL_IN_CONTEST_HAND: &str = r#"Poker Hand #BRSNAPFOLD1: Tournament #999013, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 13:00:00
Table '12' 3-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainA (1,000 in chips)
Seat 3: VillainB (1,000 in chips)
VillainA: posts small blind 50
VillainB: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Dealt to VillainA
Dealt to VillainB
Hero: raises 900 to 1,000 and is all-in
VillainA: calls 950
VillainB: folds
*** FLOP *** [2c 7d 9h]
*** TURN *** [2c 7d 9h] [Qs]
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
Hero: shows [Ah Ad]
VillainA: shows [Kd Kh]
Hero collected 2,100 from pot
*** SUMMARY ***
Total pot 2,100 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and won (2,100)
Seat 2: VillainA (small blind) showed [Kd Kh] and lost
Seat 3: VillainB (big blind) folded before Flop"#;
const BLIND_EXHAUSTED_ALL_IN_HAND: &str = r#"Poker Hand #BRCM0405: Tournament #999205, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 13:20:00
Table '12' 2-max Seat #1 is the button
Seat 1: ShortBlind (50 in chips)
Seat 2: Hero (1,000 in chips)
ShortBlind: posts small blind 50
Hero: posts big blind 100
*** HOLE CARDS ***
Dealt to ShortBlind
Dealt to Hero [Ah Ad]
Uncalled bet (50) returned to Hero
*** FLOP *** [2c 7d 9h]
*** TURN *** [2c 7d 9h] [Qs]
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
ShortBlind: shows [Kd Kh]
Hero: shows [Ah Ad]
Hero collected 100 from pot
*** SUMMARY ***
Total pot 100 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: ShortBlind (button) showed [Kd Kh] and lost
Seat 2: Hero (big blind) showed [Ah Ad] and won (100)"#;
const ANTE_EXHAUSTED_ALL_IN_HAND: &str = r#"Poker Hand #BRCM0406: Tournament #999206, Mystery Battle Royale $25 Hold'em No Limit - Level1(0/0(100)) - 2026/03/16 13:25:00
Table '13' 2-max Seat #1 is the button
Seat 1: ShortAnte (100 in chips)
Seat 2: Hero (1,000 in chips)
ShortAnte: posts the ante 100
Hero: posts the ante 100
*** HOLE CARDS ***
Dealt to ShortAnte
Dealt to Hero [Ah Ad]
Hero: checks
*** FLOP *** [2c 7d 9h]
*** TURN *** [2c 7d 9h] [Qs]
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
ShortAnte: shows [Kd Kh]
Hero: shows [Ah Ad]
Hero collected 200 from pot
*** SUMMARY ***
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: ShortAnte (button) showed [Kd Kh] and lost
Seat 2: Hero showed [Ah Ad] and won (200)"#;
const SITTING_OUT_ACTIVE_ORDER_HAND: &str = r#"Poker Hand #BRCM0407: Tournament #999207, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 13:30:00
Table '14' 4-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: Sitout (1,000 in chips) is sitting out
Seat 3: VillainA (1,000 in chips)
Seat 4: VillainB (1,000 in chips)
VillainA: posts small blind 50
VillainB: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Dealt to VillainA
Dealt to VillainB
Hero: calls 100
VillainA: calls 50
VillainB: checks
*** FLOP *** [2c 7d 9h]
VillainA: checks
VillainB: checks
Hero: checks
*** TURN *** [2c 7d 9h] [Qs]
VillainA: checks
VillainB: checks
Hero: checks
*** RIVER *** [2c 7d 9h Qs] [3c]
VillainA: checks
VillainB: checks
Hero: checks
*** SHOWDOWN ***
Hero: shows [Ah Ad]
VillainA: shows [Kd Kh]
VillainB: shows [Qc Qd]
Hero collected 300 from pot
*** SUMMARY ***
Total pot 300 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and won (300)
Seat 3: VillainA (small blind) showed [Kd Kh] and lost
Seat 4: VillainB (big blind) showed [Qc Qd] and lost"#;

const HH_FIXTURE_FILES: &[&str] = &[
    "GG20260316-0307 - Mystery Battle Royale 25.txt",
    "GG20260316-0312 - Mystery Battle Royale 25.txt",
    "GG20260316-0316 - Mystery Battle Royale 25.txt",
    "GG20260316-0319 - Mystery Battle Royale 25.txt",
    "GG20260316-0323 - Mystery Battle Royale 25.txt",
    "GG20260316-0338 - Mystery Battle Royale 25.txt",
    "GG20260316-0342 - Mystery Battle Royale 25.txt",
    "GG20260316-0344 - Mystery Battle Royale 25.txt",
    "GG20260316-0351 - Mystery Battle Royale 25.txt",
];

#[test]
fn captures_terminal_all_in_snapshot_with_exact_pot_and_stacks() {
    let first_hand = HH_FT.split("\n\n").next().unwrap();
    let hand = parse_canonical_hand(first_hand).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let snapshot = normalized.snapshot.as_ref().expect("snapshot must exist");
    assert_eq!(snapshot.snapshot_street, Street::Preflop);
    assert_eq!(snapshot.snapshot_event_seq, 5);
    assert_eq!(snapshot.known_board_cards.len(), 0);
    assert_eq!(snapshot.future_board_cards_count, 5);
    assert_eq!(snapshot.pots.len(), 1);
    assert_eq!(snapshot.pots[0].amount, 3_984);
    assert_eq!(
        snapshot.pots[0].eligible_players,
        vec!["f02e54a6".to_string(), "Hero".to_string()]
    );

    let hero = snapshot
        .players
        .iter()
        .find(|player| player.player_name == "Hero")
        .unwrap();
    let villain = snapshot
        .players
        .iter()
        .find(|player| player.player_name == "f02e54a6")
        .unwrap();

    assert_eq!(hero.status, PlayerStatus::Live);
    assert_eq!(hero.stack_at_snapshot, 14_016);
    assert_eq!(hero.committed_total, 1_992);
    assert_eq!(villain.status, PlayerStatus::AllIn);
    assert_eq!(villain.stack_at_snapshot, 0);
    assert_eq!(villain.committed_total, 1_992);

    assert_eq!(
        normalized.actual.stacks_after_observed.get("Hero"),
        Some(&18_000)
    );
    assert_eq!(
        normalized.actual.stacks_after_observed.get("f02e54a6"),
        Some(&0)
    );
    assert_eq!(
        normalized.actual.observed_winner_collections.get("Hero"),
        Some(&3_984)
    );
    assert_eq!(
        normalized.actual.committed_total_by_player.get("Hero"),
        Some(&1_992)
    );
    assert_eq!(
        normalized.actual.committed_total_by_player.get("f02e54a6"),
        Some(&1_992)
    );
    assert_eq!(normalized.actual.rake_amount, 0);
    let final_pots = final_pots(&normalized);
    let pot_winners = pot_winners(&normalized);
    let pot_contributions = pot_contributions(&normalized);
    assert_eq!(final_pots.len(), 1);
    assert_eq!(final_pots[0].pot_no, 1);
    assert!(final_pots[0].is_main);
    assert_eq!(final_pots[0].amount, 3_984);
    assert!(normalized.returns.is_empty());
    assert_eq!(pot_contributions.len(), 2);
    assert_eq!(pot_winners.len(), 1);
    assert_eq!(pot_winners[0].pot_no, 1);
    assert_eq!(pot_winners[0].seat_no, 7);
    assert_eq!(pot_winners[0].player_name, "Hero");
    assert_eq!(pot_winners[0].share_amount, 3_984);
    assert_eq!(normalized.eliminations.len(), 1);
    assert_eq!(normalized.eliminations[0].eliminated_seat_no, 3);
    assert_eq!(
        normalized.eliminations[0].eliminated_player_name,
        "f02e54a6"
    );
    let elimination = elimination_json_by_player(&normalized, "f02e54a6");
    assert_eq!(elimination["pots_participated_by_busted"], json!([1]));
    assert_eq!(elimination["pots_causing_bust"], json!([1]));
    assert_eq!(elimination["last_busting_pot_no"], json!(1));
    assert_eq!(elimination["ko_winner_set"], json!(["Hero"]));
    assert_eq!(
        elimination["ko_share_fraction_by_winner"],
        json!([{
            "seat_no": 7,
            "player_name": "Hero",
            "share_fraction": 1.0
        }])
    );
    assert_eq!(elimination["elimination_certainty_state"], json!("exact"));
    assert_eq!(elimination["ko_certainty_state"], json!("exact"));
    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.issues.is_empty());
}

#[test]
fn materializes_unified_settlement_with_selected_allocation_for_exact_hand() {
    let first_hand = HH_FT.split("\n\n").next().unwrap();
    let hand = parse_canonical_hand(first_hand).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let settlement = &normalized.settlement;
    assert_eq!(settlement.certainty_state, CertaintyState::Exact);
    assert!(settlement.issues.is_empty());
    assert_eq!(settlement.evidence.collect_events_seen.len(), 1);
    assert_eq!(
        settlement.evidence.collect_events_seen[0].player_name,
        "Hero"
    );
    assert_eq!(settlement.evidence.collect_events_seen[0].amount, 3_984);
    assert_eq!(settlement.pots.len(), 1);

    let pot = &settlement.pots[0];
    assert_eq!(pot.pot_no, 1);
    assert_eq!(pot.amount, 3_984);
    assert!(pot.is_main);
    assert_eq!(pot.issues, Vec::<PotSettlementIssue>::new());
    assert_eq!(pot.contenders, vec!["Hero".to_string()]);
    assert_eq!(pot.candidate_allocations.len(), 1);
    assert_eq!(
        pot.candidate_allocations[0].source,
        SettlementAllocationSource::ShowdownRank
    );
    assert_eq!(
        pot.contributions
            .iter()
            .map(|contribution| (
                contribution.pot_no,
                contribution.player_name.as_str(),
                contribution.amount
            ))
            .collect::<Vec<_>>(),
        vec![(1, "f02e54a6", 1_992), (1, "Hero", 1_992)]
    );
    assert_eq!(
        pot.eligibilities
            .iter()
            .map(|eligibility| (eligibility.pot_no, eligibility.player_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "f02e54a6"), (1, "Hero")]
    );
    assert_eq!(
        pot.selected_allocation
            .as_ref()
            .expect("exact hand must have selected allocation")
            .shares
            .iter()
            .map(|share| (share.player_name.as_str(), share.share_amount))
            .collect::<Vec<_>>(),
        vec![("Hero", 3_984)]
    );
}

#[test]
fn exact_hand_serializes_separate_observed_and_exact_actual_layers() {
    let first_hand = HH_FT.split("\n\n").next().unwrap();
    let hand = parse_canonical_hand(first_hand).unwrap();
    let normalized = normalize_hand(&hand).unwrap();
    let actual = serde_json::to_value(&normalized.actual).unwrap();

    assert_eq!(actual.get("winner_collections"), None);
    assert_eq!(actual.get("stacks_after_actual"), None);
    assert_eq!(actual["observed_winner_collections"]["Hero"], json!(3_984));
    assert_eq!(actual["stacks_after_observed"]["Hero"], json!(18_000));
    assert_eq!(actual["exact_selected_payout_totals"]["Hero"], json!(3_984));
    assert_eq!(actual["stacks_after_exact"]["Hero"], json!(18_000));
}

#[test]
fn handles_uncalled_return_without_creating_fake_snapshot() {
    let second_hand = HH_FT.split("\n\n").nth(1).unwrap();
    let hand = parse_canonical_hand(second_hand).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.snapshot.is_none());
    assert_eq!(
        normalized.actual.stacks_after_observed.get("Hero"),
        Some(&16_008)
    );
    assert_eq!(
        normalized.actual.stacks_after_observed.get("f02e54a6"),
        Some(&1_992)
    );
    assert_eq!(
        normalized.actual.observed_winner_collections.get("Hero"),
        Some(&960)
    );
    assert_eq!(
        normalized.actual.committed_total_by_player.get("Hero"),
        Some(&480)
    );
    assert_eq!(
        normalized.actual.committed_total_by_player.get("f02e54a6"),
        Some(&480)
    );
    assert_eq!(normalized.actual.rake_amount, 0);
    let final_pots = final_pots(&normalized);
    let pot_winners = pot_winners(&normalized);
    assert_eq!(final_pots.len(), 1);
    assert_eq!(final_pots[0].amount, 960);
    assert_eq!(normalized.returns.len(), 1);
    assert_eq!(normalized.returns[0].seat_no, 7);
    assert_eq!(normalized.returns[0].player_name, "Hero");
    assert_eq!(normalized.returns[0].amount, 15_048);
    assert_eq!(normalized.returns[0].reason, "uncalled");
    assert_eq!(pot_winners.len(), 1);
    assert_eq!(pot_winners[0].player_name, "Hero");
    assert_eq!(pot_winners[0].share_amount, 960);
    assert!(normalized.eliminations.is_empty());
    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.issues.is_empty());
}

#[test]
fn fold_close_all_in_contest_captures_snapshot() {
    let hand = parse_canonical_hand(FOLD_CLOSES_ALL_IN_CONTEST_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let snapshot = normalized.snapshot.as_ref().expect("snapshot must exist");
    assert_eq!(snapshot.snapshot_street, Street::Preflop);
    assert_eq!(snapshot.snapshot_event_seq, 4);
    assert_eq!(snapshot.future_board_cards_count, 5);

    let hero = snapshot
        .players
        .iter()
        .find(|player| player.player_name == "Hero")
        .unwrap();
    let villain_a = snapshot
        .players
        .iter()
        .find(|player| player.player_name == "VillainA")
        .unwrap();
    let villain_b = snapshot
        .players
        .iter()
        .find(|player| player.player_name == "VillainB")
        .unwrap();

    assert_eq!(hero.status, PlayerStatus::AllIn);
    assert_eq!(villain_a.status, PlayerStatus::AllIn);
    assert_eq!(villain_b.status, PlayerStatus::Folded);
}

#[test]
fn all_in_only_closure_captures_snapshot() {
    let hand = parse_canonical_hand(SPLIT_KO_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let snapshot = normalized.snapshot.as_ref().expect("snapshot must exist");
    assert_eq!(snapshot.snapshot_street, Street::Flop);
    assert_eq!(snapshot.snapshot_event_seq, 7);
    assert_eq!(snapshot.future_board_cards_count, 2);
    assert!(
        snapshot
            .players
            .iter()
            .all(|player| matches!(player.status, PlayerStatus::AllIn)),
        "all contestants should be all-in at the snapshot"
    );
}

#[test]
fn simple_bet_fold_does_not_create_snapshot() {
    let hand = parse_canonical_hand(HEADS_UP_POSTFLOP_ILLEGAL_ORDER_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.snapshot.is_none());
}

#[test]
fn resolves_split_ko_from_last_busting_pot_with_proportional_shares() {
    let hand = parse_canonical_hand(SPLIT_KO_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let final_pots = final_pots(&normalized);
    let pot_winners = pot_winners(&normalized);
    assert_eq!(final_pots.len(), 1);
    assert_eq!(final_pots[0].amount, 3_000);
    assert_eq!(pot_winners.len(), 2);
    assert_eq!(normalized.eliminations.len(), 1);
    assert_eq!(
        normalized.eliminations[0].eliminated_player_name,
        "VillainA"
    );
    let elimination = elimination_json_by_player(&normalized, "VillainA");
    assert_eq!(elimination["pots_participated_by_busted"], json!([1]));
    assert_eq!(elimination["pots_causing_bust"], json!([1]));
    assert_eq!(elimination["last_busting_pot_no"], json!(1));
    assert_eq!(elimination["ko_winner_set"], json!(["Hero", "VillainB"]));
    assert_eq!(
        elimination["ko_share_fraction_by_winner"],
        json!([
            {
                "seat_no": 2,
                "player_name": "Hero",
                "share_fraction": 0.5
            },
            {
                "seat_no": 3,
                "player_name": "VillainB",
                "share_fraction": 0.5
            }
        ])
    );
    assert_eq!(elimination["elimination_certainty_state"], json!("exact"));
    assert_eq!(elimination["ko_certainty_state"], json!("exact"));
}

#[test]
fn separates_participation_pots_from_bust_causing_pots_for_sidepot_ko() {
    let hand = parse_canonical_hand(SIDEPOT_KO_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let final_pots = final_pots(&normalized);
    assert_eq!(final_pots.len(), 3);
    assert_eq!(final_pots[0].amount, 400);
    assert_eq!(final_pots[1].amount, 1_200);
    assert_eq!(final_pots[2].amount, 1_000);
    assert_eq!(normalized.returns.len(), 0);

    let medium = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "Medium")
        .unwrap();
    let medium = serde_json::to_value(medium).unwrap();
    assert_eq!(medium["pots_participated_by_busted"], json!([1, 2, 3]));
    assert_eq!(medium["pots_causing_bust"], json!([3]));
    assert_eq!(medium["last_busting_pot_no"], json!(3));
    assert_eq!(medium["ko_winner_set"], json!(["BigStack"]));
    assert_eq!(
        medium["ko_share_fraction_by_winner"],
        json!([{
            "seat_no": 4,
            "player_name": "BigStack",
            "share_fraction": 1.0
        }])
    );
    assert_eq!(medium["elimination_certainty_state"], json!("exact"));
    assert_eq!(medium["ko_certainty_state"], json!("exact"));
}

#[test]
fn keeps_folded_contributor_in_pot_contributions_but_out_of_eligibility() {
    let hand = parse_canonical_hand(SIDEPOT_KO_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(
        pot_contributions(&normalized)
            .iter()
            .map(|contribution| {
                (
                    contribution.pot_no,
                    contribution.player_name.as_str(),
                    contribution.amount,
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (1, "Shorty", 100),
            (1, "Hero", 100),
            (1, "Medium", 100),
            (1, "BigStack", 100),
            (2, "Shorty", 400),
            (2, "Medium", 400),
            (2, "BigStack", 400),
            (3, "Medium", 500),
            (3, "BigStack", 500),
        ]
    );
    assert_eq!(
        pot_eligibilities(&normalized)
            .iter()
            .map(|eligibility| (eligibility.pot_no, eligibility.player_name.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (1, "Shorty"),
            (1, "Medium"),
            (1, "BigStack"),
            (2, "Shorty"),
            (2, "Medium"),
            (2, "BigStack"),
            (3, "Medium"),
            (3, "BigStack"),
        ]
    );
}

#[test]
fn keeps_full_pack_invariants_green_for_all_committed_hands() {
    let mut issues = Vec::new();

    for fixture in HH_FIXTURE_FILES {
        let content = read_hh_fixture(fixture);
        let hands = split_hand_history(&content)
            .unwrap_or_else(|error| panic!("fixture `{fixture}` failed to split: {error}"));

        for hand in hands {
            let parsed = parse_canonical_hand(&hand.raw_text).unwrap_or_else(|error| {
                panic!(
                    "fixture `{fixture}` hand `{}` failed to parse: {error}",
                    hand.header.hand_id
                )
            });
            let normalized = normalize_hand(&parsed).unwrap_or_else(|error| {
                panic!(
                    "fixture `{fixture}` hand `{}` failed to normalize: {error}",
                    parsed.header.hand_id
                )
            });

            if !normalized.invariants.chip_conservation_ok
                || !normalized.invariants.pot_conservation_ok
                || !normalized.invariants.issues.is_empty()
                || normalized.eliminations.iter().any(|elimination| {
                    elimination_ko_certainty(elimination) == Some("inconsistent".to_string())
                })
            {
                issues.push(format!(
                    "{fixture} :: {} :: chip_ok={} pot_ok={} errors={:?} eliminations={:?}",
                    parsed.header.hand_id,
                    normalized.invariants.chip_conservation_ok,
                    normalized.invariants.pot_conservation_ok,
                    normalized.invariants.issues,
                    normalized
                        .eliminations
                        .iter()
                        .map(|elimination| (
                            elimination.eliminated_player_name.clone(),
                            elimination_elimination_certainty(elimination),
                            elimination_last_busting_pot_no(elimination)
                        ))
                        .collect::<Vec<_>>()
                ));
            }
        }
    }

    assert!(
        issues.is_empty(),
        "full-pack normalization issues:\n{}",
        issues.join("\n")
    );
}

#[test]
fn resolves_pot_winners_even_when_collect_lines_are_not_grouped_by_pot() {
    let hand = parse_canonical_hand(REORDERED_COLLECT_SIDE_POT_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let final_pots = final_pots(&normalized);
    let pot_winners = pot_winners(&normalized);
    assert_eq!(final_pots.len(), 3);
    assert_eq!(pot_winners.len(), 3);
    assert_eq!(
        pot_winners
            .iter()
            .map(|winner| (winner.pot_no, winner.share_amount))
            .collect::<Vec<_>>(),
        vec![(1, 400), (2, 1_200), (3, 1_000)]
    );

    let medium = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "Medium")
        .unwrap();
    let medium = serde_json::to_value(medium).unwrap();
    assert_eq!(medium["pots_participated_by_busted"], json!([1, 2, 3]));
    assert_eq!(medium["pots_causing_bust"], json!([3]));
    assert_eq!(medium["last_busting_pot_no"], json!(3));
    assert_eq!(medium["ko_certainty_state"], json!("exact"));
}

#[test]
fn resolves_split_main_and_single_winner_side_from_showdown_ranks() {
    let hand = parse_canonical_hand(SPLIT_MAIN_SINGLE_SIDE_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let final_pots = final_pots(&normalized);
    let pot_eligibilities = pot_eligibilities(&normalized);
    let pot_winners = pot_winners(&normalized);
    assert_eq!(final_pots.len(), 2);
    assert_eq!(final_pots[0].amount, 900);
    assert_eq!(final_pots[1].amount, 400);
    assert_eq!(pot_eligibilities.len(), 5);
    assert_eq!(
        pot_eligibilities
            .iter()
            .map(|eligibility| (eligibility.pot_no, eligibility.player_name.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (1, "Hero"),
            (1, "Shorty"),
            (1, "Villain"),
            (2, "Hero"),
            (2, "Villain"),
        ]
    );
    assert_eq!(
        pot_winners
            .iter()
            .map(|winner| (
                winner.pot_no,
                winner.player_name.as_str(),
                winner.share_amount
            ))
            .collect::<Vec<_>>(),
        vec![(1, "Hero", 450), (1, "Shorty", 450), (2, "Hero", 400),]
    );
    assert!(normalized.invariants.issues.is_empty());
}

#[test]
fn uses_only_last_busting_pot_for_multi_pot_ko_credit() {
    let hand = parse_canonical_hand(JOINT_KO_MULTI_POT_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let medium = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "Medium")
        .unwrap();

    let medium = serde_json::to_value(medium).unwrap();
    assert_eq!(medium["pots_participated_by_busted"], json!([1, 2]));
    assert_eq!(medium["pots_causing_bust"], json!([2]));
    assert_eq!(medium["last_busting_pot_no"], json!(2));
    assert_eq!(medium["ko_winner_set"], json!(["Hero"]));
    assert_eq!(
        medium["ko_share_fraction_by_winner"],
        json!([{
            "seat_no": 1,
            "player_name": "Hero",
            "share_fraction": 1.0
        }])
    );
    assert_eq!(medium["elimination_certainty_state"], json!("exact"));
    assert_eq!(medium["ko_certainty_state"], json!("exact"));
}

#[test]
fn keeps_hidden_showdown_side_pot_ambiguity_uncertain_without_guessing_winners() {
    let hand = parse_canonical_hand(HIDDEN_SHOWDOWN_AMBIGUITY_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(
        normalized.settlement.certainty_state,
        CertaintyState::Uncertain
    );
    assert!(normalized.settlement.issues.is_empty());
    assert_eq!(normalized.settlement.pots.len(), 2);
    assert!(
        normalized
            .settlement
            .pots
            .iter()
            .all(|pot| pot.selected_allocation.is_none())
    );
    assert_eq!(
        normalized.settlement.pots[0].issues,
        vec![PotSettlementIssue::AmbiguousHiddenShowdown {
            eligible_players: vec!["Hero".to_string(), "Villain".to_string()],
        }]
    );
    assert_eq!(
        normalized.settlement.pots[1].issues,
        vec![PotSettlementIssue::AmbiguousHiddenShowdown {
            eligible_players: vec!["Hero".to_string(), "Villain".to_string()],
        }]
    );

    let shorty_a = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "ShortyA")
        .unwrap();
    let shorty_a = serde_json::to_value(shorty_a).unwrap();
    assert_eq!(shorty_a["pots_participated_by_busted"], json!([1]));
    assert_eq!(shorty_a["pots_causing_bust"], json!([1]));
    assert_eq!(shorty_a["last_busting_pot_no"], json!(1));
    assert_eq!(shorty_a["ko_winner_set"], json!([]));
    assert_eq!(shorty_a["ko_share_fraction_by_winner"], json!([]));
    assert_eq!(shorty_a["elimination_certainty_state"], json!("exact"));
    assert_eq!(shorty_a["ko_certainty_state"], json!("uncertain"));
}

#[test]
fn surfaces_collect_distribution_conflict_with_showdown_as_inconsistent() {
    let hand = parse_canonical_hand(AMBIGUOUS_COLLECT_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let final_pots = final_pots(&normalized);
    let pot_winners = pot_winners(&normalized);
    assert_eq!(final_pots.len(), 2);
    assert_eq!(final_pots[0].amount, 400);
    assert_eq!(final_pots[1].amount, 400);
    assert!(pot_winners.is_empty());
    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert_eq!(
        normalized.settlement.issues,
        vec![tracker_parser_core::models::SettlementIssue::CollectConflictNoExactSettlementMatchesCollectedAmounts]
    );

    let shorty_a = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "ShortyA")
        .unwrap();
    let shorty_a = serde_json::to_value(shorty_a).unwrap();
    assert_eq!(shorty_a["pots_participated_by_busted"], json!([1]));
    assert_eq!(shorty_a["pots_causing_bust"], json!([1]));
    assert_eq!(shorty_a["last_busting_pot_no"], json!(1));
    assert_eq!(shorty_a["ko_winner_set"], json!([]));
    assert_eq!(shorty_a["ko_share_fraction_by_winner"], json!([]));
    assert_eq!(shorty_a["elimination_certainty_state"], json!("exact"));
    assert_eq!(shorty_a["ko_certainty_state"], json!("inconsistent"));

    let shorty_b = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "ShortyB")
        .unwrap();
    let shorty_b = serde_json::to_value(shorty_b).unwrap();
    assert_eq!(shorty_b["pots_participated_by_busted"], json!([1]));
    assert_eq!(shorty_b["pots_causing_bust"], json!([1]));
    assert_eq!(shorty_b["last_busting_pot_no"], json!(1));
    assert_eq!(shorty_b["ko_winner_set"], json!([]));
    assert_eq!(shorty_b["ko_share_fraction_by_winner"], json!([]));
    assert_eq!(shorty_b["elimination_certainty_state"], json!("exact"));
    assert_eq!(shorty_b["ko_certainty_state"], json!("inconsistent"));
}

#[test]
fn inconsistent_hand_keeps_observed_actuals_but_omits_exact_layers() {
    let hand = parse_canonical_hand(AMBIGUOUS_COLLECT_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();
    let actual = serde_json::to_value(&normalized.actual).unwrap();

    assert_eq!(actual.get("winner_collections"), None);
    assert_eq!(actual.get("stacks_after_actual"), None);
    assert_eq!(actual["observed_winner_collections"]["Hero"], json!(400));
    assert_eq!(actual["observed_winner_collections"]["Villain"], json!(400));
    assert_eq!(actual["stacks_after_observed"]["Hero"], json!(400));
    assert_eq!(actual["stacks_after_observed"]["Villain"], json!(400));
    assert!(actual.get("exact_selected_payout_totals").is_none());
    assert!(actual.get("stacks_after_exact").is_none());
}

#[test]
fn resolves_odd_chip_split_from_collect_totals_without_guessing_bonus_chip() {
    let hand = parse_canonical_hand(ODD_CHIP_SPLIT_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let final_pots = final_pots(&normalized);
    let pot_winners = pot_winners(&normalized);
    assert_eq!(final_pots.len(), 2);
    assert_eq!(final_pots[0].amount, 3);
    assert_eq!(final_pots[1].amount, 398);
    assert_eq!(
        pot_winners
            .iter()
            .map(|winner| (
                winner.pot_no,
                winner.player_name.as_str(),
                winner.share_amount
            ))
            .collect::<Vec<_>>(),
        vec![
            (1, "Hero", 2),
            (1, "Villain", 1),
            (2, "Hero", 199),
            (2, "Villain", 199),
        ]
    );
    assert!(normalized.invariants.issues.is_empty());
}

#[test]
fn resolves_odd_chip_split_from_summary_amounts_when_collect_lines_are_absent() {
    let hand = parse_canonical_hand(ODD_CHIP_SUMMARY_ONLY_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let final_pots = final_pots(&normalized);
    let pot_winners = pot_winners(&normalized);
    assert_eq!(normalized.settlement.certainty_state, CertaintyState::Exact);
    assert!(normalized.settlement.issues.is_empty());
    assert_eq!(normalized.settlement.evidence.collect_events_seen.len(), 0);
    assert_eq!(final_pots.len(), 2);
    assert_eq!(final_pots[0].amount, 3);
    assert_eq!(final_pots[1].amount, 398);
    assert_eq!(
        pot_winners
            .iter()
            .map(|winner| (
                winner.pot_no,
                winner.player_name.as_str(),
                winner.share_amount
            ))
            .collect::<Vec<_>>(),
        vec![
            (1, "Hero", 2),
            (1, "Villain", 1),
            (2, "Hero", 199),
            (2, "Villain", 199),
        ]
    );
    assert!(normalized.invariants.issues.is_empty());
}

#[test]
fn treats_conflicting_odd_chip_collect_and_summary_evidence_as_inconsistent() {
    let hand = parse_canonical_hand(ODD_CHIP_CONFLICTING_EVIDENCE_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(
        normalized.settlement.certainty_state,
        CertaintyState::Inconsistent
    );
    assert_eq!(
        normalized.settlement.issues,
        vec![tracker_parser_core::models::SettlementIssue::CollectConflictNoExactSettlementMatchesCollectedAmounts]
    );
    assert!(pot_winners(&normalized).is_empty());
}

#[test]
fn leaves_odd_chip_aggregate_ambiguity_unresolved_when_multiple_exact_allocations_fit() {
    let hand = parse_canonical_hand(ODD_CHIP_AMBIGUOUS_AGGREGATE_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(
        normalized.settlement.certainty_state,
        CertaintyState::Uncertain
    );
    assert_eq!(
        normalized.settlement.issues,
        vec![tracker_parser_core::models::SettlementIssue::MultipleExactAllocations]
    );
    assert!(pot_winners(&normalized).is_empty());
    assert!(
        normalized
            .settlement
            .pots
            .iter()
            .all(|pot| pot.selected_allocation.is_none())
    );
    assert_eq!(
        normalized
            .settlement
            .pots
            .iter()
            .map(|pot| pot.candidate_allocations.len())
            .collect::<Vec<_>>(),
        vec![2, 2]
    );
}

#[test]
fn surfaces_unsatisfied_collect_mapping_as_invariant_error_without_guessing_winners() {
    let hand = parse_canonical_hand(UNSATISFIED_COLLECT_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let final_pots = final_pots(&normalized);
    let pot_winners = pot_winners(&normalized);
    assert_eq!(final_pots.len(), 1);
    assert_eq!(final_pots[0].amount, 200);
    assert!(pot_winners.is_empty());
    assert!(!normalized.invariants.chip_conservation_ok);
    assert!(!normalized.invariants.pot_conservation_ok);
    assert_eq!(
        normalized.settlement.issues,
        vec![tracker_parser_core::models::SettlementIssue::CollectConflictNoExactSettlementMatchesCollectedAmounts]
    );

    let villain = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "Villain")
        .unwrap();
    let villain = serde_json::to_value(villain).unwrap();
    assert_eq!(villain["pots_participated_by_busted"], json!([1]));
    assert_eq!(villain["pots_causing_bust"], json!([1]));
    assert_eq!(villain["last_busting_pot_no"], json!(1));
    assert_eq!(villain["ko_winner_set"], json!([]));
    assert_eq!(villain["ko_share_fraction_by_winner"], json!([]));
    assert_eq!(villain["elimination_certainty_state"], json!("exact"));
    assert_eq!(villain["ko_certainty_state"], json!("inconsistent"));
}

#[test]
fn surfaces_illegal_heads_up_preflop_actor_order() {
    let hand = parse_canonical_hand(HEADS_UP_PREFLOP_ILLEGAL_ORDER_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(
        normalized
            .invariants
            .issues
            .iter()
            .any(|issue| matches!(issue, InvariantIssue::IllegalActorOrder { .. })),
        "expected illegal_actor_order, got {:?}",
        normalized.invariants.issues
    );
}

#[test]
fn surfaces_illegal_heads_up_postflop_actor_order() {
    let hand = parse_canonical_hand(HEADS_UP_POSTFLOP_ILLEGAL_ORDER_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(
        invariant_issue_codes(&normalized)
            .iter()
            .any(|issue| *issue == "illegal_actor_order"),
        "expected illegal_actor_order, got {:?}",
        normalized.invariants.issues
    );
}

#[test]
fn surfaces_non_reopening_short_all_in_reraise() {
    let hand = parse_canonical_hand(SHORT_ALL_IN_NON_REOPEN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(
        invariant_issue_codes(&normalized)
            .iter()
            .any(|issue| *issue == "action_not_reopened_after_short_all_in"),
        "expected short-all-in non-reopen error, got {:?}",
        normalized.invariants.issues
    );
}

#[test]
fn allows_reraise_after_full_raise_reopens_action() {
    let hand = parse_canonical_hand(FULL_RAISE_REOPEN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.issues.is_empty());
}

#[test]
fn surfaces_premature_street_close_when_pending_actor_is_skipped() {
    let hand = parse_canonical_hand(PREMATURE_STREET_CLOSE_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(
        invariant_issue_codes(&normalized)
            .iter()
            .any(|issue| *issue == "premature_street_close"),
        "expected premature_street_close, got {:?}",
        normalized.invariants.issues
    );
}

#[test]
fn accepts_limp_raise_call_chain_without_legality_errors() {
    let hand = parse_canonical_hand(LIMP_RAISE_CALL_CHAIN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.issues.is_empty());
}

#[test]
fn accepts_uncalled_return_after_failed_call_chain_without_legality_errors() {
    let hand = parse_canonical_hand(FAILED_CALL_CHAIN_RETURN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.issues.is_empty());
}

#[test]
fn handles_blind_exhausted_all_in_without_legality_errors() {
    let hand = parse_canonical_hand(BLIND_EXHAUSTED_ALL_IN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.issues.is_empty());
}

#[test]
fn handles_ante_exhausted_all_in_without_legality_errors() {
    let hand = parse_canonical_hand(ANTE_EXHAUSTED_ALL_IN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.issues.is_empty());
}

#[test]
fn excludes_sitting_out_seats_from_active_order() {
    let hand = parse_canonical_hand(SITTING_OUT_ACTIVE_ORDER_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.issues.is_empty());
}

fn read_hh_fixture(filename: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/mbr/hh/{filename}")),
    )
    .unwrap()
}

fn final_pots(hand: &NormalizedHand) -> Vec<FinalPot> {
    hand.settlement.final_pots()
}

fn pot_contributions(hand: &NormalizedHand) -> Vec<PotContribution> {
    hand.settlement.pot_contributions()
}

fn pot_eligibilities(hand: &NormalizedHand) -> Vec<PotEligibility> {
    hand.settlement.pot_eligibilities()
}

fn pot_winners(hand: &NormalizedHand) -> Vec<PotWinner> {
    hand.settlement.pot_winners()
}

fn elimination_json_by_player(hand: &NormalizedHand, player_name: &str) -> Value {
    hand.eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == player_name)
        .map(|elimination| serde_json::to_value(elimination).unwrap())
        .unwrap_or_else(|| panic!("missing elimination for `{player_name}`"))
}

fn elimination_elimination_certainty(
    elimination: &tracker_parser_core::models::HandElimination,
) -> Option<String> {
    let value = serde_json::to_value(elimination).unwrap();
    value
        .get("elimination_certainty_state")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .get("certainty_state")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn elimination_ko_certainty(
    elimination: &tracker_parser_core::models::HandElimination,
) -> Option<String> {
    let value = serde_json::to_value(elimination).unwrap();
    value
        .get("ko_certainty_state")
        .and_then(Value::as_str)
        .or_else(|| value.get("certainty_state").and_then(Value::as_str))
        .map(str::to_string)
}

fn elimination_last_busting_pot_no(
    elimination: &tracker_parser_core::models::HandElimination,
) -> Option<u64> {
    let value = serde_json::to_value(elimination).unwrap();
    value
        .get("last_busting_pot_no")
        .and_then(Value::as_u64)
        .or_else(|| value.get("resolved_by_pot_no").and_then(Value::as_u64))
}

fn invariant_issue_codes(hand: &NormalizedHand) -> Vec<&'static str> {
    hand.invariants
        .issues
        .iter()
        .map(invariant_issue_code)
        .collect()
}

fn invariant_issue_code(issue: &InvariantIssue) -> &'static str {
    match issue {
        InvariantIssue::ChipConservationMismatch { .. } => "chip_conservation_mismatch",
        InvariantIssue::PotConservationMismatch { .. } => "pot_conservation_mismatch",
        InvariantIssue::SummaryTotalPotMismatch { .. } => "summary_total_pot_mismatch",
        InvariantIssue::PrematureStreetClose { .. } => "premature_street_close",
        InvariantIssue::IllegalActorOrder { .. } => "illegal_actor_order",
        InvariantIssue::IllegalSmallBlindActor { .. } => "illegal_small_blind_actor",
        InvariantIssue::IllegalBigBlindActor { .. } => "illegal_big_blind_actor",
        InvariantIssue::UncalledReturnActorMismatch { .. } => "uncalled_return_actor_mismatch",
        InvariantIssue::UncalledReturnAmountMismatch { .. } => "uncalled_return_amount_mismatch",
        InvariantIssue::ActionAmountExceedsStack { .. } => "action_amount_exceeds_stack",
        InvariantIssue::RefundExceedsCommitted { .. } => "refund_exceeds_committed",
        InvariantIssue::RefundExceedsBettingRoundContrib { .. } => {
            "refund_exceeds_betting_round_contrib"
        }
        InvariantIssue::IllegalCheck { .. } => "illegal_check",
        InvariantIssue::IllegalCallAmount { .. } => "illegal_call_amount",
        InvariantIssue::UndercallInconsistency { .. } => "undercall_inconsistency",
        InvariantIssue::OvercallInconsistency { .. } => "overcall_inconsistency",
        InvariantIssue::IllegalBetFacingOpenBet { .. } => "illegal_bet_facing_open_bet",
        InvariantIssue::ActionNotReopenedAfterShortAllIn { .. } => {
            "action_not_reopened_after_short_all_in"
        }
        InvariantIssue::IncompleteRaiseToCall { .. } => "incomplete_raise",
        InvariantIssue::IncompleteRaiseSize { .. } => "incomplete_raise",
    }
}
