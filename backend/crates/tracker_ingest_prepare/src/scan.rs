use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracker_parser_core::{SourceKind, quick_detect_source_kind, quick_extract_gg_tournament_id};

use crate::{
    PreparedFileRef, PreparedSourceKind, RejectReasonCode, RejectedTournament,
    archive::{ArchiveScanEntry, hash_archive_member, list_archive_members},
};

#[derive(Debug, Clone)]
pub(crate) enum ScanLocation {
    File {
        path: PathBuf,
    },
    ArchiveMember {
        archive_path: PathBuf,
        member_path: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedCandidate {
    pub file: PreparedFileRef,
    pub location: ScanLocation,
}

#[derive(Debug)]
pub(crate) struct ScanOutcome {
    pub scanned_files: usize,
    pub entries: Vec<PreparedScanEntry>,
}

#[derive(Debug, Clone)]
pub(crate) enum HeaderProbe {
    FirstNonEmptyLine(String),
    Empty,
    InvalidUtf8,
    ContainsNul,
}

#[derive(Debug)]
pub(crate) enum PreparedScanEntry {
    Candidate(PreparedCandidate),
    Rejected(RejectedTournament),
}

pub(crate) fn scan_path(path: &Path) -> Result<ScanOutcome> {
    let mut file_paths = Vec::new();
    collect_input_paths(path, &mut file_paths)?;
    file_paths.sort();

    let mut scanned_files = 0usize;
    let mut entries = Vec::new();

    for file_path in file_paths {
        if is_zip_path(&file_path) {
            let source_path = file_path.display().to_string();
            match list_archive_members(&file_path) {
                Ok(archive_members) => {
                    for member in archive_members {
                        scanned_files += 1;
                        match member {
                            ArchiveScanEntry::Candidate(member) => entries.push(classify_line(
                                source_path.clone(),
                                Some(member.member_path.clone()),
                                member.byte_size,
                                member.header_probe,
                                ScanLocation::ArchiveMember {
                                    archive_path: file_path.clone(),
                                    member_path: member.member_path,
                                },
                            )),
                            ArchiveScanEntry::Rejected(member) => {
                                entries.push(reject_unsupported_source(
                                    source_path.clone(),
                                    Some(member.member_path),
                                    member.byte_size,
                                    member.reason_text,
                                ));
                            }
                        }
                    }
                }
                Err(error) => {
                    let metadata = fs::metadata(&file_path).with_context(|| {
                        format!("failed to read metadata for `{}`", file_path.display())
                    })?;
                    scanned_files += 1;
                    entries.push(reject_unsupported_source(
                        source_path,
                        None,
                        metadata.len() as i64,
                        format!("Failed to read ZIP archive: {error:#}"),
                    ));
                }
            }
            continue;
        }

        scanned_files += 1;
        let metadata = fs::metadata(&file_path)
            .with_context(|| format!("failed to read metadata for `{}`", file_path.display()))?;
        let first_non_empty_line = read_first_non_empty_line_from_file(&file_path)?;
        entries.push(classify_line(
            file_path.display().to_string(),
            None,
            metadata.len() as i64,
            first_non_empty_line,
            ScanLocation::File {
                path: file_path.clone(),
            },
        ));
    }

    Ok(ScanOutcome {
        scanned_files,
        entries,
    })
}

pub(crate) fn ensure_sha256(candidate: &mut PreparedCandidate) -> Result<()> {
    if candidate.file.sha256.is_some() {
        return Ok(());
    }

    let sha256 = match &candidate.location {
        ScanLocation::File { path } => sha256_from_file(path)?,
        ScanLocation::ArchiveMember {
            archive_path,
            member_path,
        } => hash_archive_member(archive_path, member_path)?,
    };
    candidate.file.sha256 = Some(sha256);
    Ok(())
}

pub(crate) fn sha256_from_reader<R: Read>(mut reader: R) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256_from_file(path: &Path) -> Result<String> {
    let file =
        File::open(path).with_context(|| format!("failed to open file `{}`", path.display()))?;
    sha256_from_reader(BufReader::new(file))
}

fn classify_line(
    source_path: String,
    member_path: Option<String>,
    byte_size: i64,
    header_probe: HeaderProbe,
    location: ScanLocation,
) -> PreparedScanEntry {
    let first_line = match header_probe {
        HeaderProbe::FirstNonEmptyLine(first_line) => first_line,
        HeaderProbe::Empty => {
            return reject_unsupported_source(
                source_path,
                member_path,
                byte_size,
                "Source has no non-empty header line".to_string(),
            );
        }
        HeaderProbe::InvalidUtf8 => {
            return reject_unsupported_source(
                source_path,
                member_path,
                byte_size,
                "Source header is not valid UTF-8".to_string(),
            );
        }
        HeaderProbe::ContainsNul => {
            return reject_unsupported_source(
                source_path,
                member_path,
                byte_size,
                "Source header contains embedded NUL bytes".to_string(),
            );
        }
    };

    let source_kind = match quick_detect_source_kind(&first_line) {
        Ok(kind) => map_source_kind(kind),
        Err(_) => {
            return reject_unsupported_source(
                source_path,
                member_path,
                byte_size,
                format!("Unsupported source header `{first_line}`"),
            );
        }
    };

    let tournament_id = quick_extract_gg_tournament_id(&first_line)
        .ok()
        .flatten()
        .map(|value| value.to_string());

    let file = PreparedFileRef {
        source_path,
        member_path,
        source_kind,
        tournament_id: tournament_id.clone(),
        byte_size,
        sha256: None,
    };

    if tournament_id.is_none() {
        return PreparedScanEntry::Rejected(RejectedTournament {
            tournament_id: None,
            files: vec![file],
            reason_code: RejectReasonCode::MissingTournamentId,
            reason_text: "Could not extract GG tournament_id from source header".to_string(),
        });
    }

    PreparedScanEntry::Candidate(PreparedCandidate { file, location })
}

fn map_source_kind(kind: SourceKind) -> PreparedSourceKind {
    match kind {
        SourceKind::HandHistory => PreparedSourceKind::HandHistory,
        SourceKind::TournamentSummary => PreparedSourceKind::TournamentSummary,
    }
}

fn collect_input_paths(path: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)
            .with_context(|| format!("failed to read directory `{}`", path.display()))?
        {
            let entry = entry?;
            collect_input_paths(&entry.path(), output)?;
        }
        return Ok(());
    }

    if path.is_file() {
        output.push(path.to_path_buf());
    }

    Ok(())
}

pub(crate) fn read_first_non_empty_line_from_reader<R: Read>(reader: R) -> Result<HeaderProbe> {
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();

    loop {
        line.clear();
        let bytes = reader.read_until(b'\n', &mut line)?;
        if bytes == 0 {
            return Ok(HeaderProbe::Empty);
        }
        let trimmed = match std::str::from_utf8(&line) {
            Ok(value) => value.trim(),
            Err(_) => return Ok(HeaderProbe::InvalidUtf8),
        };
        if trimmed.contains('\0') {
            return Ok(HeaderProbe::ContainsNul);
        }
        if !trimmed.is_empty() {
            return Ok(HeaderProbe::FirstNonEmptyLine(trimmed.to_string()));
        }
    }
}

fn read_first_non_empty_line_from_file(path: &Path) -> Result<HeaderProbe> {
    let file =
        File::open(path).with_context(|| format!("failed to open file `{}`", path.display()))?;
    read_first_non_empty_line_from_reader(file)
}

fn reject_unsupported_source(
    source_path: String,
    member_path: Option<String>,
    byte_size: i64,
    reason_text: String,
) -> PreparedScanEntry {
    PreparedScanEntry::Rejected(RejectedTournament {
        tournament_id: None,
        files: vec![PreparedFileRef {
            source_path,
            member_path,
            source_kind: PreparedSourceKind::Unknown,
            tournament_id: None,
            byte_size,
            sha256: None,
        }],
        reason_code: RejectReasonCode::UnsupportedSource,
        reason_text,
    })
}

fn is_zip_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}
