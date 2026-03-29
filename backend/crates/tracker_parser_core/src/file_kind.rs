use crate::ParserError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    HandHistory,
    TournamentSummary,
}

pub fn quick_detect_source_kind(input: &str) -> Result<SourceKind, ParserError> {
    detect_source_kind_from_line(first_non_empty_line(input)?)
}

pub fn detect_source_kind(input: &str) -> Result<SourceKind, ParserError> {
    quick_detect_source_kind(input)
}

pub fn quick_extract_gg_tournament_id(input: &str) -> Result<Option<u64>, ParserError> {
    let first_line = first_non_empty_line(input)?;

    match detect_source_kind_from_line(first_line)? {
        SourceKind::HandHistory => {
            let Some((_, suffix)) = first_line.split_once("Tournament #") else {
                return Ok(None);
            };
            parse_leading_tournament_id(suffix)
        }
        SourceKind::TournamentSummary => {
            let Some(suffix) = first_line.strip_prefix("Tournament #") else {
                return Ok(None);
            };
            parse_leading_tournament_id(suffix)
        }
    }
}

fn first_non_empty_line(input: &str) -> Result<&str, ParserError> {
    input
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or(ParserError::EmptySource)
}

fn detect_source_kind_from_line(first_line: &str) -> Result<SourceKind, ParserError> {
    if first_line.starts_with("Poker Hand #") {
        return Ok(SourceKind::HandHistory);
    }

    if first_line.starts_with("Tournament #") {
        return Ok(SourceKind::TournamentSummary);
    }

    Err(ParserError::UnsupportedSourceFormat)
}

fn parse_leading_tournament_id(suffix: &str) -> Result<Option<u64>, ParserError> {
    let digits: String = suffix
        .chars()
        .take_while(|value| value.is_ascii_digit())
        .collect();

    if digits.is_empty() {
        return Ok(None);
    }

    digits
        .parse::<u64>()
        .map(Some)
        .map_err(|_| ParserError::InvalidField {
            field: "tournament_id",
            value: digits,
        })
}
