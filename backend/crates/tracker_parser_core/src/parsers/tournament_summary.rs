use regex::Regex;

use crate::{
    ParserError,
    models::{ParseIssue, ParseIssueCode, ParseIssuePayload, TournamentSummary},
};

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
    let buy_in_line = find_line(&lines, |line| line.starts_with("Buy-in: "))
        .ok_or(ParserError::MissingLine("buy-in line"))?;
    let entrants_line = find_line(&lines, |line| line.ends_with(" Players"))
        .ok_or(ParserError::MissingLine("entrants line"))?;
    let prize_pool_line = find_line(&lines, |line| line.starts_with("Total Prize Pool: "))
        .ok_or(ParserError::MissingLine("prize pool line"))?;
    let started_line = find_line(&lines, |line| line.starts_with("Tournament started "))
        .ok_or(ParserError::MissingLine("started line"))?;
    let result_regex =
        Regex::new(r"^(?P<place>\d+)(?:st|nd|rd|th)\s*:\s*(?P<hero>.+),\s*(?P<payout>\$[\d.,]+)$")
            .expect("result regex must compile");
    let (result_line_index, result_line) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| result_regex.is_match(line))
        .map(|(index, line)| (index, *line))
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

    let (buy_in_cents, rake_cents, bounty_cents) = parse_buy_in_components(buy_in_line)?;

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

    let captures = result_regex
        .captures(result_line)
        .ok_or_else(|| invalid_field("result_line", result_line))?;
    let confirmed_finish_place =
        find_confirmation_finish_place(lines.iter().skip(result_line_index + 1).copied())?;
    let confirmed_payout_cents =
        find_confirmation_payout(lines.iter().skip(result_line_index + 1).copied())?;
    let finish_place = parse_u32(&captures["place"], "finish_place")?;
    let payout_cents = parse_money_to_cents(&captures["payout"], "payout")?;
    let mut parse_issues = Vec::new();

    if let Some(confirmed) = confirmed_finish_place
        && confirmed != finish_place
    {
        parse_issues.push(ParseIssue::warning(
            ParseIssueCode::TsTailFinishPlaceMismatch,
            format!(
                "result line finish_place={} conflicts with tail finish_place={confirmed}",
                finish_place
            ),
            None,
            Some(ParseIssuePayload::TsTailFinishPlaceMismatch {
                result_finish_place: finish_place,
                tail_finish_place: confirmed,
            }),
        ));
    }
    if let Some(confirmed) = confirmed_payout_cents
        && confirmed != payout_cents
    {
        parse_issues.push(ParseIssue::warning(
            ParseIssueCode::TsTailTotalReceivedMismatch,
            format!(
                "result line payout_cents={} conflicts with tail payout_cents={confirmed}",
                payout_cents
            ),
            None,
            Some(ParseIssuePayload::TsTailTotalReceivedMismatch {
                result_payout_cents: payout_cents,
                tail_payout_cents: confirmed,
            }),
        ));
    }

    Ok(TournamentSummary {
        tournament_id: parse_u64(tournament_id_raw, "tournament_id")?,
        tournament_name: tournament_name.to_string(),
        game_name: game_name.to_string(),
        buy_in_cents,
        rake_cents,
        bounty_cents,
        entrants,
        total_prize_pool_cents,
        started_at,
        hero_name: captures["hero"].to_string(),
        finish_place,
        payout_cents,
        confirmed_finish_place,
        confirmed_payout_cents,
        parse_issues,
    })
}

fn find_line<'a>(lines: &'a [&str], predicate: impl Fn(&str) -> bool) -> Option<&'a str> {
    lines.iter().copied().find(|line| predicate(line))
}

fn parse_buy_in_components(line: &str) -> Result<(i64, i64, i64), ParserError> {
    let buy_in_regex = Regex::new(
        r"^Buy-in:\s*(?P<buy_in>\$[\d.,]+(?:\.\d+)?)\s*\+\s*(?P<rake>\$[\d.,]+(?:\.\d+)?)\s*\+\s*(?P<bounty>\$[\d.,]+(?:\.\d+)?)$",
    )
    .expect("buy-in regex must compile");
    let captures = buy_in_regex
        .captures(line)
        .ok_or_else(|| invalid_field("buy_in_line", line))?;

    Ok((
        parse_money_to_cents(&captures["buy_in"], "buy_in")?,
        parse_money_to_cents(&captures["rake"], "rake")?,
        parse_money_to_cents(&captures["bounty"], "bounty")?,
    ))
}

fn find_confirmation_finish_place<'a>(
    mut tail_lines: impl Iterator<Item = &'a str>,
) -> Result<Option<u32>, ParserError> {
    let finish_regex =
        Regex::new(r"^You finished the tournament in (?P<place>\d+)(?:st|nd|rd|th) place\.?$")
            .expect("finish confirmation regex must compile");

    tail_lines
        .find_map(|line| finish_regex.captures(line))
        .map(|captures| parse_u32(&captures["place"], "confirmed_finish_place"))
        .transpose()
}

fn find_confirmation_payout<'a>(
    mut tail_lines: impl Iterator<Item = &'a str>,
) -> Result<Option<i64>, ParserError> {
    let payout_regex = Regex::new(r"^You received a total of (?P<payout>\$[\d.,]+)\.?$")
        .expect("payout confirmation regex must compile");

    tail_lines
        .find_map(|line| payout_regex.captures(line))
        .map(|captures| parse_money_to_cents(&captures["payout"], "confirmed_payout"))
        .transpose()
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
