use std::path::Path;

use anyhow::{Result, anyhow};
use sha2::{Digest, Sha256};
use tracker_ingest_prepare::{PreparedFileRef, PreparedSourceKind, RejectReasonCode, RejectedTournament};
use tracker_ingest_runtime::{
    FileKind as IngestFileKind, IngestDiagnosticInput, IngestFileInput, IngestMemberInput,
};
use tracker_parser_core::{
    SourceKind, detect_source_kind,
    models::{ActionType, CertaintyState, ParseIssue, ParseIssueCode, ParseIssuePayload, Street},
};

use super::archive::ArchiveReaderCache;
use super::row_models::ParseIssueRow;

pub(crate) fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn sha256_bytes_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(crate) fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '_',
            _ => ch,
        })
        .collect()
}

pub(crate) fn cents_to_f64(cents: i64) -> f64 {
    (cents as f64) / 100.0
}

pub(crate) fn source_filename(path: &str) -> Result<String> {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("failed to derive filename from `{path}`"))
}

pub(crate) fn street_code(street: Street) -> &'static str {
    match street {
        Street::Preflop => "preflop",
        Street::Flop => "flop",
        Street::Turn => "turn",
        Street::River => "river",
        Street::Showdown => "showdown",
        Street::Summary => "summary",
    }
}

pub(crate) fn action_code(action_type: ActionType) -> &'static str {
    match action_type {
        ActionType::PostAnte => "post_ante",
        ActionType::PostSb => "post_sb",
        ActionType::PostBb => "post_bb",
        ActionType::PostDead => "post_dead",
        ActionType::Fold => "fold",
        ActionType::Check => "check",
        ActionType::Call => "call",
        ActionType::Bet => "bet",
        ActionType::RaiseTo => "raise_to",
        ActionType::ReturnUncalled => "return_uncalled",
        ActionType::Collect => "collect",
        ActionType::Show => "show",
        ActionType::Muck => "muck",
    }
}

pub(crate) fn certainty_state_code(state: CertaintyState) -> &'static str {
    match state {
        CertaintyState::Exact => "exact",
        CertaintyState::Estimated => "estimated",
        CertaintyState::Uncertain => "uncertain",
        CertaintyState::Inconsistent => "inconsistent",
    }
}

pub(crate) fn gg_timestamp_provenance(timezone_name: Option<&str>) -> &'static str {
    if timezone_name.is_some() {
        super::GG_TIMESTAMP_PROVENANCE_PRESENT
    } else {
        super::GG_TIMESTAMP_PROVENANCE_MISSING
    }
}

pub(crate) fn map_prepared_source_kind(kind: PreparedSourceKind) -> Result<IngestFileKind> {
    match kind {
        PreparedSourceKind::HandHistory => Ok(IngestFileKind::HandHistory),
        PreparedSourceKind::TournamentSummary => Ok(IngestFileKind::TournamentSummary),
        PreparedSourceKind::Unknown => Err(anyhow!("unknown prepared source kind cannot enqueue")),
    }
}

pub(crate) fn reject_reason_code_as_str(code: RejectReasonCode) -> &'static str {
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

pub(crate) fn summarize_rejected_by_reason(report: &tracker_ingest_prepare::PrepareReport) -> std::collections::BTreeMap<String, usize> {
    let mut summary = std::collections::BTreeMap::new();
    for rejected in &report.rejected_tournaments {
        *summary
            .entry(reject_reason_code_as_str(rejected.reason_code).to_string())
            .or_insert(0) += 1;
    }
    summary
}

pub(crate) fn format_fraction_value(value: f64) -> String {
    format!("{value:.6}")
}

pub(crate) fn exact_hero_boundary_ko_share(
    hand: &tracker_parser_core::models::CanonicalParsedHand,
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Option<f64> {
    let hero_name = hand.hero_name.as_deref()?;

    normalized_hand
        .eliminations
        .iter()
        .filter(|elimination| elimination.ko_certainty_state == CertaintyState::Exact)
        .filter_map(|elimination| hero_ko_share_fraction(elimination, hero_name))
        .reduce(|accumulator, share| accumulator + share)
}

pub(crate) fn hero_ko_share_fraction(
    elimination: &tracker_parser_core::models::HandElimination,
    hero_name: &str,
) -> Option<f64> {
    elimination
        .ko_share_fraction_by_winner
        .iter()
        .find(|share| share.player_name == hero_name)
        .map(|share| share.share_fraction)
}

pub(crate) fn build_ingest_file_input(path: &str, input: &str) -> Result<IngestFileInput> {
    let file_kind = match detect_source_kind(input)? {
        SourceKind::TournamentSummary => IngestFileKind::TournamentSummary,
        SourceKind::HandHistory => IngestFileKind::HandHistory,
    };

    Ok(IngestFileInput {
        room: "gg".to_string(),
        file_kind,
        sha256: sha256_hex(input),
        original_filename: source_filename(path)?,
        byte_size: input.len() as i64,
        storage_uri: format!("local://{}", path.replace('\\', "/")),
        members: vec![],
        diagnostics: vec![],
    })
}

pub(crate) fn build_prepared_archive_input(
    output_root: &Path,
    report: &tracker_ingest_prepare::PrepareReport,
) -> Result<Option<super::row_models::MaterializedPreparedArchive>> {
    if report.paired_tournaments.is_empty() {
        return Ok(None);
    }

    use anyhow::Context;
    use std::fs;

    fs::create_dir_all(output_root).with_context(|| {
        format!(
            "failed to create prepared archive dir `{}`",
            output_root.display()
        )
    })?;
    let archive_path = output_root.join("prepared-pairs.zip");
    let file = fs::File::create(&archive_path).with_context(|| {
        format!(
            "failed to create prepared archive `{}`",
            archive_path.display()
        )
    })?;
    let mut writer = zip::ZipWriter::new(file);
    let mut members = Vec::new();
    let mut archive_reader_cache = ArchiveReaderCache::default();

    for (pair_index, pair) in report.paired_tournaments.iter().enumerate() {
        let ts_member_index = members.len() as i32;
        let (ts_member, ts_bytes) = build_prepared_archive_member(
            pair_index,
            "ts",
            &pair.ts,
            None,
            &mut archive_reader_cache,
        )?;
        writer
            .start_file(
                ts_member.member_path.clone(),
                zip::write::SimpleFileOptions::default(),
            )
            .with_context(|| {
                format!("failed to start archive member `{}`", ts_member.member_path)
            })?;
        std::io::Write::write_all(&mut writer, &ts_bytes).with_context(|| {
            format!("failed to write archive member `{}`", ts_member.member_path)
        })?;
        members.push(ts_member);

        let (hh_member, hh_bytes) = build_prepared_archive_member(
            pair_index,
            "hh",
            &pair.hh,
            Some(ts_member_index),
            &mut archive_reader_cache,
        )?;
        writer
            .start_file(
                hh_member.member_path.clone(),
                zip::write::SimpleFileOptions::default(),
            )
            .with_context(|| {
                format!("failed to start archive member `{}`", hh_member.member_path)
            })?;
        std::io::Write::write_all(&mut writer, &hh_bytes).with_context(|| {
            format!("failed to write archive member `{}`", hh_member.member_path)
        })?;
        members.push(hh_member);
    }

    writer.finish().with_context(|| {
        format!(
            "failed to finalize prepared archive `{}`",
            archive_path.display()
        )
    })?;
    let archive_bytes = fs::read(&archive_path).with_context(|| {
        format!(
            "failed to read prepared archive `{}`",
            archive_path.display()
        )
    })?;

    Ok(Some(super::row_models::MaterializedPreparedArchive {
        archive_path: archive_path.clone(),
        ingest_file: IngestFileInput {
            room: "gg".to_string(),
            file_kind: IngestFileKind::Archive,
            sha256: sha256_bytes_hex(&archive_bytes),
            original_filename: archive_path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "prepared-pairs.zip".to_string()),
            byte_size: archive_bytes.len() as i64,
            storage_uri: format!("local://{}", archive_path.display()),
            members,
            diagnostics: report
                .rejected_tournaments
                .iter()
                .map(build_reject_diagnostic)
                .collect(),
        },
    }))
}

pub(crate) fn build_prepared_archive_member(
    pair_index: usize,
    role: &str,
    file: &PreparedFileRef,
    depends_on_member_index: Option<i32>,
    archive_reader_cache: &mut ArchiveReaderCache,
) -> Result<(IngestMemberInput, Vec<u8>)> {
    let sha256 = file
        .sha256
        .clone()
        .ok_or_else(|| anyhow!("prepared file is missing sha256"))?;
    let member_path = format!(
        "pair-{pair_index:04}-{role}-{}",
        sanitize_filename(&prepared_file_display_path(file))
    );
    let bytes = read_prepared_file_bytes(file, archive_reader_cache)?;

    Ok((
        IngestMemberInput {
            member_path,
            member_kind: map_prepared_source_kind(file.source_kind)?,
            sha256,
            byte_size: bytes.len() as i64,
            depends_on_member_index,
        },
        bytes,
    ))
}

pub(crate) fn read_prepared_file_bytes(
    file: &PreparedFileRef,
    archive_reader_cache: &mut ArchiveReaderCache,
) -> Result<Vec<u8>> {
    use anyhow::Context;

    match &file.member_path {
        Some(member_path) => {
            archive_reader_cache.read_member_bytes(Path::new(&file.source_path), member_path)
        }
        None => std::fs::read(&file.source_path)
            .with_context(|| format!("failed to read prepared source `{}`", file.source_path)),
    }
}

pub(crate) fn build_reject_diagnostic(rejected: &RejectedTournament) -> IngestDiagnosticInput {
    let target = rejected
        .files
        .first()
        .and_then(rejected_file_display_path)
        .or_else(|| rejected.tournament_id.clone());
    let message = match &rejected.tournament_id {
        Some(tournament_id) => {
            format!(
                "Rejected tournament `{tournament_id}`: {}",
                rejected.reason_text
            )
        }
        None => format!("Rejected upload source: {}", rejected.reason_text),
    };

    IngestDiagnosticInput {
        code: reject_reason_code_as_str(rejected.reason_code).to_string(),
        message,
        member_path: target,
    }
}

pub(crate) fn rejected_file_display_path(file: &PreparedFileRef) -> Option<String> {
    file.member_path.clone().or_else(|| {
        Path::new(&file.source_path)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
    })
}

pub(crate) fn prepared_file_display_path(file: &PreparedFileRef) -> String {
    rejected_file_display_path(file).unwrap_or_else(|| file.source_path.clone())
}

pub(crate) fn tournament_summary_parse_issues(
    summary: &tracker_parser_core::models::TournamentSummary,
) -> Vec<ParseIssueRow> {
    summary.parse_issues.iter().map(parse_issue_row).collect()
}

pub(crate) fn parse_issue_row(issue: &ParseIssue) -> ParseIssueRow {
    ParseIssueRow {
        severity: issue.severity.as_str().to_string(),
        code: issue.code.as_str().to_string(),
        message: issue.message.clone(),
        raw_line: issue.raw_line.clone(),
        payload: issue_payload_json(issue),
    }
}

pub(crate) fn error_issue_row(
    code: ParseIssueCode,
    message: String,
    raw_line: Option<String>,
    payload: Option<ParseIssuePayload>,
) -> ParseIssueRow {
    parse_issue_row(&ParseIssue::error(code, message, raw_line, payload))
}

pub(crate) fn issue_payload_json(issue: &ParseIssue) -> serde_json::Value {
    issue
        .payload
        .as_ref()
        .map(|payload| serde_json::to_value(payload).expect("parse issue payload must serialize"))
        .unwrap_or_else(|| serde_json::json!({}))
}
