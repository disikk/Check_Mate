pub mod betting_rules;
mod error;
mod file_kind;
pub mod models;
pub mod normalizer;
pub mod parsers;
pub mod positions;
mod pot_resolution;
pub mod street_strength;

pub use error::ParserError;
pub use file_kind::{SourceKind, detect_source_kind};

pub const EXACT_CORE_RESOLUTION_VERSION: &str = "gg_mbr_v2";
