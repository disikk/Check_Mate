use std::{fs, path::PathBuf};

use tracker_parser_core::{
    models::{CertaintyState, PlayerStatus, Street},
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
        normalized.actual.stacks_after_actual.get("Hero"),
        Some(&18_000)
    );
    assert_eq!(
        normalized.actual.stacks_after_actual.get("f02e54a6"),
        Some(&0)
    );
    assert_eq!(
        normalized.actual.winner_collections.get("Hero"),
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
    assert_eq!(normalized.final_pots.len(), 1);
    assert_eq!(normalized.final_pots[0].pot_no, 1);
    assert!(normalized.final_pots[0].is_main);
    assert_eq!(normalized.final_pots[0].amount, 3_984);
    assert!(normalized.returns.is_empty());
    assert_eq!(normalized.pot_contributions.len(), 2);
    assert_eq!(normalized.pot_winners.len(), 1);
    assert_eq!(normalized.pot_winners[0].pot_no, 1);
    assert_eq!(normalized.pot_winners[0].seat_no, 7);
    assert_eq!(normalized.pot_winners[0].player_name, "Hero");
    assert_eq!(normalized.pot_winners[0].share_amount, 3_984);
    assert_eq!(normalized.eliminations.len(), 1);
    assert_eq!(normalized.eliminations[0].eliminated_seat_no, 3);
    assert_eq!(
        normalized.eliminations[0].eliminated_player_name,
        "f02e54a6"
    );
    assert_eq!(normalized.eliminations[0].resolved_by_pot_nos, vec![1]);
    assert_eq!(
        normalized.eliminations[0].ko_involved_winners,
        vec!["Hero".to_string()]
    );
    assert_eq!(normalized.eliminations[0].hero_ko_share_total, Some(1.0));
    assert!(!normalized.eliminations[0].joint_ko);
    assert_eq!(normalized.eliminations[0].resolved_by_pot_no, Some(1));
    assert_eq!(normalized.eliminations[0].ko_involved_winner_count, 1);
    assert!(normalized.eliminations[0].hero_involved);
    assert_eq!(normalized.eliminations[0].hero_share_fraction, Some(1.0));
    assert!(!normalized.eliminations[0].is_split_ko);
    assert_eq!(normalized.eliminations[0].split_n, Some(1));
    assert!(!normalized.eliminations[0].is_sidepot_based);
    assert_eq!(
        normalized.eliminations[0].certainty_state,
        CertaintyState::Exact
    );
    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn handles_uncalled_return_without_creating_fake_snapshot() {
    let second_hand = HH_FT.split("\n\n").nth(1).unwrap();
    let hand = parse_canonical_hand(second_hand).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.snapshot.is_none());
    assert_eq!(
        normalized.actual.stacks_after_actual.get("Hero"),
        Some(&16_008)
    );
    assert_eq!(
        normalized.actual.stacks_after_actual.get("f02e54a6"),
        Some(&1_992)
    );
    assert_eq!(normalized.actual.winner_collections.get("Hero"), Some(&960));
    assert_eq!(
        normalized.actual.committed_total_by_player.get("Hero"),
        Some(&480)
    );
    assert_eq!(
        normalized.actual.committed_total_by_player.get("f02e54a6"),
        Some(&480)
    );
    assert_eq!(normalized.actual.rake_amount, 0);
    assert_eq!(normalized.final_pots.len(), 1);
    assert_eq!(normalized.final_pots[0].amount, 960);
    assert_eq!(normalized.returns.len(), 1);
    assert_eq!(normalized.returns[0].seat_no, 7);
    assert_eq!(normalized.returns[0].player_name, "Hero");
    assert_eq!(normalized.returns[0].amount, 15_048);
    assert_eq!(normalized.returns[0].reason, "uncalled");
    assert_eq!(normalized.pot_winners.len(), 1);
    assert_eq!(normalized.pot_winners[0].player_name, "Hero");
    assert_eq!(normalized.pot_winners[0].share_amount, 960);
    assert!(normalized.eliminations.is_empty());
    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn resolves_split_ko_with_exact_hero_share_fraction() {
    let hand = parse_canonical_hand(SPLIT_KO_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(normalized.final_pots.len(), 1);
    assert_eq!(normalized.final_pots[0].amount, 3_000);
    assert_eq!(normalized.pot_winners.len(), 2);
    assert_eq!(normalized.eliminations.len(), 1);
    assert_eq!(
        normalized.eliminations[0].eliminated_player_name,
        "VillainA"
    );
    assert_eq!(normalized.eliminations[0].resolved_by_pot_nos, vec![1]);
    assert_eq!(
        normalized.eliminations[0].ko_involved_winners,
        vec!["Hero".to_string(), "VillainB".to_string()]
    );
    assert_eq!(normalized.eliminations[0].hero_ko_share_total, Some(0.5));
    assert!(normalized.eliminations[0].joint_ko);
    assert_eq!(normalized.eliminations[0].resolved_by_pot_no, Some(1));
    assert!(normalized.eliminations[0].hero_involved);
    assert_eq!(normalized.eliminations[0].hero_share_fraction, Some(0.5));
    assert!(normalized.eliminations[0].is_split_ko);
    assert_eq!(normalized.eliminations[0].split_n, Some(2));
    assert!(!normalized.eliminations[0].is_sidepot_based);
    assert_eq!(
        normalized.eliminations[0].certainty_state,
        CertaintyState::Exact
    );
}

#[test]
fn resolves_sidepot_ko_without_marking_hero_involved() {
    let hand = parse_canonical_hand(SIDEPOT_KO_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(normalized.final_pots.len(), 3);
    assert_eq!(normalized.final_pots[0].amount, 400);
    assert_eq!(normalized.final_pots[1].amount, 1_200);
    assert_eq!(normalized.final_pots[2].amount, 1_000);
    assert_eq!(normalized.returns.len(), 0);

    let medium = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "Medium")
        .unwrap();
    assert_eq!(medium.resolved_by_pot_nos, vec![1, 2, 3]);
    assert_eq!(medium.ko_involved_winners, vec!["BigStack".to_string()]);
    assert_eq!(medium.hero_ko_share_total, Some(0.0));
    assert!(!medium.joint_ko);
    assert_eq!(medium.resolved_by_pot_no, None);
    assert!(!medium.hero_involved);
    assert_eq!(medium.hero_share_fraction, Some(0.0));
    assert!(!medium.is_split_ko);
    assert_eq!(medium.split_n, Some(1));
    assert!(medium.is_sidepot_based);
    assert_eq!(medium.certainty_state, CertaintyState::Exact);
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
                || !normalized.invariants.invariant_errors.is_empty()
                || normalized
                    .eliminations
                    .iter()
                    .any(|elimination| elimination.certainty_state == CertaintyState::Inconsistent)
            {
                issues.push(format!(
                    "{fixture} :: {} :: chip_ok={} pot_ok={} errors={:?} eliminations={:?}",
                    parsed.header.hand_id,
                    normalized.invariants.chip_conservation_ok,
                    normalized.invariants.pot_conservation_ok,
                    normalized.invariants.invariant_errors,
                    normalized
                        .eliminations
                        .iter()
                        .map(|elimination| (
                            elimination.eliminated_player_name.clone(),
                            elimination.certainty_state,
                            elimination.resolved_by_pot_no
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

    assert_eq!(normalized.final_pots.len(), 3);
    assert_eq!(normalized.pot_winners.len(), 3);
    assert_eq!(
        normalized
            .pot_winners
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
    assert_eq!(medium.resolved_by_pot_nos, vec![1, 2, 3]);
    assert_eq!(medium.resolved_by_pot_no, None);
    assert_eq!(medium.certainty_state, CertaintyState::Exact);
}

#[test]
fn resolves_split_main_and_single_winner_side_from_showdown_ranks() {
    let hand = parse_canonical_hand(SPLIT_MAIN_SINGLE_SIDE_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(normalized.final_pots.len(), 2);
    assert_eq!(normalized.final_pots[0].amount, 900);
    assert_eq!(normalized.final_pots[1].amount, 400);
    assert_eq!(normalized.pot_eligibilities.len(), 5);
    assert_eq!(
        normalized
            .pot_eligibilities
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
        normalized
            .pot_winners
            .iter()
            .map(|winner| (
                winner.pot_no,
                winner.player_name.as_str(),
                winner.share_amount
            ))
            .collect::<Vec<_>>(),
        vec![(1, "Hero", 450), (1, "Shorty", 450), (2, "Hero", 400),]
    );
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn resolves_joint_ko_across_main_and_side_pots_with_different_winners() {
    let hand = parse_canonical_hand(JOINT_KO_MULTI_POT_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    let medium = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "Medium")
        .unwrap();

    assert_eq!(medium.resolved_by_pot_nos, vec![1, 2]);
    assert_eq!(
        medium.ko_involved_winners,
        vec!["Shorty".to_string(), "Hero".to_string()]
    );
    assert_eq!(medium.hero_ko_share_total, Some(0.4));
    assert!(medium.hero_involved);
    assert!(medium.joint_ko);
    assert_eq!(medium.resolved_by_pot_no, None);
    assert_eq!(medium.ko_involved_winner_count, 2);
    assert_eq!(medium.hero_share_fraction, Some(0.4));
    assert_eq!(medium.split_n, Some(2));
    assert!(medium.is_sidepot_based);
    assert_eq!(medium.certainty_state, CertaintyState::Exact);
}

#[test]
fn keeps_hidden_showdown_side_pot_ambiguity_uncertain_without_guessing_winners() {
    let hand = parse_canonical_hand(HIDDEN_SHOWDOWN_AMBIGUITY_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(normalized.final_pots.len(), 2);
    assert_eq!(normalized.final_pots[0].amount, 400);
    assert_eq!(normalized.final_pots[1].amount, 400);
    assert_eq!(normalized.pot_eligibilities.len(), 6);
    assert!(normalized.pot_winners.is_empty());
    assert!(
        normalized
            .invariants
            .uncertain_reason_codes
            .iter()
            .any(|issue| issue.starts_with("pot_settlement_ambiguous_hidden_showdown:")),
        "expected hidden-showdown ambiguity, got {:?}",
        normalized.invariants.uncertain_reason_codes
    );

    let shorty_a = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "ShortyA")
        .unwrap();
    assert_eq!(shorty_a.certainty_state, CertaintyState::Uncertain);
}

#[test]
fn surfaces_collect_distribution_conflict_with_showdown_as_inconsistent() {
    let hand = parse_canonical_hand(AMBIGUOUS_COLLECT_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(normalized.final_pots.len(), 2);
    assert_eq!(normalized.final_pots[0].amount, 400);
    assert_eq!(normalized.final_pots[1].amount, 400);
    assert!(normalized.pot_winners.is_empty());
    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(
        normalized
            .invariants
            .invariant_errors
            .iter()
            .any(|issue| issue.starts_with("pot_settlement_collect_conflict:")),
        "expected collect conflict, got {:?}",
        normalized.invariants.invariant_errors
    );

    let shorty_a = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "ShortyA")
        .unwrap();
    assert_eq!(shorty_a.resolved_by_pot_no, Some(1));
    assert_eq!(shorty_a.ko_involved_winner_count, 0);
    assert!(!shorty_a.hero_involved);
    assert_eq!(shorty_a.hero_share_fraction, None);
    assert!(!shorty_a.is_split_ko);
    assert_eq!(shorty_a.split_n, None);
    assert!(!shorty_a.is_sidepot_based);
    assert_eq!(shorty_a.certainty_state, CertaintyState::Inconsistent);

    let shorty_b = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "ShortyB")
        .unwrap();
    assert_eq!(shorty_b.resolved_by_pot_no, Some(1));
    assert_eq!(shorty_b.ko_involved_winner_count, 0);
    assert!(!shorty_b.hero_involved);
    assert_eq!(shorty_b.hero_share_fraction, None);
    assert_eq!(shorty_b.certainty_state, CertaintyState::Inconsistent);
}

#[test]
fn resolves_odd_chip_split_from_collect_totals_without_guessing_bonus_chip() {
    let hand = parse_canonical_hand(ODD_CHIP_SPLIT_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(normalized.final_pots.len(), 2);
    assert_eq!(normalized.final_pots[0].amount, 3);
    assert_eq!(normalized.final_pots[1].amount, 398);
    assert_eq!(
        normalized
            .pot_winners
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
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn surfaces_unsatisfied_collect_mapping_as_invariant_error_without_guessing_winners() {
    let hand = parse_canonical_hand(UNSATISFIED_COLLECT_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert_eq!(normalized.final_pots.len(), 1);
    assert_eq!(normalized.final_pots[0].amount, 200);
    assert!(normalized.pot_winners.is_empty());
    assert!(!normalized.invariants.chip_conservation_ok);
    assert!(!normalized.invariants.pot_conservation_ok);
    assert!(
        normalized
            .invariants
            .invariant_errors
            .iter()
            .any(|issue| issue.starts_with("pot_settlement_collect_conflict:"))
    );

    let villain = normalized
        .eliminations
        .iter()
        .find(|elimination| elimination.eliminated_player_name == "Villain")
        .unwrap();
    assert_eq!(villain.resolved_by_pot_no, Some(1));
    assert_eq!(villain.ko_involved_winner_count, 0);
    assert!(!villain.hero_involved);
    assert_eq!(villain.hero_share_fraction, None);
    assert_eq!(villain.certainty_state, CertaintyState::Inconsistent);
}

#[test]
fn surfaces_illegal_heads_up_preflop_actor_order() {
    let hand = parse_canonical_hand(HEADS_UP_PREFLOP_ILLEGAL_ORDER_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(
        normalized
            .invariants
            .invariant_errors
            .iter()
            .any(|issue| issue.starts_with("illegal_actor_order:")),
        "expected illegal_actor_order, got {:?}",
        normalized.invariants.invariant_errors
    );
}

#[test]
fn surfaces_illegal_heads_up_postflop_actor_order() {
    let hand = parse_canonical_hand(HEADS_UP_POSTFLOP_ILLEGAL_ORDER_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(
        normalized
            .invariants
            .invariant_errors
            .iter()
            .any(|issue| issue.starts_with("illegal_actor_order:")),
        "expected illegal_actor_order, got {:?}",
        normalized.invariants.invariant_errors
    );
}

#[test]
fn surfaces_non_reopening_short_all_in_reraise() {
    let hand = parse_canonical_hand(SHORT_ALL_IN_NON_REOPEN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(
        normalized
            .invariants
            .invariant_errors
            .iter()
            .any(|issue| issue.starts_with("action_not_reopened_after_short_all_in:")),
        "expected short-all-in non-reopen error, got {:?}",
        normalized.invariants.invariant_errors
    );
}

#[test]
fn allows_reraise_after_full_raise_reopens_action() {
    let hand = parse_canonical_hand(FULL_RAISE_REOPEN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn surfaces_premature_street_close_when_pending_actor_is_skipped() {
    let hand = parse_canonical_hand(PREMATURE_STREET_CLOSE_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(
        normalized
            .invariants
            .invariant_errors
            .iter()
            .any(|issue| issue.starts_with("premature_street_close:")),
        "expected premature_street_close, got {:?}",
        normalized.invariants.invariant_errors
    );
}

#[test]
fn accepts_limp_raise_call_chain_without_legality_errors() {
    let hand = parse_canonical_hand(LIMP_RAISE_CALL_CHAIN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn accepts_uncalled_return_after_failed_call_chain_without_legality_errors() {
    let hand = parse_canonical_hand(FAILED_CALL_CHAIN_RETURN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn handles_blind_exhausted_all_in_without_legality_errors() {
    let hand = parse_canonical_hand(BLIND_EXHAUSTED_ALL_IN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn handles_ante_exhausted_all_in_without_legality_errors() {
    let hand = parse_canonical_hand(ANTE_EXHAUSTED_ALL_IN_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}

#[test]
fn excludes_sitting_out_seats_from_active_order() {
    let hand = parse_canonical_hand(SITTING_OUT_ACTIVE_ORDER_HAND).unwrap();
    let normalized = normalize_hand(&hand).unwrap();

    assert!(normalized.invariants.chip_conservation_ok);
    assert!(normalized.invariants.pot_conservation_ok);
    assert!(normalized.invariants.invariant_errors.is_empty());
}

fn read_hh_fixture(filename: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/mbr/hh/{filename}")),
    )
    .unwrap()
}
