use regex::Regex;

use crate::{ParserError, models::TournamentSummary};

pub fn parse_tournament_summary(input: &str) -> Result<TournamentSummary, ParserError> {
    let normalized = normalize_newlines(input);
    let lines: Vec<&str> = normalized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    let title_line = *lines
        .first()
        .ok_or(ParserError::MissingLine("tournament summary title"))?;
    let buy_in_line = *lines
        .get(1)
        .ok_or(ParserError::MissingLine("buy-in line"))?;
    let entrants_line = *lines
        .get(2)
        .ok_or(ParserError::MissingLine("entrants line"))?;
    let prize_pool_line = *lines
        .get(3)
        .ok_or(ParserError::MissingLine("prize pool line"))?;
    let started_line = *lines
        .get(4)
        .ok_or(ParserError::MissingLine("started line"))?;
    let result_line = *lines
        .get(5)
        .ok_or(ParserError::MissingLine("result line"))?;

    let title_body = title_line
        .strip_prefix("Tournament #")
        .ok_or_else(|| invalid_field("title_line", title_line))?;
    let (tournament_id_raw, name_and_game) = title_body
        .split_once(", ")
        .ok_or_else(|| invalid_field("title_line", title_line))?;
    let (tournament_name, game_name) = name_and_game
        .rsplit_once(", ")
        .ok_or_else(|| invalid_field("title_line", title_line))?;

    let buy_in_body = buy_in_line
        .strip_prefix("Buy-in: ")
        .ok_or_else(|| invalid_field("buy_in_line", buy_in_line))?;
    let buy_in_parts: Vec<&str> = buy_in_body.split('+').collect();
    if buy_in_parts.len() != 3 {
        return Err(invalid_field("buy_in_line", buy_in_line));
    }

    let entrants = entrants_line
        .strip_suffix(" Players")
        .ok_or_else(|| invalid_field("entrants_line", entrants_line))
        .and_then(|value| parse_u32(value, "entrants"))?;

    let total_prize_pool_cents = prize_pool_line
        .strip_prefix("Total Prize Pool: ")
        .ok_or_else(|| invalid_field("prize_pool_line", prize_pool_line))
        .and_then(|value| parse_money_to_cents(value, "total_prize_pool"))?;

    let started_at = started_line
        .strip_prefix("Tournament started ")
        .ok_or_else(|| invalid_field("started_line", started_line))?
        .trim()
        .to_string();

    let result_regex =
        Regex::new(r"^(?P<place>\d+)(?:st|nd|rd|th)\s*:\s*(?P<hero>.+),\s*(?P<payout>\$[\d.,]+)$")
            .expect("result regex must compile");
    let captures = result_regex
        .captures(result_line)
        .ok_or_else(|| invalid_field("result_line", result_line))?;

    Ok(TournamentSummary {
        tournament_id: parse_u64(tournament_id_raw, "tournament_id")?,
        tournament_name: tournament_name.to_string(),
        game_name: game_name.to_string(),
        buy_in_cents: parse_money_to_cents(buy_in_parts[0], "buy_in")?,
        rake_cents: parse_money_to_cents(buy_in_parts[1], "rake")?,
        bounty_cents: parse_money_to_cents(buy_in_parts[2], "bounty")?,
        entrants,
        total_prize_pool_cents,
        started_at,
        hero_name: captures["hero"].to_string(),
        finish_place: parse_u32(&captures["place"], "finish_place")?,
        payout_cents: parse_money_to_cents(&captures["payout"], "payout")?,
    })
}

fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

fn parse_money_to_cents(raw: &str, field: &'static str) -> Result<i64, ParserError> {
    let cleaned = raw.trim().trim_start_matches('$').replace(',', "");
    let (whole, fractional) = match cleaned.split_once('.') {
        Some((whole, fractional)) => (whole, fractional),
        None => (cleaned.as_str(), "0"),
    };

    let normalized_fractional = match fractional.len() {
        0 => "00".to_string(),
        1 => format!("{fractional}0"),
        2 => fractional.to_string(),
        _ => return Err(invalid_field(field, raw)),
    };

    let whole_part = whole
        .parse::<i64>()
        .map_err(|_| invalid_field(field, raw))?;
    let fractional_part = normalized_fractional
        .parse::<i64>()
        .map_err(|_| invalid_field(field, raw))?;

    Ok((whole_part * 100) + fractional_part)
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
