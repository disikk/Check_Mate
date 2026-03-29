use std::{collections::BTreeMap, time::Instant};

use anyhow::Result;

use crate::{
    PreparedSourceKind, PreparedTournamentPair, RejectReasonCode, RejectedTournament,
    scan::{PreparedCandidate, PreparedScanEntry, ensure_sha256},
};

#[derive(Debug)]
pub(crate) struct PairOutcome {
    pub paired_tournaments: Vec<PreparedTournamentPair>,
    pub rejected_tournaments: Vec<RejectedTournament>,
    pub hash_ms: u64,
}

#[derive(Default)]
struct TournamentBucket {
    hh: Vec<PreparedCandidate>,
    ts: Vec<PreparedCandidate>,
}

enum CandidateResolution {
    Missing,
    Ready(PreparedCandidate),
    Conflict(Vec<PreparedCandidate>),
}

pub(crate) fn build_prepare_report(entries: Vec<PreparedScanEntry>) -> Result<PairOutcome> {
    let mut rejected_tournaments = Vec::new();
    let mut buckets = BTreeMap::<String, TournamentBucket>::new();
    let mut hash_ms = 0u64;

    for entry in entries {
        match entry {
            PreparedScanEntry::Rejected(rejected) => rejected_tournaments.push(rejected),
            PreparedScanEntry::Candidate(candidate) => {
                let tournament_id = candidate
                    .file
                    .tournament_id
                    .clone()
                    .expect("candidate tournament_id must be present");
                let bucket = buckets.entry(tournament_id).or_default();
                match candidate.file.source_kind {
                    PreparedSourceKind::HandHistory => bucket.hh.push(candidate),
                    PreparedSourceKind::TournamentSummary => bucket.ts.push(candidate),
                    PreparedSourceKind::Unknown => unreachable!("unknown source kind cannot pair"),
                }
            }
        }
    }

    let mut paired_tournaments = Vec::new();
    for (tournament_id, bucket) in buckets {
        let hh = resolve_candidates(bucket.hh, &mut hash_ms)?;
        let ts = resolve_candidates(bucket.ts, &mut hash_ms)?;

        match (ts, hh) {
            (CandidateResolution::Ready(mut ts), CandidateResolution::Ready(mut hh)) => {
                let started_at = Instant::now();
                ensure_sha256(&mut ts)?;
                ensure_sha256(&mut hh)?;
                hash_ms += started_at.elapsed().as_millis() as u64;
                paired_tournaments.push(PreparedTournamentPair {
                    tournament_id,
                    ts: ts.file,
                    hh: hh.file,
                });
            }
            (CandidateResolution::Conflict(ts_files), CandidateResolution::Ready(hh)) => {
                let mut files = ts_files
                    .into_iter()
                    .map(|item| item.file)
                    .collect::<Vec<_>>();
                files.push(hh.file);
                rejected_tournaments.push(RejectedTournament {
                    tournament_id: Some(tournament_id),
                    files,
                    reason_code: RejectReasonCode::ConflictingTs,
                    reason_text: "Multiple tournament summary files with conflicting content"
                        .to_string(),
                });
            }
            (CandidateResolution::Conflict(ts_files), CandidateResolution::Missing) => {
                let files = ts_files
                    .into_iter()
                    .map(|item| item.file)
                    .collect::<Vec<_>>();
                rejected_tournaments.push(RejectedTournament {
                    tournament_id: Some(tournament_id),
                    files,
                    reason_code: RejectReasonCode::ConflictingTs,
                    reason_text: "Multiple tournament summary files with conflicting content"
                        .to_string(),
                });
            }
            (CandidateResolution::Ready(ts), CandidateResolution::Conflict(hh_files)) => {
                let mut files = hh_files
                    .into_iter()
                    .map(|item| item.file)
                    .collect::<Vec<_>>();
                files.push(ts.file);
                rejected_tournaments.push(RejectedTournament {
                    tournament_id: Some(tournament_id),
                    files,
                    reason_code: RejectReasonCode::ConflictingHh,
                    reason_text: "Multiple hand history files with conflicting content".to_string(),
                });
            }
            (CandidateResolution::Missing, CandidateResolution::Conflict(hh_files)) => {
                let files = hh_files
                    .into_iter()
                    .map(|item| item.file)
                    .collect::<Vec<_>>();
                rejected_tournaments.push(RejectedTournament {
                    tournament_id: Some(tournament_id),
                    files,
                    reason_code: RejectReasonCode::ConflictingHh,
                    reason_text: "Multiple hand history files with conflicting content".to_string(),
                });
            }
            (CandidateResolution::Missing, CandidateResolution::Ready(hh)) => {
                rejected_tournaments.push(RejectedTournament {
                    tournament_id: Some(tournament_id),
                    files: vec![hh.file],
                    reason_code: RejectReasonCode::MissingTs,
                    reason_text: "Found HH without matching TS".to_string(),
                });
            }
            (CandidateResolution::Ready(ts), CandidateResolution::Missing) => {
                rejected_tournaments.push(RejectedTournament {
                    tournament_id: Some(tournament_id),
                    files: vec![ts.file],
                    reason_code: RejectReasonCode::MissingHh,
                    reason_text: "Found TS without matching HH".to_string(),
                });
            }
            (CandidateResolution::Missing, CandidateResolution::Missing) => {}
            (CandidateResolution::Conflict(ts_files), CandidateResolution::Conflict(hh_files)) => {
                let mut files = ts_files
                    .into_iter()
                    .map(|item| item.file)
                    .collect::<Vec<_>>();
                files.extend(hh_files.into_iter().map(|item| item.file));
                rejected_tournaments.push(RejectedTournament {
                    tournament_id: Some(tournament_id),
                    files,
                    reason_code: RejectReasonCode::ConflictingTs,
                    reason_text: "Tournament contains conflicting TS and HH files".to_string(),
                });
            }
        }
    }

    Ok(PairOutcome {
        paired_tournaments,
        rejected_tournaments,
        hash_ms,
    })
}

fn resolve_candidates(
    mut candidates: Vec<PreparedCandidate>,
    hash_ms: &mut u64,
) -> Result<CandidateResolution> {
    if candidates.is_empty() {
        return Ok(CandidateResolution::Missing);
    }
    if candidates.len() == 1 {
        return Ok(CandidateResolution::Ready(candidates.remove(0)));
    }

    let started_at = Instant::now();
    for candidate in &mut candidates {
        ensure_sha256(candidate)?;
    }
    *hash_ms += started_at.elapsed().as_millis() as u64;

    let mut by_sha = BTreeMap::<String, Vec<PreparedCandidate>>::new();
    for candidate in candidates {
        let sha = candidate
            .file
            .sha256
            .clone()
            .expect("sha256 must be present after hashing");
        by_sha.entry(sha).or_default().push(candidate);
    }

    if by_sha.len() == 1 {
        let mut group = by_sha.into_values().next().unwrap_or_default();
        return Ok(CandidateResolution::Ready(group.remove(0)));
    }

    Ok(CandidateResolution::Conflict(
        by_sha.into_values().flatten().collect(),
    ))
}
