use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use axum::http::StatusCode;
use sha2::{Digest, Sha256};
use tokio::fs;
use tracker_ingest_prepare::{
    PrepareReport, PreparedFileRef, PreparedSourceKind, RejectReasonCode, RejectedTournament,
    prepare_path,
};
use tracker_ingest_runtime::{
    FileKind, IngestDiagnosticInput, IngestFileInput, IngestMemberInput,
};
use tracker_parser_core::{SourceKind, detect_source_kind};
use uuid::Uuid;

use crate::errors::ApiError;

/// A single file spooled to disk from a multipart upload.
#[derive(Debug, Clone)]
pub(crate) struct StoredUpload {
    pub(crate) original_filename: String,
    pub(crate) spool_path: PathBuf,
}

/// Receive a single multipart field, validate filename, and write to spool dir.
pub(crate) async fn store_upload_field(
    upload_root: &Path,
    field: axum::extract::multipart::Field<'_>,
) -> Result<StoredUpload, ApiError> {
    let filename = field.file_name().map(ToOwned::to_owned).ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "multipart field is missing filename",
        )
    })?;
    let bytes = field.bytes().await.map_err(ApiError::internal)?;

    if !is_supported_upload_filename(&filename) {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            format!("unsupported upload file `{filename}`"),
        ));
    }

    let spool_path = upload_root.join(format!(
        "{}-{}",
        Uuid::new_v4(),
        sanitize_filename(&filename)
    ));
    fs::write(&spool_path, &bytes)
        .await
        .map_err(ApiError::internal)?;

    Ok(StoredUpload {
        original_filename: filename,
        spool_path,
    })
}

/// Classify stored uploads into ingest file inputs.
pub(crate) async fn build_upload_inputs(
    upload_root: &Path,
    uploads: &[StoredUpload],
) -> Result<Vec<IngestFileInput>, ApiError> {
    if uploads.len() == 1 {
        let upload = &uploads[0];
        let bytes = fs::read(&upload.spool_path)
            .await
            .map_err(ApiError::internal)?;
        return Ok(vec![classify_upload_file(
            &upload.original_filename,
            bytes.as_ref(),
            storage_uri_for_path(&upload.spool_path),
        )?]);
    }

    classify_upload_batch(upload_root, uploads)
}

fn classify_upload_batch(
    upload_root: &Path,
    uploads: &[StoredUpload],
) -> Result<Vec<IngestFileInput>, ApiError> {
    let report = prepare_path(upload_root).map_err(|error| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("failed to prepare upload batch: {error:#}"),
        )
    })?;
    if report.paired_tournaments.is_empty() {
        return Err(empty_pair_batch_error(&report));
    }

    let stored_names_by_path = uploads
        .iter()
        .map(|upload| {
            (
                upload.spool_path.display().to_string(),
                upload.original_filename.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let diagnostics = report
        .rejected_tournaments
        .iter()
        .map(|rejected| build_reject_diagnostic_for_batch(rejected, &stored_names_by_path))
        .collect::<Vec<_>>();

    Ok(vec![build_prepared_batch_archive_input(
        upload_root,
        &stored_names_by_path,
        &report,
        diagnostics,
    )?])
}

fn classify_upload_file(
    filename: &str,
    bytes: &[u8],
    storage_uri: String,
) -> Result<IngestFileInput, ApiError> {
    if filename.to_lowercase().ends_with(".zip") {
        classify_archive_upload(filename, bytes, storage_uri)
    } else {
        classify_flat_upload(filename, bytes, storage_uri)
    }
}

fn classify_flat_upload(
    filename: &str,
    bytes: &[u8],
    storage_uri: String,
) -> Result<IngestFileInput, ApiError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("flat upload `{filename}` must be UTF-8 text"),
        )
    })?;
    let file_kind = match detect_source_kind(text).map_err(|error| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("failed to classify `{filename}`: {error}"),
        )
    })? {
        SourceKind::HandHistory => FileKind::HandHistory,
        SourceKind::TournamentSummary => FileKind::TournamentSummary,
    };

    Ok(IngestFileInput {
        room: "gg".to_string(),
        file_kind,
        sha256: sha256_bytes_hex(bytes),
        original_filename: filename.to_string(),
        byte_size: bytes.len() as i64,
        storage_uri,
        members: vec![],
        diagnostics: vec![],
    })
}

fn classify_archive_upload(
    filename: &str,
    bytes: &[u8],
    storage_uri: String,
) -> Result<IngestFileInput, ApiError> {
    let report = prepare_archive_upload(filename, bytes, &storage_uri)?;
    if report.paired_tournaments.is_empty() {
        return Err(empty_pair_batch_error(&report));
    }

    let mut members = Vec::new();
    for pair in &report.paired_tournaments {
        let ts_member_index = members.len() as i32;
        members.push(prepared_archive_member_input(&pair.ts, None)?);
        members.push(prepared_archive_member_input(
            &pair.hh,
            Some(ts_member_index),
        )?);
    }

    let diagnostics = report
        .rejected_tournaments
        .iter()
        .map(build_reject_diagnostic)
        .collect();

    Ok(IngestFileInput {
        room: "gg".to_string(),
        file_kind: FileKind::Archive,
        sha256: sha256_bytes_hex(bytes),
        original_filename: filename.to_string(),
        byte_size: bytes.len() as i64,
        storage_uri,
        members,
        diagnostics,
    })
}

fn is_supported_upload_filename(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".txt") || lower.ends_with(".hh") || lower.ends_with(".zip")
}

fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '_',
            _ => ch,
        })
        .collect()
}

fn sha256_bytes_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn prepare_archive_upload(
    filename: &str,
    bytes: &[u8],
    storage_uri: &str,
) -> Result<PrepareReport, ApiError> {
    let stored_path = storage_uri
        .strip_prefix("local://")
        .map(PathBuf::from)
        .filter(|path| path.exists());
    let temp_path = stored_path.is_none().then(|| {
        std::env::temp_dir().join(format!(
            "check-mate-upload-{}-{}",
            Uuid::new_v4(),
            sanitize_filename(filename)
        ))
    });
    let archive_path = stored_path
        .clone()
        .or_else(|| temp_path.clone())
        .expect("archive path must exist or be materialized");

    if stored_path.is_none() {
        std::fs::write(&archive_path, bytes).map_err(ApiError::internal)?;
    }

    let report = prepare_path(&archive_path).map_err(|error| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("failed to prepare archive upload `{filename}`: {error:#}"),
        )
    });

    if let Some(temp_path) = temp_path {
        let _ = std::fs::remove_file(temp_path);
    }

    report
}

fn build_prepared_batch_archive_input(
    upload_root: &Path,
    stored_names_by_path: &BTreeMap<String, String>,
    report: &PrepareReport,
    diagnostics: Vec<IngestDiagnosticInput>,
) -> Result<IngestFileInput, ApiError> {
    let archive_path = upload_root.join("prepared-pairs.zip");
    let file = std::fs::File::create(&archive_path).map_err(ApiError::internal)?;
    let mut writer = zip::ZipWriter::new(file);
    let mut members = Vec::new();

    for (pair_index, pair) in report.paired_tournaments.iter().enumerate() {
        let ts_member_index = members.len() as i32;
        let (ts_member, ts_bytes) =
            build_prepared_batch_member(pair_index, "ts", &pair.ts, stored_names_by_path, None)?;
        writer
            .start_file(
                ts_member.member_path.clone(),
                zip::write::SimpleFileOptions::default(),
            )
            .map_err(ApiError::internal)?;
        std::io::Write::write_all(&mut writer, &ts_bytes).map_err(ApiError::internal)?;
        members.push(ts_member);

        let (hh_member, hh_bytes) = build_prepared_batch_member(
            pair_index,
            "hh",
            &pair.hh,
            stored_names_by_path,
            Some(ts_member_index),
        )?;
        writer
            .start_file(
                hh_member.member_path.clone(),
                zip::write::SimpleFileOptions::default(),
            )
            .map_err(ApiError::internal)?;
        std::io::Write::write_all(&mut writer, &hh_bytes).map_err(ApiError::internal)?;
        members.push(hh_member);
    }

    writer.finish().map_err(ApiError::internal)?;
    let archive_bytes = std::fs::read(&archive_path).map_err(ApiError::internal)?;

    Ok(IngestFileInput {
        room: "gg".to_string(),
        file_kind: FileKind::Archive,
        sha256: sha256_bytes_hex(&archive_bytes),
        original_filename: "prepared-pairs.zip".to_string(),
        byte_size: archive_bytes.len() as i64,
        storage_uri: storage_uri_for_path(&archive_path),
        members,
        diagnostics,
    })
}

fn build_prepared_batch_member(
    pair_index: usize,
    role: &str,
    file: &PreparedFileRef,
    stored_names_by_path: &BTreeMap<String, String>,
    depends_on_member_index: Option<i32>,
) -> Result<(IngestMemberInput, Vec<u8>), ApiError> {
    let sha256 = file
        .sha256
        .clone()
        .ok_or_else(|| ApiError::internal("prepared upload file is missing sha256"))?;
    let member_path = format!(
        "pair-{pair_index:04}-{role}-{}",
        sanitize_filename(&display_prepared_file_name(file, stored_names_by_path))
    );
    let bytes = read_prepared_file_bytes(file)?;

    Ok((
        IngestMemberInput {
            member_path,
            member_kind: map_prepared_source_kind(file.source_kind),
            sha256,
            byte_size: bytes.len() as i64,
            depends_on_member_index,
        },
        bytes,
    ))
}

fn display_prepared_file_name(
    file: &PreparedFileRef,
    stored_names_by_path: &BTreeMap<String, String>,
) -> String {
    file.member_path.clone().unwrap_or_else(|| {
        stored_names_by_path
            .get(&file.source_path)
            .cloned()
            .unwrap_or_else(|| {
                Path::new(&file.source_path)
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_else(|| file.source_path.clone())
            })
    })
}

fn read_prepared_file_bytes(file: &PreparedFileRef) -> Result<Vec<u8>, ApiError> {
    match &file.member_path {
        Some(member_path) => read_archive_member_bytes(Path::new(&file.source_path), member_path),
        None => std::fs::read(&file.source_path).map_err(ApiError::internal),
    }
}

fn read_archive_member_bytes(archive_path: &Path, member_path: &str) -> Result<Vec<u8>, ApiError> {
    let file = std::fs::File::open(archive_path).map_err(ApiError::internal)?;
    let mut archive = zip::ZipArchive::new(file).map_err(ApiError::internal)?;
    let mut member = archive.by_name(member_path).map_err(ApiError::internal)?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut member, &mut bytes).map_err(ApiError::internal)?;
    Ok(bytes)
}

fn prepared_archive_member_input(
    file: &PreparedFileRef,
    depends_on_member_index: Option<i32>,
) -> Result<IngestMemberInput, ApiError> {
    let member_path = file
        .member_path
        .clone()
        .ok_or_else(|| ApiError::internal("prepared archive member is missing member_path"))?;
    let sha256 = file
        .sha256
        .clone()
        .ok_or_else(|| ApiError::internal("prepared archive member is missing sha256"))?;

    Ok(IngestMemberInput {
        member_path,
        member_kind: map_prepared_source_kind(file.source_kind),
        sha256,
        byte_size: file.byte_size,
        depends_on_member_index,
    })
}

fn build_reject_diagnostic(rejected: &RejectedTournament) -> IngestDiagnosticInput {
    build_reject_diagnostic_with_names(rejected, None)
}

fn build_reject_diagnostic_for_batch(
    rejected: &RejectedTournament,
    stored_names_by_path: &BTreeMap<String, String>,
) -> IngestDiagnosticInput {
    build_reject_diagnostic_with_names(rejected, Some(stored_names_by_path))
}

fn build_reject_diagnostic_with_names(
    rejected: &RejectedTournament,
    stored_names_by_path: Option<&BTreeMap<String, String>>,
) -> IngestDiagnosticInput {
    let target = rejected
        .files
        .first()
        .and_then(|file| rejected_file_display_path(file, stored_names_by_path))
        .or_else(|| rejected.tournament_id.clone());
    let message = match &rejected.tournament_id {
        Some(tournament_id) => format!(
            "Rejected tournament `{tournament_id}`: {}",
            rejected.reason_text
        ),
        None => format!("Rejected upload source: {}", rejected.reason_text),
    };

    IngestDiagnosticInput {
        code: reject_reason_code_as_str(rejected.reason_code).to_string(),
        message,
        member_path: target,
    }
}

fn rejected_file_display_path(
    file: &PreparedFileRef,
    stored_names_by_path: Option<&BTreeMap<String, String>>,
) -> Option<String> {
    file.member_path.clone().or_else(|| {
        stored_names_by_path
            .and_then(|map| map.get(&file.source_path).cloned())
            .or_else(|| {
                Path::new(&file.source_path)
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
            })
    })
}

fn empty_pair_batch_error(report: &PrepareReport) -> ApiError {
    let mut counts = std::collections::BTreeMap::<&'static str, usize>::new();
    for rejected in &report.rejected_tournaments {
        *counts
            .entry(reject_reason_code_as_str(rejected.reason_code))
            .or_default() += 1;
    }
    let summary = counts
        .into_iter()
        .map(|(code, count)| format!("{code}={count}"))
        .collect::<Vec<_>>()
        .join(", ");

    let suffix = if summary.is_empty() {
        String::new()
    } else {
        format!("; rejected: {summary}")
    };

    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("upload batch contains no valid HH+TS pairs{suffix}"),
    )
}

fn map_prepared_source_kind(kind: PreparedSourceKind) -> FileKind {
    match kind {
        PreparedSourceKind::HandHistory => FileKind::HandHistory,
        PreparedSourceKind::TournamentSummary => FileKind::TournamentSummary,
        PreparedSourceKind::Unknown => unreachable!("unknown prepared source kind cannot enqueue"),
    }
}

fn reject_reason_code_as_str(code: RejectReasonCode) -> &'static str {
    match code {
        RejectReasonCode::MissingTs => "missing_ts",
        RejectReasonCode::MissingHh => "missing_hh",
        RejectReasonCode::ConflictingTs => "conflicting_ts",
        RejectReasonCode::ConflictingHh => "conflicting_hh",
        RejectReasonCode::UnsupportedSource => "unsupported_source",
        RejectReasonCode::MissingTournamentId => "missing_tournament_id",
        RejectReasonCode::DuplicateSameContent => "duplicate_same_content",
    }
}

fn storage_uri_for_path(path: &Path) -> String {
    format!("local://{}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Cursor, Write},
        path::Path,
    };
    use tempfile::tempdir;
    use tracker_ingest_runtime::FileKind;
    use zip::write::SimpleFileOptions;

    const HH_FT: &str =
        include_str!("../../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");
    const TS_WINNER: &str = include_str!(
        "../../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
    );
    const TS_ORPHAN: &str = include_str!(
        "../../../fixtures/mbr/ts/GG20260316 - Tournament #271769772 - Mystery Battle Royale 25.txt"
    );

    #[test]
    fn classify_upload_file_keeps_single_flat_ts_upload_supported() {
        let input = classify_upload_file(
            "single-ts.txt",
            TS_WINNER.as_bytes(),
            "local:///tmp/single-ts.txt".to_string(),
        )
        .expect("single flat TS upload should stay supported");

        assert_eq!(input.file_kind, FileKind::TournamentSummary);
        assert!(input.members.is_empty());
        assert!(input.diagnostics.is_empty());
    }

    #[test]
    fn classify_upload_file_rejects_zip_without_valid_hh_ts_pair() {
        let zip_bytes = build_zip_bytes(&[
            ("nested/summary.txt", TS_WINNER),
            ("notes/readme.md", "unsupported"),
        ]);

        let error = classify_upload_file(
            "bundle.zip",
            &zip_bytes,
            "local:///tmp/bundle.zip".to_string(),
        )
        .expect_err("ZIP without a valid HH+TS pair must be rejected before enqueue");

        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(error.message.contains("valid HH+TS pair"));
    }

    #[test]
    fn classify_upload_file_orders_zip_pair_as_ts_then_hh() {
        let zip_bytes = build_zip_bytes(&[
            ("nested/history.txt", HH_FT),
            ("nested/summary.txt", TS_WINNER),
        ]);

        let input = classify_upload_file(
            "bundle.zip",
            &zip_bytes,
            "local:///tmp/bundle.zip".to_string(),
        )
        .expect("ZIP with one valid pair should classify");

        assert_eq!(input.file_kind, FileKind::Archive);
        assert_eq!(input.members.len(), 2);
        assert_eq!(input.members[0].member_kind, FileKind::TournamentSummary);
        assert_eq!(input.members[0].member_path, "nested/summary.txt");
        assert_eq!(input.members[0].depends_on_member_index, None);
        assert_eq!(input.members[1].member_kind, FileKind::HandHistory);
        assert_eq!(input.members[1].member_path, "nested/history.txt");
        assert_eq!(input.members[1].depends_on_member_index, Some(0));
    }

    #[test]
    fn classify_upload_file_orders_nested_zip_pair_as_ts_then_hh() {
        let inner_zip_bytes =
            build_zip_bytes(&[("deep/history.txt", HH_FT), ("deep/summary.txt", TS_WINNER)]);
        let outer_zip_bytes = build_zip_bytes_bytes(&[("nested/inner.zip", &inner_zip_bytes)]);

        let input = classify_upload_file(
            "bundle.zip",
            &outer_zip_bytes,
            "local:///tmp/bundle.zip".to_string(),
        )
        .expect("nested ZIP with one valid pair should classify");

        assert_eq!(input.file_kind, FileKind::Archive);
        assert_eq!(input.members.len(), 2);
        assert_eq!(input.members[0].member_kind, FileKind::TournamentSummary);
        assert_eq!(
            input.members[0].member_path,
            "nested/inner.zip!/deep/summary.txt"
        );
        assert_eq!(input.members[0].depends_on_member_index, None);
        assert_eq!(input.members[1].member_kind, FileKind::HandHistory);
        assert_eq!(
            input.members[1].member_path,
            "nested/inner.zip!/deep/history.txt"
        );
        assert_eq!(input.members[1].depends_on_member_index, Some(0));
    }

    #[test]
    fn classify_upload_batch_pairs_multi_file_uploads_and_logs_orphans() {
        let dir = tempdir().unwrap();
        let winner_ts = write_text_upload(dir.path(), "winner.ts.txt", TS_WINNER);
        let winner_hh = write_text_upload(dir.path(), "winner.hh.txt", HH_FT);
        let orphan_ts = write_text_upload(dir.path(), "orphan.ts.txt", TS_ORPHAN);

        let inputs = classify_upload_batch(
            dir.path(),
            &[
                StoredUpload {
                    original_filename: "winner.ts.txt".to_string(),
                    spool_path: winner_ts,
                },
                StoredUpload {
                    original_filename: "winner.hh.txt".to_string(),
                    spool_path: winner_hh,
                },
                StoredUpload {
                    original_filename: "orphan.ts.txt".to_string(),
                    spool_path: orphan_ts,
                },
            ],
        )
        .expect("multi-file batch should enqueue only the valid pair");

        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].file_kind, FileKind::Archive);
        assert_eq!(inputs[0].members.len(), 2);
        assert_eq!(
            inputs[0].members[0].member_kind,
            FileKind::TournamentSummary
        );
        assert_eq!(inputs[0].members[0].depends_on_member_index, None);
        assert_eq!(inputs[0].members[1].member_kind, FileKind::HandHistory);
        assert_eq!(inputs[0].members[1].depends_on_member_index, Some(0));
        assert_eq!(inputs[0].diagnostics.len(), 1);
        assert_eq!(inputs[0].diagnostics[0].code, "missing_hh");
        assert_eq!(
            inputs[0].diagnostics[0].member_path.as_deref(),
            Some("orphan.ts.txt")
        );
    }

    fn build_zip_bytes(members: &[(&str, &str)]) -> Vec<u8> {
        build_zip_bytes_bytes(
            &members
                .iter()
                .map(|(member_path, contents)| (*member_path, contents.as_bytes()))
                .collect::<Vec<_>>(),
        )
    }

    fn build_zip_bytes_bytes(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            for (member_path, contents) in members {
                writer
                    .start_file((*member_path).to_string(), SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(contents).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn write_text_upload(root: &Path, filename: &str, contents: &str) -> PathBuf {
        let path = root.join(filename);
        std::fs::write(&path, contents).unwrap();
        path
    }
}
