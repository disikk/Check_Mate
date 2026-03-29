use crate::{
    ParserError,
    models::{
        ActionType, AllInReason, CanonicalParsedHand, HandActionEvent, HandHeader, HandRecord,
        ParseIssue, ParseIssueCode, ParsedHandSeat, Street, SummarySeatMarker, SummarySeatOutcome,
        SummarySeatOutcomeKind,
    },
    money_state::{apply_debit, apply_refund},
};

use regex::Regex;
use std::sync::OnceLock;

enum SummarySeatOutcomeParseResult {
    Parsed(SummarySeatOutcome),
    UnknownTail,
    InvalidHead,
}

struct ParsedSummarySeatHead {
    seat_no: u8,
    player_name: String,
    position_marker: Option<SummarySeatMarker>,
    tail: String,
}

enum SummarySeatTailAst {
    Folded {
        street: Street,
    },
    ShowedWon {
        shown_cards: Vec<String>,
        won_amount: i64,
        hand_class: Option<String>,
    },
    ShowedLost {
        shown_cards: Vec<String>,
        hand_class: Option<String>,
    },
    Lost,
    Mucked,
    Won {
        won_amount: i64,
        hand_class: Option<String>,
    },
    Collected {
        won_amount: i64,
    },
}

pub fn split_hand_history(input: &str) -> Result<Vec<HandRecord>, ParserError> {
    let normalized = normalize_newlines(input);
    if normalized.trim().is_empty() {
        return Err(ParserError::EmptySource);
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in normalized.lines() {
        if line.starts_with("Poker Hand #") && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current.clear();
        }

        if current.is_empty() && !line.starts_with("Poker Hand #") {
            continue;
        }

        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }

    if !current.is_empty() {
        chunks.push(current.trim().to_string());
    }

    if chunks.is_empty() {
        return Err(ParserError::UnsupportedSourceFormat);
    }

    chunks
        .into_iter()
        .map(|raw_text| {
            let header = parse_hand_header(&raw_text)?;
            Ok(HandRecord { header, raw_text })
        })
        .collect()
}

pub fn parse_hand_header(hand_text: &str) -> Result<HandHeader, ParserError> {
    static FIRST_REGEX: OnceLock<Regex> = OnceLock::new();
    static SECOND_REGEX: OnceLock<Regex> = OnceLock::new();
    static ANTE_REGEX: OnceLock<Regex> = OnceLock::new();

    let normalized = normalize_newlines(hand_text);
    let mut lines = normalized.lines().filter(|line| !line.trim().is_empty());

    let first_line = lines
        .next()
        .ok_or(ParserError::MissingLine("hand header line"))?;
    let second_line = lines.next().ok_or(ParserError::MissingLine("table line"))?;

    let first_regex = FIRST_REGEX.get_or_init(|| {
        Regex::new(
            r"^Poker Hand #(?P<hand_id>[^:]+): Tournament #(?P<tournament_id>\d+), (?P<game_name>.+) - (?P<level_name>Level\d+)\((?P<small_blind>[\d,]+)/(?P<big_blind>[\d,]+)(?:\((?P<ante>[\d,]+)\))?\) - (?P<played_at>\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2})$",
        )
        .expect("hand header regex must compile")
    });
    let second_regex = SECOND_REGEX.get_or_init(|| {
        Regex::new(
            r"^Table '(?P<table_name>[^']+)' (?P<max_players>\d+)-max Seat #(?P<button_seat>\d+) is the button$",
        )
        .expect("table regex must compile")
    });
    let ante_regex = ANTE_REGEX.get_or_init(|| {
        Regex::new(r"^[^:]+: posts the ante (?P<ante>[\d,]+)$").expect("ante regex must compile")
    });

    let header_caps = first_regex
        .captures(first_line)
        .ok_or_else(|| invalid_field("hand_header_line", first_line))?;
    let table_caps = second_regex
        .captures(second_line)
        .ok_or_else(|| invalid_field("table_line", second_line))?;

    let ante = header_caps
        .name("ante")
        .map(|ante| parse_u32(ante.as_str(), "ante"))
        .transpose()?
        .or_else(|| {
            normalized
                .lines()
                .skip(2)
                .map(str::trim)
                .find_map(|line| ante_regex.captures(line))
                .map(|captures| parse_u32(&captures["ante"], "ante"))
                .transpose()
                .ok()
                .flatten()
        })
        .unwrap_or(0);

    Ok(HandHeader {
        hand_id: header_caps["hand_id"].to_string(),
        tournament_id: parse_u64(&header_caps["tournament_id"], "tournament_id")?,
        game_name: header_caps["game_name"].to_string(),
        level_name: header_caps["level_name"].to_string(),
        small_blind: parse_u32(&header_caps["small_blind"], "small_blind")?,
        big_blind: parse_u32(&header_caps["big_blind"], "big_blind")?,
        ante,
        played_at: header_caps["played_at"].to_string(),
        table_name: table_caps["table_name"].to_string(),
        max_players: parse_u8(&table_caps["max_players"], "max_players")?,
        button_seat: parse_u8(&table_caps["button_seat"], "button_seat")?,
    })
}

pub fn parse_canonical_hand(hand_text: &str) -> Result<CanonicalParsedHand, ParserError> {
    let normalized = normalize_newlines(hand_text);
    let header = parse_hand_header(&normalized)?;

    let mut seats = Vec::new();
    let mut actions = Vec::new();
    let mut board_final = Vec::new();
    let mut summary_total_pot = None;
    let mut summary_rake_amount = None;
    let mut summary_board = Vec::new();
    let mut hero_hole_cards = None;
    let mut hero_name = None;
    let mut showdown_hands = std::collections::BTreeMap::new();
    let mut summary_seat_outcomes = Vec::new();
    let mut collected_amounts = std::collections::BTreeMap::new();
    let mut parse_issues = Vec::new();
    let mut street = Street::Preflop;
    let mut seq = 0usize;

    for line in normalized.lines().skip(2) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line == "*** HOLE CARDS ***" {
            street = Street::Preflop;
            continue;
        }

        if let Some((player_name, cards)) = parse_dealt_to_line(line) {
            hero_name = Some(player_name);
            hero_hole_cards = Some(cards);
            continue;
        }

        if let Some(player_name) = parse_hidden_dealt_to_line(line) {
            // In the sanitized repo fixtures the hidden hero surface is still labeled as `Hero`.
            if player_name == "Hero" {
                hero_name = Some(player_name);
                hero_hole_cards = None;
            }
            continue;
        }

        if line.starts_with("Dealt to ") {
            parse_issues.push(raw_line_error(ParseIssueCode::MalformedDealtToLine, line));
            continue;
        }

        if let Some((next_street, cards)) = parse_board_transition(line) {
            street = next_street;
            match street {
                Street::Flop => board_final = cards,
                Street::Turn | Street::River => board_final.extend(cards),
                Street::Preflop | Street::Showdown | Street::Summary => {}
            }
            continue;
        }

        if line == "*** SHOWDOWN ***" {
            street = Street::Showdown;
            continue;
        }

        if line == "*** SUMMARY ***" {
            street = Street::Summary;
            continue;
        }

        if street == Street::Summary && line.starts_with("Seat ") {
            match parse_summary_seat_outcome_line(line) {
                SummarySeatOutcomeParseResult::Parsed(outcome) => {
                    summary_seat_outcomes.push(outcome)
                }
                SummarySeatOutcomeParseResult::UnknownTail => {
                    parse_issues.push(raw_line_warning(
                        ParseIssueCode::UnparsedSummarySeatTail,
                        line,
                    ));
                }
                SummarySeatOutcomeParseResult::InvalidHead => {
                    parse_issues.push(raw_line_warning(
                        ParseIssueCode::UnparsedSummarySeatLine,
                        line,
                    ));
                }
            }
            continue;
        }

        if let Some(seat) = parse_seat_line(line)? {
            seats.push(seat);
            continue;
        }

        if let Some((total_pot, rake_amount)) = parse_summary_total_line(line)? {
            summary_total_pot = Some(total_pot);
            summary_rake_amount = Some(rake_amount);
            continue;
        }

        if let Some(cards) = parse_summary_board_line(line) {
            summary_board = cards;
            continue;
        }

        if let Some(event) = parse_uncalled_return(line, street, seq)? {
            actions.push(event);
            seq += 1;
            continue;
        }

        if let Some((player_name, cards, event)) = parse_show_line(line, street, seq)? {
            showdown_hands.insert(player_name, cards);
            actions.push(event);
            seq += 1;
            continue;
        }

        if let Some((player_name, amount, event)) = parse_collect_line(line, street, seq)? {
            *collected_amounts.entry(player_name).or_default() += amount;
            actions.push(event);
            seq += 1;
            continue;
        }

        if let Some(event) = parse_player_action_line(line, street, seq)? {
            actions.push(event);
            seq += 1;
            continue;
        }

        if is_no_show_line(line) {
            parse_issues.push(raw_line_warning(
                ParseIssueCode::UnsupportedNoShowLine,
                line,
            ));
            continue;
        }

        if !line.starts_with("Seat ") {
            parse_issues.push(raw_line_warning(ParseIssueCode::UnparsedLine, line));
        }
    }

    annotate_partial_reveal_parse_issues(&mut parse_issues, &actions, &summary_seat_outcomes);
    annotate_action_all_in_metadata(&seats, &mut actions)?;

    Ok(CanonicalParsedHand {
        header,
        hero_name,
        seats,
        actions,
        board_final,
        summary_total_pot,
        summary_rake_amount,
        summary_board,
        hero_hole_cards,
        showdown_hands,
        summary_seat_outcomes,
        collected_amounts,
        raw_hand_text: normalized,
        parse_issues,
    })
}

fn raw_line_warning(code: ParseIssueCode, line: &str) -> ParseIssue {
    ParseIssue::warning(
        code,
        format!("{}: {line}", code.as_str()),
        Some(line.to_string()),
        None,
    )
}

fn raw_line_error(code: ParseIssueCode, line: &str) -> ParseIssue {
    ParseIssue::error(
        code,
        format!("{}: {line}", code.as_str()),
        Some(line.to_string()),
        None,
    )
}

fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

fn parse_u8(raw: &str, field: &'static str) -> Result<u8, ParserError> {
    raw.trim()
        .replace(',', "")
        .parse::<u8>()
        .map_err(|_| invalid_field(field, raw))
}

fn parse_u32(raw: &str, field: &'static str) -> Result<u32, ParserError> {
    raw.trim()
        .replace(',', "")
        .parse::<u32>()
        .map_err(|_| invalid_field(field, raw))
}

fn parse_u64(raw: &str, field: &'static str) -> Result<u64, ParserError> {
    raw.trim()
        .replace(',', "")
        .parse::<u64>()
        .map_err(|_| invalid_field(field, raw))
}

fn invalid_field(field: &'static str, value: &str) -> ParserError {
    ParserError::InvalidField {
        field,
        value: value.to_string(),
    }
}

fn parse_seat_line(line: &str) -> Result<Option<ParsedHandSeat>, ParserError> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = REGEX.get_or_init(|| {
        Regex::new(
            r"^Seat (?P<seat_no>\d+): (?P<player_name>.+) \((?P<stack>[\d,]+) in chips\)(?P<sitting_out> is sitting out)?$",
        )
        .expect("seat regex must compile")
    });
    let Some(captures) = regex.captures(line) else {
        return Ok(None);
    };

    Ok(Some(ParsedHandSeat {
        seat_no: parse_u8(&captures["seat_no"], "seat_no")?,
        player_name: captures["player_name"].to_string(),
        starting_stack: parse_i64(&captures["stack"], "starting_stack")?,
        is_sitting_out: captures.name("sitting_out").is_some(),
    }))
}

fn parse_dealt_to_line(line: &str) -> Option<(String, Vec<String>)> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = REGEX.get_or_init(|| {
        Regex::new(r"^Dealt to (?P<player_name>.+) \[(?P<cards>[^\]]+)\]$")
            .expect("dealt-to regex must compile")
    });
    let captures = regex.captures(line)?;
    Some((
        captures["player_name"].to_string(),
        split_cards(&captures["cards"]),
    ))
}

fn parse_hidden_dealt_to_line(line: &str) -> Option<String> {
    if line.contains('[') || line.contains(']') {
        return None;
    }

    static REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = REGEX.get_or_init(|| {
        Regex::new(r"^Dealt to (?P<player_name>.+)$").expect("hidden dealt-to regex must compile")
    });
    regex
        .captures(line)
        .map(|captures| captures["player_name"].to_string())
}

fn parse_board_transition(line: &str) -> Option<(Street, Vec<String>)> {
    static FLOP_REGEX: OnceLock<Regex> = OnceLock::new();
    static TURN_REGEX: OnceLock<Regex> = OnceLock::new();
    static RIVER_REGEX: OnceLock<Regex> = OnceLock::new();

    let flop_regex = FLOP_REGEX.get_or_init(|| {
        Regex::new(r"^\*\*\* FLOP \*\*\* \[(?P<cards>[^\]]+)\]$").expect("flop regex must compile")
    });
    if let Some(captures) = flop_regex.captures(line) {
        return Some((Street::Flop, split_cards(&captures["cards"])));
    }

    let turn_regex = TURN_REGEX.get_or_init(|| {
        Regex::new(r"^\*\*\* TURN \*\*\* \[[^\]]+\] \[(?P<card>[^\]]+)\]$")
            .expect("turn regex must compile")
    });
    if let Some(captures) = turn_regex.captures(line) {
        return Some((Street::Turn, vec![captures["card"].to_string()]));
    }

    let river_regex = RIVER_REGEX.get_or_init(|| {
        Regex::new(r"^\*\*\* RIVER \*\*\* \[[^\]]+\] \[(?P<card>[^\]]+)\]$")
            .expect("river regex must compile")
    });
    if let Some(captures) = river_regex.captures(line) {
        return Some((Street::River, vec![captures["card"].to_string()]));
    }

    None
}

fn parse_summary_total_line(line: &str) -> Result<Option<(i64, i64)>, ParserError> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = REGEX.get_or_init(|| {
        Regex::new(r"^Total pot (?P<total_pot>[\d,]+) \| Rake (?P<rake_amount>[\d,]+)(?: \| .+)?$")
            .expect("summary total regex must compile")
    });
    let Some(captures) = regex.captures(line) else {
        return Ok(None);
    };

    Ok(Some((
        parse_i64(&captures["total_pot"], "summary_total_pot")?,
        parse_i64(&captures["rake_amount"], "summary_rake_amount")?,
    )))
}

fn parse_summary_board_line(line: &str) -> Option<Vec<String>> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = REGEX.get_or_init(|| {
        Regex::new(r"^Board \[(?P<cards>[^\]]+)\]$").expect("summary board regex must compile")
    });
    let captures = regex.captures(line)?;
    Some(split_cards(&captures["cards"]))
}

fn parse_summary_seat_outcome_line(line: &str) -> SummarySeatOutcomeParseResult {
    let Some(head) = parse_summary_seat_head(line) else {
        return SummarySeatOutcomeParseResult::InvalidHead;
    };
    let Some(tail_ast) = parse_summary_seat_tail_ast(&head.tail) else {
        return SummarySeatOutcomeParseResult::UnknownTail;
    };

    SummarySeatOutcomeParseResult::Parsed(map_summary_seat_outcome(line, head, tail_ast))
}

fn parse_summary_seat_head(line: &str) -> Option<ParsedSummarySeatHead> {
    let seat_payload = line.strip_prefix("Seat ")?;
    let (seat_no_raw, rest) = seat_payload.split_once(": ")?;
    let seat_no = parse_u8(seat_no_raw, "summary_seat_no").ok()?;
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let captures = REGEX
        .get_or_init(|| {
            Regex::new(
                r"^(?P<player_name>.+?)(?: \((?P<marker>button|small blind|big blind)\))? (?P<tail>.+)$",
            )
            .expect("summary seat head regex must compile")
        })
        .captures(rest)?;

    let position_marker = match captures.name("marker").map(|marker| marker.as_str()) {
        Some("button") => Some(SummarySeatMarker::Button),
        Some("small blind") => Some(SummarySeatMarker::SmallBlind),
        Some("big blind") => Some(SummarySeatMarker::BigBlind),
        Some(_) => return None,
        None => None,
    };

    Some(ParsedSummarySeatHead {
        seat_no,
        player_name: captures["player_name"].to_string(),
        position_marker,
        tail: captures["tail"].to_string(),
    })
}

fn parse_summary_seat_tail_ast(tail: &str) -> Option<SummarySeatTailAst> {
    if let Some(street) = parse_summary_folded_tail(tail) {
        return Some(SummarySeatTailAst::Folded { street });
    }

    if let Some((shown_cards, won_amount, hand_class)) = parse_summary_showed_won_tail(tail) {
        return Some(SummarySeatTailAst::ShowedWon {
            shown_cards,
            won_amount,
            hand_class,
        });
    }

    if let Some((shown_cards, won_amount, hand_class)) = parse_summary_showed_collected_tail(tail) {
        return Some(SummarySeatTailAst::ShowedWon {
            shown_cards,
            won_amount,
            hand_class,
        });
    }

    if let Some((shown_cards, hand_class)) = parse_summary_showed_lost_tail(tail) {
        return Some(SummarySeatTailAst::ShowedLost {
            shown_cards,
            hand_class,
        });
    }

    if tail == "lost" {
        return Some(SummarySeatTailAst::Lost);
    }

    if tail == "mucked" {
        return Some(SummarySeatTailAst::Mucked);
    }

    if let Some((won_amount, hand_class)) = parse_summary_won_tail(tail) {
        return Some(SummarySeatTailAst::Won {
            won_amount,
            hand_class,
        });
    }

    if let Some(won_amount) = parse_summary_collected_tail(tail) {
        return Some(SummarySeatTailAst::Collected { won_amount });
    }

    None
}

fn map_summary_seat_outcome(
    line: &str,
    head: ParsedSummarySeatHead,
    tail_ast: SummarySeatTailAst,
) -> SummarySeatOutcome {
    match tail_ast {
        SummarySeatTailAst::Folded { street } => SummarySeatOutcome {
            seat_no: head.seat_no,
            player_name: head.player_name,
            position_marker: head.position_marker,
            outcome_kind: SummarySeatOutcomeKind::Folded,
            folded_at: Some(street),
            shown_cards: None,
            won_amount: None,
            hand_class: None,
            raw_line: line.to_string(),
        },
        SummarySeatTailAst::ShowedWon {
            shown_cards,
            won_amount,
            hand_class,
        } => SummarySeatOutcome {
            seat_no: head.seat_no,
            player_name: head.player_name,
            position_marker: head.position_marker,
            outcome_kind: SummarySeatOutcomeKind::ShowedWon,
            folded_at: None,
            shown_cards: Some(shown_cards),
            won_amount: Some(won_amount),
            hand_class,
            raw_line: line.to_string(),
        },
        SummarySeatTailAst::ShowedLost {
            shown_cards,
            hand_class,
        } => SummarySeatOutcome {
            seat_no: head.seat_no,
            player_name: head.player_name,
            position_marker: head.position_marker,
            outcome_kind: SummarySeatOutcomeKind::ShowedLost,
            folded_at: None,
            shown_cards: Some(shown_cards),
            won_amount: None,
            hand_class,
            raw_line: line.to_string(),
        },
        SummarySeatTailAst::Lost => SummarySeatOutcome {
            seat_no: head.seat_no,
            player_name: head.player_name,
            position_marker: head.position_marker,
            outcome_kind: SummarySeatOutcomeKind::Lost,
            folded_at: None,
            shown_cards: None,
            won_amount: None,
            hand_class: None,
            raw_line: line.to_string(),
        },
        SummarySeatTailAst::Mucked => SummarySeatOutcome {
            seat_no: head.seat_no,
            player_name: head.player_name,
            position_marker: head.position_marker,
            outcome_kind: SummarySeatOutcomeKind::Mucked,
            folded_at: None,
            shown_cards: None,
            won_amount: None,
            hand_class: None,
            raw_line: line.to_string(),
        },
        SummarySeatTailAst::Won {
            won_amount,
            hand_class,
        } => SummarySeatOutcome {
            seat_no: head.seat_no,
            player_name: head.player_name,
            position_marker: head.position_marker,
            outcome_kind: SummarySeatOutcomeKind::Won,
            folded_at: None,
            shown_cards: None,
            won_amount: Some(won_amount),
            hand_class,
            raw_line: line.to_string(),
        },
        SummarySeatTailAst::Collected { won_amount } => SummarySeatOutcome {
            seat_no: head.seat_no,
            player_name: head.player_name,
            position_marker: head.position_marker,
            outcome_kind: SummarySeatOutcomeKind::Collected,
            folded_at: None,
            shown_cards: None,
            won_amount: Some(won_amount),
            hand_class: None,
            raw_line: line.to_string(),
        },
    }
}

fn parse_summary_folded_tail(tail: &str) -> Option<Street> {
    if tail == "folded before Flop" {
        return Some(Street::Preflop);
    }

    static REGEX: OnceLock<Regex> = OnceLock::new();
    let captures = REGEX
        .get_or_init(|| {
            Regex::new(r"^folded on the (?P<street>Flop|Turn|River)$")
                .expect("summary folded tail regex must compile")
        })
        .captures(tail)?;
    Some(match &captures["street"] {
        "Flop" => Street::Flop,
        "Turn" => Street::Turn,
        "River" => Street::River,
        _ => return None,
    })
}

fn parse_summary_showed_won_tail(tail: &str) -> Option<(Vec<String>, i64, Option<String>)> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let captures = REGEX
        .get_or_init(|| {
            Regex::new(
                r"^showed \[(?P<cards>[^\]]+)\] and won \((?P<amount>[\d,]+)\)(?: with (?P<hand_class>.+))?$",
            )
            .expect("summary showed won tail regex must compile")
        })
        .captures(tail)?;
    Some((
        split_cards(&captures["cards"]),
        parse_i64(&captures["amount"], "summary_won_amount").ok()?,
        captures
            .name("hand_class")
            .map(|hand_class| hand_class.as_str().to_string()),
    ))
}

fn parse_summary_showed_lost_tail(tail: &str) -> Option<(Vec<String>, Option<String>)> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let captures = REGEX
        .get_or_init(|| {
            Regex::new(r"^showed \[(?P<cards>[^\]]+)\] and lost(?: with (?P<hand_class>.+))?$")
                .expect("summary showed lost tail regex must compile")
        })
        .captures(tail)?;
    Some((
        split_cards(&captures["cards"]),
        captures
            .name("hand_class")
            .map(|hand_class| hand_class.as_str().to_string()),
    ))
}

fn parse_summary_showed_collected_tail(tail: &str) -> Option<(Vec<String>, i64, Option<String>)> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let captures = REGEX
        .get_or_init(|| {
            Regex::new(
                r"^showed \[(?P<cards>[^\]]+)\] and collected \((?P<amount>[\d,]+)\)(?: with (?P<hand_class>.+))?$",
            )
            .expect("summary showed collected tail regex must compile")
        })
        .captures(tail)?;
    Some((
        split_cards(&captures["cards"]),
        parse_i64(&captures["amount"], "summary_collected_amount").ok()?,
        captures
            .name("hand_class")
            .map(|hand_class| hand_class.as_str().to_string()),
    ))
}

fn parse_summary_won_tail(tail: &str) -> Option<(i64, Option<String>)> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let captures = REGEX
        .get_or_init(|| {
            Regex::new(r"^won \((?P<amount>[\d,]+)\)(?: with (?P<hand_class>.+))?$")
                .expect("summary won tail regex must compile")
        })
        .captures(tail)?;
    Some((
        parse_i64(&captures["amount"], "summary_won_amount").ok()?,
        captures
            .name("hand_class")
            .map(|hand_class| hand_class.as_str().to_string()),
    ))
}

fn parse_summary_collected_tail(tail: &str) -> Option<i64> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let captures = REGEX
        .get_or_init(|| {
            Regex::new(r"^collected \((?P<amount>[\d,]+)\)$")
                .expect("summary collected regex must compile")
        })
        .captures(tail)?;
    parse_i64(&captures["amount"], "summary_collected_amount").ok()
}

fn parse_uncalled_return(
    line: &str,
    street: Street,
    seq: usize,
) -> Result<Option<HandActionEvent>, ParserError> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = REGEX.get_or_init(|| {
        Regex::new(r"^Uncalled bet \((?P<amount>[\d,]+)\) returned to (?P<player_name>.+)$")
            .expect("uncalled return regex must compile")
    });
    let Some(captures) = regex.captures(line) else {
        return Ok(None);
    };

    Ok(Some(HandActionEvent {
        seq,
        street,
        player_name: Some(captures["player_name"].to_string()),
        action_type: ActionType::ReturnUncalled,
        is_forced: false,
        is_all_in: false,
        all_in_reason: None,
        forced_all_in_preflop: false,
        amount: Some(parse_i64(&captures["amount"], "return_amount")?),
        to_amount: None,
        cards: None,
        raw_line: line.to_string(),
    }))
}

fn parse_show_line(
    line: &str,
    street: Street,
    seq: usize,
) -> Result<Option<(String, Vec<String>, HandActionEvent)>, ParserError> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = REGEX.get_or_init(|| {
        Regex::new(r"^(?P<player_name>.+): shows \[(?P<cards>[^\]]+)\](?: .+)?$")
            .expect("show regex must compile")
    });
    let Some(captures) = regex.captures(line) else {
        return Ok(None);
    };

    let player_name = captures["player_name"].to_string();
    let cards = split_cards(&captures["cards"]);
    let event = HandActionEvent {
        seq,
        street,
        player_name: Some(player_name.clone()),
        action_type: ActionType::Show,
        is_forced: false,
        is_all_in: false,
        all_in_reason: None,
        forced_all_in_preflop: false,
        amount: None,
        to_amount: None,
        cards: Some(cards.clone()),
        raw_line: line.to_string(),
    };

    Ok(Some((player_name, cards, event)))
}

fn parse_collect_line(
    line: &str,
    street: Street,
    seq: usize,
) -> Result<Option<(String, i64, HandActionEvent)>, ParserError> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = REGEX.get_or_init(|| {
        Regex::new(r"^(?P<player_name>.+) collected (?P<amount>[\d,]+) from .+$")
            .expect("collect regex must compile")
    });
    let Some(captures) = regex.captures(line) else {
        return Ok(None);
    };

    let player_name = captures["player_name"].to_string();
    let amount = parse_i64(&captures["amount"], "collect_amount")?;
    let event = HandActionEvent {
        seq,
        street,
        player_name: Some(player_name.clone()),
        action_type: ActionType::Collect,
        is_forced: false,
        is_all_in: false,
        all_in_reason: None,
        forced_all_in_preflop: false,
        amount: Some(amount),
        to_amount: None,
        cards: None,
        raw_line: line.to_string(),
    };

    Ok(Some((player_name, amount, event)))
}

fn parse_player_action_line(
    line: &str,
    street: Street,
    seq: usize,
) -> Result<Option<HandActionEvent>, ParserError> {
    static POST_ANTE_REGEX: OnceLock<Regex> = OnceLock::new();
    static POST_SB_REGEX: OnceLock<Regex> = OnceLock::new();
    static POST_BB_REGEX: OnceLock<Regex> = OnceLock::new();
    static POST_DEAD_REGEX: OnceLock<Regex> = OnceLock::new();
    static FOLD_REGEX: OnceLock<Regex> = OnceLock::new();
    static CHECK_REGEX: OnceLock<Regex> = OnceLock::new();
    static CALL_REGEX: OnceLock<Regex> = OnceLock::new();
    static BET_REGEX: OnceLock<Regex> = OnceLock::new();
    static RAISE_REGEX: OnceLock<Regex> = OnceLock::new();
    static MUCK_REGEX: OnceLock<Regex> = OnceLock::new();

    let forced_patterns = [
        (
            POST_ANTE_REGEX.get_or_init(|| {
                Regex::new(
                    r"^(?P<player_name>.+): posts the ante (?P<amount>[\d,]+)(?: and is all-in)?$",
                )
                .expect("post ante regex must compile")
            }),
            ActionType::PostAnte,
        ),
        (
            POST_SB_REGEX.get_or_init(|| {
                Regex::new(
                    r"^(?P<player_name>.+): posts small blind (?P<amount>[\d,]+)(?: and is all-in)?$",
                )
                .expect("post sb regex must compile")
            }),
            ActionType::PostSb,
        ),
        (
            POST_BB_REGEX.get_or_init(|| {
                Regex::new(
                    r"^(?P<player_name>.+): posts big blind (?P<amount>[\d,]+)(?: and is all-in)?$",
                )
                .expect("post bb regex must compile")
            }),
            ActionType::PostBb,
        ),
        (
            POST_DEAD_REGEX.get_or_init(|| {
                Regex::new(
                    r"^(?P<player_name>.+): posts dead (?P<amount>[\d,]+)(?: and is all-in)?$",
                )
                .expect("post dead regex must compile")
            }),
            ActionType::PostDead,
        ),
    ];

    for (regex, action_type) in forced_patterns {
        if let Some(captures) = regex.captures(line) {
            return Ok(Some(HandActionEvent {
                seq,
                street,
                player_name: Some(captures["player_name"].to_string()),
                action_type,
                is_forced: true,
                is_all_in: line.contains("and is all-in"),
                all_in_reason: None,
                forced_all_in_preflop: false,
                amount: Some(parse_i64(&captures["amount"], "forced_amount")?),
                to_amount: None,
                cards: None,
                raw_line: line.to_string(),
            }));
        }
    }

    let fold_regex = FOLD_REGEX.get_or_init(|| {
        Regex::new(r"^(?P<player_name>.+): folds$").expect("fold regex must compile")
    });
    if let Some(captures) = fold_regex.captures(line) {
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::Fold,
            is_forced: false,
            is_all_in: false,
            all_in_reason: None,
            forced_all_in_preflop: false,
            amount: None,
            to_amount: None,
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    let check_regex = CHECK_REGEX.get_or_init(|| {
        Regex::new(r"^(?P<player_name>.+): checks$").expect("check regex must compile")
    });
    if let Some(captures) = check_regex.captures(line) {
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::Check,
            is_forced: false,
            is_all_in: false,
            all_in_reason: None,
            forced_all_in_preflop: false,
            amount: None,
            to_amount: None,
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    let call_regex = CALL_REGEX.get_or_init(|| {
        Regex::new(r"^(?P<player_name>.+): calls (?P<amount>[\d,]+)(?: and is all-in)?$")
            .expect("call regex must compile")
    });
    if let Some(captures) = call_regex.captures(line) {
        let amount = parse_i64(&captures["amount"], "call_amount")?;
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::Call,
            is_forced: false,
            is_all_in: line.contains("and is all-in"),
            all_in_reason: None,
            forced_all_in_preflop: false,
            amount: Some(amount),
            to_amount: None,
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    let bet_regex = BET_REGEX.get_or_init(|| {
        Regex::new(r"^(?P<player_name>.+): bets (?P<amount>[\d,]+)(?: and is all-in)?$")
            .expect("bet regex must compile")
    });
    if let Some(captures) = bet_regex.captures(line) {
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::Bet,
            is_forced: false,
            is_all_in: line.contains("and is all-in"),
            all_in_reason: None,
            forced_all_in_preflop: false,
            amount: Some(parse_i64(&captures["amount"], "bet_amount")?),
            to_amount: None,
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    let raise_regex = RAISE_REGEX.get_or_init(|| {
        Regex::new(
            r"^(?P<player_name>.+): raises (?P<amount>[\d,]+) to (?P<to_amount>[\d,]+)(?: and is all-in)?$",
        )
        .expect("raise regex must compile")
    });
    if let Some(captures) = raise_regex.captures(line) {
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::RaiseTo,
            is_forced: false,
            is_all_in: line.contains("and is all-in"),
            all_in_reason: None,
            forced_all_in_preflop: false,
            amount: Some(parse_i64(&captures["amount"], "raise_amount")?),
            to_amount: Some(parse_i64(&captures["to_amount"], "raise_to_amount")?),
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    let muck_regex = MUCK_REGEX.get_or_init(|| {
        Regex::new(r"^(?P<player_name>.+): mucks(?: hand)?$").expect("muck regex must compile")
    });
    if let Some(captures) = muck_regex.captures(line) {
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::Muck,
            is_forced: false,
            is_all_in: false,
            all_in_reason: None,
            forced_all_in_preflop: false,
            amount: None,
            to_amount: None,
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    Ok(None)
}

fn is_no_show_line(line: &str) -> bool {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"^.+: doesn't show hand$").expect("no-show regex must compile"))
        .is_match(line)
}

fn annotate_partial_reveal_parse_issues(
    parse_issues: &mut Vec<ParseIssue>,
    actions: &[HandActionEvent],
    summary_seat_outcomes: &[SummarySeatOutcome],
) {
    for action in actions {
        if action.action_type == ActionType::Show
            && action.cards.as_ref().is_some_and(|cards| cards.len() != 2)
        {
            parse_issues.push(raw_line_warning(
                ParseIssueCode::PartialRevealShowLine,
                &action.raw_line,
            ));
        }
    }

    for outcome in summary_seat_outcomes {
        if outcome
            .shown_cards
            .as_ref()
            .is_some_and(|cards| cards.len() != 2)
        {
            parse_issues.push(raw_line_warning(
                ParseIssueCode::PartialRevealSummaryShowSurface,
                &outcome.raw_line,
            ));
        }
    }
}

fn annotate_action_all_in_metadata(
    seats: &[ParsedHandSeat],
    actions: &mut [HandActionEvent],
) -> Result<(), ParserError> {
    let mut stack_current = seats
        .iter()
        .map(|seat| (seat.player_name.clone(), seat.starting_stack))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut betting_round_contrib = seats
        .iter()
        .map(|seat| (seat.player_name.clone(), 0_i64))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut current_street = Street::Preflop;

    for action in actions {
        if is_betting_street(current_street)
            && is_betting_street(action.street)
            && action.street != current_street
        {
            for amount in betting_round_contrib.values_mut() {
                *amount = 0;
            }
        }
        current_street = action.street;

        let Some(player_name) = action.player_name.as_ref() else {
            continue;
        };

        let before_contrib = betting_round_contrib[player_name];
        let mut delta = 0_i64;
        let mut contributes_to_betting_round = false;

        match action.action_type {
            ActionType::PostAnte => {
                delta = action.amount.unwrap_or(0);
            }
            ActionType::PostSb | ActionType::PostBb | ActionType::PostDead => {
                delta = action.amount.unwrap_or(0);
                contributes_to_betting_round = true;
            }
            ActionType::Call | ActionType::Bet => {
                delta = action.amount.unwrap_or(0);
                contributes_to_betting_round = true;
            }
            ActionType::RaiseTo => {
                let to_amount = action.to_amount.ok_or(ParserError::InvalidField {
                    field: "to_amount",
                    value: action.raw_line.clone(),
                })?;
                delta = (to_amount - before_contrib).max(0);
                contributes_to_betting_round = true;
            }
            ActionType::ReturnUncalled => {
                let refund = action.amount.unwrap_or(0);
                let stack_current = stack_current.get_mut(player_name).unwrap();
                let betting_round_contrib = betting_round_contrib.get_mut(player_name).unwrap();
                let _ = apply_refund(stack_current, None, None, betting_round_contrib, refund);
                continue;
            }
            ActionType::Fold
            | ActionType::Check
            | ActionType::Collect
            | ActionType::Show
            | ActionType::Muck => {}
        }

        if delta > 0 {
            let stack_current = stack_current.get_mut(player_name).unwrap();
            if apply_debit(stack_current, delta).is_ok() && contributes_to_betting_round {
                *betting_round_contrib.get_mut(player_name).unwrap() += delta;
            }
        }

        let exhausted_stack = stack_current[player_name] == 0;
        if action.is_all_in || exhausted_stack {
            action.is_all_in = true;
            action.all_in_reason = Some(match action.action_type {
                ActionType::PostAnte => AllInReason::AnteExhausted,
                ActionType::PostSb | ActionType::PostBb | ActionType::PostDead => {
                    AllInReason::BlindExhausted
                }
                ActionType::Call => AllInReason::CallExhausted,
                ActionType::RaiseTo => AllInReason::RaiseExhausted,
                ActionType::Bet => AllInReason::Voluntary,
                ActionType::Fold
                | ActionType::Check
                | ActionType::ReturnUncalled
                | ActionType::Collect
                | ActionType::Show
                | ActionType::Muck => continue,
            });
            action.forced_all_in_preflop = matches!(
                action.all_in_reason,
                Some(AllInReason::BlindExhausted | AllInReason::AnteExhausted)
            ) && action.street == Street::Preflop;
        }
    }

    Ok(())
}

fn is_betting_street(street: Street) -> bool {
    matches!(
        street,
        Street::Preflop | Street::Flop | Street::Turn | Street::River
    )
}

fn split_cards(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_string).collect()
}

fn parse_i64(raw: &str, field: &'static str) -> Result<i64, ParserError> {
    raw.trim()
        .replace(',', "")
        .parse::<i64>()
        .map_err(|_| invalid_field(field, raw))
}
