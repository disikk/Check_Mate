use std::{fs, io::Write, path::Path};

use tempfile::tempdir;
use tracker_ingest_prepare::{PreparedSourceKind, RejectReasonCode, prepare_path};
use zip::write::SimpleFileOptions;

const HH_FT: &str =
    include_str!("../../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");
const TS_WINNER: &str = include_str!(
    "../../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
);

#[test]
fn prepares_directory_with_one_valid_pair() {
    let dir = tempdir().unwrap();
    write_text_file(dir.path().join("one.ts.txt"), TS_WINNER);
    write_text_file(dir.path().join("one.hh.txt"), HH_FT);

    let report = prepare_path(dir.path()).unwrap();

    assert_eq!(report.scanned_files, 2);
    assert_eq!(report.paired_tournaments.len(), 1);
    assert!(report.rejected_tournaments.is_empty());

    let pair = &report.paired_tournaments[0];
    assert_eq!(pair.tournament_id, "271770266");
    assert_eq!(pair.ts.source_kind, PreparedSourceKind::TournamentSummary);
    assert_eq!(pair.hh.source_kind, PreparedSourceKind::HandHistory);
    assert_eq!(pair.ts.tournament_id.as_deref(), Some("271770266"));
    assert_eq!(pair.hh.tournament_id.as_deref(), Some("271770266"));
    assert!(pair.ts.sha256.is_some());
    assert!(pair.hh.sha256.is_some());
}

#[test]
fn rejects_missing_pair_and_unsupported_files() {
    let dir = tempdir().unwrap();
    write_text_file(dir.path().join("one.ts.txt"), TS_WINNER);
    write_text_file(dir.path().join("notes.txt"), "hello world");

    let report = prepare_path(dir.path()).unwrap();

    assert!(report.paired_tournaments.is_empty());
    assert_eq!(report.rejected_tournaments.len(), 2);
    assert!(
        report
            .rejected_tournaments
            .iter()
            .any(|item| item.reason_code == RejectReasonCode::MissingHh)
    );
    assert!(
        report
            .rejected_tournaments
            .iter()
            .any(|item| item.reason_code == RejectReasonCode::UnsupportedSource)
    );
}

#[test]
fn rejects_conflicting_hand_histories_for_same_tournament() {
    let dir = tempdir().unwrap();
    write_text_file(dir.path().join("one.ts.txt"), TS_WINNER);
    write_text_file(dir.path().join("one.hh.txt"), HH_FT);
    write_text_file(
        dir.path().join("two.hh.txt"),
        &HH_FT.replace("Hero: calls 1,512", "Hero: calls 1,400"),
    );

    let report = prepare_path(dir.path()).unwrap();

    assert!(report.paired_tournaments.is_empty());
    assert_eq!(report.rejected_tournaments.len(), 1);
    assert_eq!(
        report.rejected_tournaments[0].reason_code,
        RejectReasonCode::ConflictingHh
    );
    assert_eq!(
        report.rejected_tournaments[0].tournament_id.as_deref(),
        Some("271770266")
    );
}

#[test]
fn collapses_duplicate_same_content_files_and_keeps_pair_valid() {
    let dir = tempdir().unwrap();
    write_text_file(dir.path().join("one.ts.txt"), TS_WINNER);
    write_text_file(dir.path().join("one.hh.txt"), HH_FT);
    write_text_file(dir.path().join("dup.hh.txt"), HH_FT);

    let report = prepare_path(dir.path()).unwrap();

    assert_eq!(report.paired_tournaments.len(), 1);
    assert!(report.rejected_tournaments.is_empty());
}

#[test]
fn prepares_zip_archive_members_into_pairs() {
    let dir = tempdir().unwrap();
    let archive_path = dir.path().join("sample.zip");
    write_zip(
        &archive_path,
        &[
            ("nested/one.ts.txt", TS_WINNER),
            ("nested/one.hh.txt", HH_FT),
        ],
    );

    let report = prepare_path(&archive_path).unwrap();

    assert_eq!(report.scanned_files, 2);
    assert_eq!(report.paired_tournaments.len(), 1);
    assert!(report.rejected_tournaments.is_empty());
    assert_eq!(
        report.paired_tournaments[0].ts.member_path.as_deref(),
        Some("nested/one.ts.txt")
    );
    assert_eq!(
        report.paired_tournaments[0].hh.member_path.as_deref(),
        Some("nested/one.hh.txt")
    );
}

#[test]
fn rejects_non_utf8_file_as_unsupported_instead_of_failing_scan() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("bad.hh.txt"), [0xff, 0xfe, b'\n']).unwrap();

    let report = prepare_path(dir.path()).unwrap();

    assert_eq!(report.scanned_files, 1);
    assert!(report.paired_tournaments.is_empty());
    assert_eq!(report.rejected_tournaments.len(), 1);
    assert_eq!(
        report.rejected_tournaments[0].reason_code,
        RejectReasonCode::UnsupportedSource
    );
    assert!(report.rejected_tournaments[0].reason_text.contains("UTF-8"));
}

#[test]
fn rejects_non_utf8_zip_member_as_unsupported_instead_of_failing_scan() {
    let dir = tempdir().unwrap();
    let archive_path = dir.path().join("bad.zip");
    write_zip_bytes(
        &archive_path,
        &[("nested/bad.hh.txt", &[0xff, 0xfe, b'\n'])],
    );

    let report = prepare_path(&archive_path).unwrap();

    assert_eq!(report.scanned_files, 1);
    assert!(report.paired_tournaments.is_empty());
    assert_eq!(report.rejected_tournaments.len(), 1);
    assert_eq!(
        report.rejected_tournaments[0].reason_code,
        RejectReasonCode::UnsupportedSource
    );
    assert!(report.rejected_tournaments[0].reason_text.contains("UTF-8"));
}

fn write_text_file(path: impl AsRef<Path>, contents: &str) {
    fs::write(path, contents).unwrap();
}

fn write_zip(path: &Path, members: &[(&str, &str)]) {
    let file = fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);

    for (member_path, contents) in members {
        writer
            .start_file((*member_path).to_string(), SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }

    writer.finish().unwrap();
}

fn write_zip_bytes(path: &Path, members: &[(&str, &[u8])]) {
    let file = fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);

    for (member_path, contents) in members {
        writer
            .start_file((*member_path).to_string(), SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents).unwrap();
    }

    writer.finish().unwrap();
}
