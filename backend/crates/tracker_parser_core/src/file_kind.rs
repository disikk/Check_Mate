use crate::ParserError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    HandHistory,
    TournamentSummary,
}

pub fn detect_source_kind(input: &str) -> Result<SourceKind, ParserError> {
    let first_line = input
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or(ParserError::EmptySource)?;

    if first_line.starts_with("Poker Hand #") {
        return Ok(SourceKind::HandHistory);
    }

    if first_line.starts_with("Tournament #") {
        return Ok(SourceKind::TournamentSummary);
    }

    Err(ParserError::UnsupportedSourceFormat)
}
