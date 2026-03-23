use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParserError {
    #[error("source is empty")]
    EmptySource,
    #[error("unsupported source format")]
    UnsupportedSourceFormat,
    #[error("missing line: {0}")]
    MissingLine(&'static str),
    #[error("invalid field `{field}`: {value}")]
    InvalidField { field: &'static str, value: String },
}
