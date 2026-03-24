use crate::{
    ParserError,
    models::{
        ActionType, CanonicalParsedHand, HandActionEvent, HandHeader, HandRecord, ParsedHandSeat,
        Street,
    },
};

use regex::Regex;

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
    let normalized = normalize_newlines(hand_text);
    let mut lines = normalized.lines().filter(|line| !line.trim().is_empty());

    let first_line = lines
        .next()
        .ok_or(ParserError::MissingLine("hand header line"))?;
    let second_line = lines.next().ok_or(ParserError::MissingLine("table line"))?;

    let first_regex = Regex::new(
        r"^Poker Hand #(?P<hand_id>[^:]+): Tournament #(?P<tournament_id>\d+), (?P<game_name>.+) - (?P<level_name>Level\d+)\((?P<small_blind>[\d,]+)/(?P<big_blind>[\d,]+)\((?P<ante>[\d,]+)\)\) - (?P<played_at>\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2})$",
    )
    .expect("hand header regex must compile");
    let second_regex = Regex::new(
        r"^Table '(?P<table_name>[^']+)' (?P<max_players>\d+)-max Seat #(?P<button_seat>\d+) is the button$",
    )
    .expect("table regex must compile");

    let header_caps = first_regex
        .captures(first_line)
        .ok_or_else(|| invalid_field("hand_header_line", first_line))?;
    let table_caps = second_regex
        .captures(second_line)
        .ok_or_else(|| invalid_field("table_line", second_line))?;

    Ok(HandHeader {
        hand_id: header_caps["hand_id"].to_string(),
        tournament_id: parse_u64(&header_caps["tournament_id"], "tournament_id")?,
        game_name: header_caps["game_name"].to_string(),
        level_name: header_caps["level_name"].to_string(),
        small_blind: parse_u32(&header_caps["small_blind"], "small_blind")?,
        big_blind: parse_u32(&header_caps["big_blind"], "big_blind")?,
        ante: parse_u32(&header_caps["ante"], "ante")?,
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
    let mut collected_amounts = std::collections::BTreeMap::new();
    let mut parse_warnings = Vec::new();
    let mut street = Street::Preflop;
    let mut seq = 0usize;

    for line in normalized.lines().skip(2) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(seat) = parse_seat_line(line)? {
            seats.push(seat);
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

        if parse_hidden_dealt_to_line(line) {
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

        if !line.starts_with("Seat ") {
            parse_warnings.push(format!("unparsed_line: {line}"));
        }
    }

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
        collected_amounts,
        raw_hand_text: normalized,
        parse_warnings,
    })
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
    let regex =
        Regex::new(r"^Seat (?P<seat_no>\d+): (?P<player_name>.+) \((?P<stack>[\d,]+) in chips\)$")
            .expect("seat regex must compile");
    let Some(captures) = regex.captures(line) else {
        return Ok(None);
    };

    Ok(Some(ParsedHandSeat {
        seat_no: parse_u8(&captures["seat_no"], "seat_no")?,
        player_name: captures["player_name"].to_string(),
        starting_stack: parse_i64(&captures["stack"], "starting_stack")?,
    }))
}

fn parse_dealt_to_line(line: &str) -> Option<(String, Vec<String>)> {
    let regex = Regex::new(r"^Dealt to (?P<player_name>.+) \[(?P<cards>[^\]]+)\]$")
        .expect("dealt-to regex must compile");
    let captures = regex.captures(line)?;
    Some((
        captures["player_name"].to_string(),
        split_cards(&captures["cards"]),
    ))
}

fn parse_hidden_dealt_to_line(line: &str) -> bool {
    let regex = Regex::new(r"^Dealt to (?P<player_name>.+?)\s*$")
        .expect("hidden dealt-to regex must compile");
    regex.is_match(line)
}

fn parse_board_transition(line: &str) -> Option<(Street, Vec<String>)> {
    let flop_regex =
        Regex::new(r"^\*\*\* FLOP \*\*\* \[(?P<cards>[^\]]+)\]$").expect("flop regex must compile");
    if let Some(captures) = flop_regex.captures(line) {
        return Some((Street::Flop, split_cards(&captures["cards"])));
    }

    let turn_regex = Regex::new(r"^\*\*\* TURN \*\*\* \[[^\]]+\] \[(?P<card>[^\]]+)\]$")
        .expect("turn regex must compile");
    if let Some(captures) = turn_regex.captures(line) {
        return Some((Street::Turn, vec![captures["card"].to_string()]));
    }

    let river_regex = Regex::new(r"^\*\*\* RIVER \*\*\* \[[^\]]+\] \[(?P<card>[^\]]+)\]$")
        .expect("river regex must compile");
    if let Some(captures) = river_regex.captures(line) {
        return Some((Street::River, vec![captures["card"].to_string()]));
    }

    None
}

fn parse_summary_total_line(line: &str) -> Result<Option<(i64, i64)>, ParserError> {
    let regex =
        Regex::new(r"^Total pot (?P<total_pot>[\d,]+) \| Rake (?P<rake_amount>[\d,]+)(?: \| .+)?$")
            .expect("summary total regex must compile");
    let Some(captures) = regex.captures(line) else {
        return Ok(None);
    };

    Ok(Some((
        parse_i64(&captures["total_pot"], "summary_total_pot")?,
        parse_i64(&captures["rake_amount"], "summary_rake_amount")?,
    )))
}

fn parse_summary_board_line(line: &str) -> Option<Vec<String>> {
    let regex =
        Regex::new(r"^Board \[(?P<cards>[^\]]+)\]$").expect("summary board regex must compile");
    let captures = regex.captures(line)?;
    Some(split_cards(&captures["cards"]))
}

fn parse_uncalled_return(
    line: &str,
    street: Street,
    seq: usize,
) -> Result<Option<HandActionEvent>, ParserError> {
    let regex =
        Regex::new(r"^Uncalled bet \((?P<amount>[\d,]+)\) returned to (?P<player_name>.+)$")
            .expect("uncalled return regex must compile");
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
    let regex = Regex::new(r"^(?P<player_name>.+): shows \[(?P<cards>[^\]]+)\](?: .+)?$")
        .expect("show regex must compile");
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
    let regex = Regex::new(r"^(?P<player_name>.+) collected (?P<amount>[\d,]+) from .+$")
        .expect("collect regex must compile");
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
    let forced_patterns = [
        (
            Regex::new(
                r"^(?P<player_name>.+): posts the ante (?P<amount>[\d,]+)(?: and is all-in)?$",
            )
            .expect("post ante regex must compile"),
            ActionType::PostAnte,
        ),
        (
            Regex::new(
                r"^(?P<player_name>.+): posts small blind (?P<amount>[\d,]+)(?: and is all-in)?$",
            )
            .expect("post sb regex must compile"),
            ActionType::PostSb,
        ),
        (
            Regex::new(
                r"^(?P<player_name>.+): posts big blind (?P<amount>[\d,]+)(?: and is all-in)?$",
            )
            .expect("post bb regex must compile"),
            ActionType::PostBb,
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
                amount: Some(parse_i64(&captures["amount"], "forced_amount")?),
                to_amount: None,
                cards: None,
                raw_line: line.to_string(),
            }));
        }
    }

    let fold_regex = Regex::new(r"^(?P<player_name>.+): folds$").expect("fold regex must compile");
    if let Some(captures) = fold_regex.captures(line) {
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::Fold,
            is_forced: false,
            is_all_in: false,
            amount: None,
            to_amount: None,
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    let check_regex =
        Regex::new(r"^(?P<player_name>.+): checks$").expect("check regex must compile");
    if let Some(captures) = check_regex.captures(line) {
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::Check,
            is_forced: false,
            is_all_in: false,
            amount: None,
            to_amount: None,
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    let call_regex =
        Regex::new(r"^(?P<player_name>.+): calls (?P<amount>[\d,]+)(?: and is all-in)?$")
            .expect("call regex must compile");
    if let Some(captures) = call_regex.captures(line) {
        let amount = parse_i64(&captures["amount"], "call_amount")?;
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::Call,
            is_forced: false,
            is_all_in: line.contains("and is all-in"),
            amount: Some(amount),
            to_amount: Some(amount),
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    let bet_regex =
        Regex::new(r"^(?P<player_name>.+): bets (?P<amount>[\d,]+)(?: and is all-in)?$")
            .expect("bet regex must compile");
    if let Some(captures) = bet_regex.captures(line) {
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::Bet,
            is_forced: false,
            is_all_in: line.contains("and is all-in"),
            amount: Some(parse_i64(&captures["amount"], "bet_amount")?),
            to_amount: None,
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    let raise_regex = Regex::new(
        r"^(?P<player_name>.+): raises (?P<amount>[\d,]+) to (?P<to_amount>[\d,]+)(?: and is all-in)?$",
    )
    .expect("raise regex must compile");
    if let Some(captures) = raise_regex.captures(line) {
        return Ok(Some(HandActionEvent {
            seq,
            street,
            player_name: Some(captures["player_name"].to_string()),
            action_type: ActionType::RaiseTo,
            is_forced: false,
            is_all_in: line.contains("and is all-in"),
            amount: Some(parse_i64(&captures["amount"], "raise_amount")?),
            to_amount: Some(parse_i64(&captures["to_amount"], "raise_to_amount")?),
            cards: None,
            raw_line: line.to_string(),
        }));
    }

    Ok(None)
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
