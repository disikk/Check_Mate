mod archive;
mod pair;
mod scan;

use std::{path::Path, time::Instant};

use anyhow::Result;
use serde::Serialize;

pub use crate::archive::decode_archive_member_path;
use crate::{pair::build_prepare_report, scan::scan_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum PreparedSourceKind {
    HandHistory,
    TournamentSummary,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreparedFileRef {
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_path: Option<String>,
    pub source_kind: PreparedSourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tournament_id: Option<String>,
    pub byte_size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreparedTournamentPair {
    pub tournament_id: String,
    pub ts: PreparedFileRef,
    pub hh: PreparedFileRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReasonCode {
    MissingTs,
    MissingHh,
    ConflictingTs,
    ConflictingHh,
    UnsupportedSource,
    MissingTournamentId,
    DuplicateSameContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RejectedTournament {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tournament_id: Option<String>,
    pub files: Vec<PreparedFileRef>,
    pub reason_code: RejectReasonCode,
    pub reason_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrepareReport {
    pub scanned_files: usize,
    pub paired_tournaments: Vec<PreparedTournamentPair>,
    pub rejected_tournaments: Vec<RejectedTournament>,
    pub scan_ms: u64,
    pub pair_ms: u64,
    pub hash_ms: u64,
}

pub fn prepare_path(path: impl AsRef<Path>) -> Result<PrepareReport> {
    let scan_started_at = Instant::now();
    let scan_outcome = scan_path(path.as_ref())?;
    let scan_ms = scan_started_at.elapsed().as_millis() as u64;

    let pair_started_at = Instant::now();
    let pair_outcome = build_prepare_report(scan_outcome.entries)?;
    let pair_ms = pair_started_at.elapsed().as_millis() as u64;

    Ok(PrepareReport {
        scanned_files: scan_outcome.scanned_files,
        paired_tournaments: pair_outcome.paired_tournaments,
        rejected_tournaments: pair_outcome.rejected_tournaments,
        scan_ms,
        pair_ms,
        hash_ms: pair_outcome.hash_ms,
    })
}
