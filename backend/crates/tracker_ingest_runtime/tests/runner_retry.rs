use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use postgres::{Client, NoTls};
use tracker_ingest_runtime::{
    BundleStatus, FailureDisposition, FileKind, IngestBundleInput, IngestFileInput,
    IngestMemberInput, claim_next_job, enqueue_bundle, load_bundle_summary, mark_job_failed,
    retry_failed_job,
};
use uuid::Uuid;

fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("backend root must exist")
        .join("migrations")
}

fn apply_all_migrations(client: &mut Client) {
    let mut paths = fs::read_dir(migrations_dir())
        .expect("migrations dir must exist")
        .map(|entry| entry.expect("entry must load").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("sql"))
        .collect::<Vec<_>>();
    paths.sort();

    for path in paths {
        let sql = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read migration {}: {error}", path.display()));
        client
            .batch_execute(&sql)
            .unwrap_or_else(|error| panic!("failed to apply {}: {error}", path.display()));
    }
}

fn db_url() -> String {
    std::env::var("CHECK_MATE_DATABASE_URL")
        .expect("CHECK_MATE_DATABASE_URL must exist for ingest runtime DB tests")
}

fn db_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn seed_actor_shell(client: &mut impl postgres::GenericClient) -> (Uuid, Uuid, Uuid) {
    let organization_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let player_profile_id = Uuid::new_v4();

    client
        .execute(
            "INSERT INTO org.organizations (id, name) VALUES ($1, $2)",
            &[&organization_id, &format!("org-{organization_id}")],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO auth.users (id, email, auth_provider, status) VALUES ($1, $2, 'seed', 'active')",
            &[&user_id, &format!("{user_id}@example.com")],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO core.player_profiles (id, organization_id, owner_user_id, room, network, screen_name)
             VALUES ($1, $2, $3, 'gg', 'gg', $4)",
            &[&player_profile_id, &organization_id, &user_id, &format!("Hero-{player_profile_id}")],
        )
        .unwrap();

    (organization_id, user_id, player_profile_id)
}

fn reset_ingest_runtime_tables(client: &mut Client) {
    client
        .batch_execute(
            "DELETE FROM import.job_attempts;
             DELETE FROM import.import_jobs;
             DELETE FROM import.ingest_bundle_files;
             DELETE FROM import.ingest_bundles;",
        )
        .unwrap();
}

fn sample_bundle_input(
    organization_id: Uuid,
    player_profile_id: Uuid,
    user_id: Uuid,
) -> IngestBundleInput {
    IngestBundleInput {
        organization_id,
        player_profile_id,
        created_by_user_id: user_id,
        files: vec![IngestFileInput {
            room: "gg".to_string(),
            file_kind: FileKind::HandHistory,
            sha256: "d".repeat(64),
            original_filename: "runner-hh.txt".to_string(),
            byte_size: 12,
            storage_uri: "local://runner-hh.txt".to_string(),
            members: vec![],
            diagnostics: vec![],
        }],
    }
}

fn independent_bundle_input(
    organization_id: Uuid,
    player_profile_id: Uuid,
    user_id: Uuid,
) -> IngestBundleInput {
    IngestBundleInput {
        organization_id,
        player_profile_id,
        created_by_user_id: user_id,
        files: vec![
            IngestFileInput {
                room: "gg".to_string(),
                file_kind: FileKind::HandHistory,
                sha256: "1".repeat(64),
                original_filename: "first-hh.txt".to_string(),
                byte_size: 12,
                storage_uri: "local://first-hh.txt".to_string(),
                members: vec![],
                diagnostics: vec![],
            },
            IngestFileInput {
                room: "gg".to_string(),
                file_kind: FileKind::HandHistory,
                sha256: "2".repeat(64),
                original_filename: "second-hh.txt".to_string(),
                byte_size: 12,
                storage_uri: "local://second-hh.txt".to_string(),
                members: vec![],
                diagnostics: vec![],
            },
        ],
    }
}

fn paired_archive_bundle_input(
    organization_id: Uuid,
    player_profile_id: Uuid,
    user_id: Uuid,
) -> IngestBundleInput {
    IngestBundleInput {
        organization_id,
        player_profile_id,
        created_by_user_id: user_id,
        files: vec![IngestFileInput {
            room: "gg".to_string(),
            file_kind: FileKind::Archive,
            sha256: "paired-archive".repeat(5).chars().take(64).collect(),
            original_filename: "paired.zip".to_string(),
            byte_size: 120,
            storage_uri: "local://paired.zip".to_string(),
            members: vec![
                IngestMemberInput {
                    member_path: "pair.ts.txt".to_string(),
                    member_kind: FileKind::TournamentSummary,
                    sha256: "t".repeat(64),
                    byte_size: 10,
                    depends_on_member_index: None,
                },
                IngestMemberInput {
                    member_path: "pair.hh.txt".to_string(),
                    member_kind: FileKind::HandHistory,
                    sha256: "u".repeat(64),
                    byte_size: 10,
                    depends_on_member_index: Some(0),
                },
            ],
            diagnostics: vec![],
        }],
    }
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn claim_next_job_creates_attempt_and_marks_job_running() {
    let _guard = db_test_guard();
    let mut client = Client::connect(&db_url(), NoTls).unwrap();
    apply_all_migrations(&mut client);
    reset_ingest_runtime_tables(&mut client);
    let mut tx = client.transaction().unwrap();
    let (organization_id, user_id, player_profile_id) = seed_actor_shell(&mut tx);
    let bundle = enqueue_bundle(
        &mut tx,
        &sample_bundle_input(organization_id, player_profile_id, user_id),
    )
    .unwrap();

    let claimed = claim_next_job(&mut tx, "test-runner").unwrap().unwrap();

    assert_eq!(claimed.job_id, bundle.file_jobs[0].job_id);
    assert_eq!(claimed.attempt_no, 1);

    let attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)
             FROM import.job_attempts
             WHERE import_job_id = $1",
            &[&claimed.job_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(attempt_count, 1);

    let summary = load_bundle_summary(&mut tx, bundle.bundle_id).unwrap();
    assert_eq!(summary.status, BundleStatus::Running);
    assert_eq!(summary.running_file_jobs, 1);

    tx.rollback().unwrap();
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn second_claim_can_start_while_first_claim_transaction_is_open() {
    let _guard = db_test_guard();
    let database_url = db_url();
    let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
    apply_all_migrations(&mut setup_client);
    reset_ingest_runtime_tables(&mut setup_client);

    let mut setup_tx = setup_client.transaction().unwrap();
    let (organization_id, user_id, player_profile_id) = seed_actor_shell(&mut setup_tx);
    enqueue_bundle(
        &mut setup_tx,
        &independent_bundle_input(organization_id, player_profile_id, user_id),
    )
    .unwrap();
    setup_tx.commit().unwrap();

    let mut first_client = Client::connect(&database_url, NoTls).unwrap();
    let mut second_client = Client::connect(&database_url, NoTls).unwrap();

    let mut first_tx = first_client.transaction().unwrap();
    let first = claim_next_job(&mut first_tx, "runner-one")
        .unwrap()
        .unwrap();
    assert_eq!(first.member_path.as_deref(), Some("first-hh.txt"));

    let mut second_tx = second_client.transaction().unwrap();
    second_tx
        .batch_execute("SET LOCAL statement_timeout = '1000ms'")
        .unwrap();

    let second = claim_next_job(&mut second_tx, "runner-two")
        .expect("second worker should claim another file without waiting on bundle row")
        .expect("second worker should find remaining queued job");
    assert_eq!(second.member_path.as_deref(), Some("second-hh.txt"));

    second_tx.rollback().unwrap();
    first_tx.rollback().unwrap();
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn retriable_failure_can_be_requeued_until_max_attempts() {
    let _guard = db_test_guard();
    let mut client = Client::connect(&db_url(), NoTls).unwrap();
    apply_all_migrations(&mut client);
    reset_ingest_runtime_tables(&mut client);
    let mut tx = client.transaction().unwrap();
    let (organization_id, user_id, player_profile_id) = seed_actor_shell(&mut tx);
    let bundle = enqueue_bundle(
        &mut tx,
        &sample_bundle_input(organization_id, player_profile_id, user_id),
    )
    .unwrap();

    let claimed = claim_next_job(&mut tx, "test-runner").unwrap().unwrap();
    mark_job_failed(
        &mut tx,
        claimed.job_id,
        claimed.attempt_no,
        FailureDisposition::Retriable,
        "transient_db",
    )
    .unwrap();

    let summary_after_fail = load_bundle_summary(&mut tx, bundle.bundle_id).unwrap();
    assert_eq!(summary_after_fail.status, BundleStatus::Running);
    assert_eq!(summary_after_fail.failed_retriable_file_jobs, 1);

    retry_failed_job(&mut tx, claimed.job_id, 3).unwrap();

    let summary_after_retry = load_bundle_summary(&mut tx, bundle.bundle_id).unwrap();
    assert_eq!(summary_after_retry.status, BundleStatus::Queued);
    assert_eq!(summary_after_retry.queued_file_jobs, 1);

    tx.rollback().unwrap();
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn terminal_failure_keeps_bundle_in_partial_success_ready_surface() {
    let _guard = db_test_guard();
    let mut client = Client::connect(&db_url(), NoTls).unwrap();
    apply_all_migrations(&mut client);
    reset_ingest_runtime_tables(&mut client);
    let mut tx = client.transaction().unwrap();
    let (organization_id, user_id, player_profile_id) = seed_actor_shell(&mut tx);

    let bundle = enqueue_bundle(
        &mut tx,
        &IngestBundleInput {
            organization_id,
            player_profile_id,
            created_by_user_id: user_id,
            files: vec![
                IngestFileInput {
                    room: "gg".to_string(),
                    file_kind: FileKind::HandHistory,
                    sha256: "e".repeat(64),
                    original_filename: "one.txt".to_string(),
                    byte_size: 10,
                    storage_uri: "local://one.txt".to_string(),
                    members: vec![],
                    diagnostics: vec![],
                },
                IngestFileInput {
                    room: "gg".to_string(),
                    file_kind: FileKind::TournamentSummary,
                    sha256: "f".repeat(64),
                    original_filename: "two.txt".to_string(),
                    byte_size: 10,
                    storage_uri: "local://two.txt".to_string(),
                    members: vec![],
                    diagnostics: vec![],
                },
            ],
        },
    )
    .unwrap();

    let first = claim_next_job(&mut tx, "test-runner").unwrap().unwrap();
    let second = claim_next_job(&mut tx, "test-runner").unwrap().unwrap();

    tracker_ingest_runtime::mark_job_succeeded(&mut tx, first.job_id, first.attempt_no).unwrap();
    mark_job_failed(
        &mut tx,
        second.job_id,
        second.attempt_no,
        FailureDisposition::Terminal,
        "parse_error",
    )
    .unwrap();

    let summary = load_bundle_summary(&mut tx, bundle.bundle_id).unwrap();
    assert_eq!(summary.status, BundleStatus::PartialSuccess);
    assert_eq!(summary.succeeded_file_jobs, 1);
    assert_eq!(summary.failed_terminal_file_jobs, 1);

    tx.rollback().unwrap();
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn claim_next_job_preserves_bundle_file_order_across_uploaded_bundles() {
    let _guard = db_test_guard();
    let mut client = Client::connect(&db_url(), NoTls).unwrap();
    apply_all_migrations(&mut client);
    reset_ingest_runtime_tables(&mut client);
    let mut tx = client.transaction().unwrap();
    let (organization_id, user_id, player_profile_id) = seed_actor_shell(&mut tx);

    enqueue_bundle(
        &mut tx,
        &IngestBundleInput {
            organization_id,
            player_profile_id,
            created_by_user_id: user_id,
            files: vec![
                IngestFileInput {
                    room: "gg".to_string(),
                    file_kind: FileKind::TournamentSummary,
                    sha256: "g".repeat(64),
                    original_filename: "first-ts.txt".to_string(),
                    byte_size: 10,
                    storage_uri: "local://first-ts.txt".to_string(),
                    members: vec![],
                    diagnostics: vec![],
                },
                IngestFileInput {
                    room: "gg".to_string(),
                    file_kind: FileKind::HandHistory,
                    sha256: "h".repeat(64),
                    original_filename: "first-hh.txt".to_string(),
                    byte_size: 10,
                    storage_uri: "local://first-hh.txt".to_string(),
                    members: vec![],
                    diagnostics: vec![],
                },
            ],
        },
    )
    .unwrap();

    enqueue_bundle(
        &mut tx,
        &IngestBundleInput {
            organization_id,
            player_profile_id,
            created_by_user_id: user_id,
            files: vec![
                IngestFileInput {
                    room: "gg".to_string(),
                    file_kind: FileKind::TournamentSummary,
                    sha256: "i".repeat(64),
                    original_filename: "second-ts.txt".to_string(),
                    byte_size: 10,
                    storage_uri: "local://second-ts.txt".to_string(),
                    members: vec![],
                    diagnostics: vec![],
                },
                IngestFileInput {
                    room: "gg".to_string(),
                    file_kind: FileKind::HandHistory,
                    sha256: "j".repeat(64),
                    original_filename: "second-hh.txt".to_string(),
                    byte_size: 10,
                    storage_uri: "local://second-hh.txt".to_string(),
                    members: vec![],
                    diagnostics: vec![],
                },
            ],
        },
    )
    .unwrap();

    let claimed_order = (0..4)
        .map(|_| claim_next_job(&mut tx, "test-runner").unwrap().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        claimed_order
            .iter()
            .map(|job| job.member_path.clone().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "first-ts.txt".to_string(),
            "first-hh.txt".to_string(),
            "second-ts.txt".to_string(),
            "second-hh.txt".to_string(),
        ]
    );
    assert_eq!(
        claimed_order
            .iter()
            .map(|job| job.file_kind.unwrap())
            .collect::<Vec<_>>(),
        vec![
            FileKind::TournamentSummary,
            FileKind::HandHistory,
            FileKind::TournamentSummary,
            FileKind::HandHistory,
        ]
    );

    tx.rollback().unwrap();
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn dependent_hh_job_waits_for_successful_ts_job() {
    let _guard = db_test_guard();
    let mut client = Client::connect(&db_url(), NoTls).unwrap();
    apply_all_migrations(&mut client);
    reset_ingest_runtime_tables(&mut client);
    let mut tx = client.transaction().unwrap();
    let (organization_id, user_id, player_profile_id) = seed_actor_shell(&mut tx);
    let bundle = enqueue_bundle(
        &mut tx,
        &paired_archive_bundle_input(organization_id, player_profile_id, user_id),
    )
    .unwrap();

    assert_eq!(bundle.file_jobs.len(), 2);

    let first = claim_next_job(&mut tx, "test-runner").unwrap().unwrap();
    assert_eq!(first.member_path.as_deref(), Some("pair.ts.txt"));
    assert_eq!(first.file_kind, Some(FileKind::TournamentSummary));
    assert!(
        claim_next_job(&mut tx, "test-runner").unwrap().is_none(),
        "dependent HH must stay blocked while TS is still running"
    );

    tracker_ingest_runtime::mark_job_succeeded(&mut tx, first.job_id, first.attempt_no).unwrap();

    let second = claim_next_job(&mut tx, "test-runner").unwrap().unwrap();
    assert_eq!(second.member_path.as_deref(), Some("pair.hh.txt"));
    assert_eq!(second.file_kind, Some(FileKind::HandHistory));

    tx.rollback().unwrap();
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn terminal_failure_propagates_dependency_failed_to_hh_job() {
    let _guard = db_test_guard();
    let mut client = Client::connect(&db_url(), NoTls).unwrap();
    apply_all_migrations(&mut client);
    reset_ingest_runtime_tables(&mut client);
    let mut tx = client.transaction().unwrap();
    let (organization_id, user_id, player_profile_id) = seed_actor_shell(&mut tx);
    let bundle = enqueue_bundle(
        &mut tx,
        &paired_archive_bundle_input(organization_id, player_profile_id, user_id),
    )
    .unwrap();

    let first = claim_next_job(&mut tx, "test-runner").unwrap().unwrap();
    assert_eq!(first.member_path.as_deref(), Some("pair.ts.txt"));

    mark_job_failed(
        &mut tx,
        first.job_id,
        first.attempt_no,
        FailureDisposition::Terminal,
        "parse_error",
    )
    .unwrap();

    let summary = load_bundle_summary(&mut tx, bundle.bundle_id).unwrap();
    assert_eq!(summary.status, BundleStatus::Failed);
    assert_eq!(summary.failed_terminal_file_jobs, 2);

    let hh_row = tx
        .query_one(
            "SELECT jobs.status, jobs.error_code
             FROM import.import_jobs jobs
             JOIN import.source_file_members members
               ON members.id = jobs.source_file_member_id
             WHERE jobs.bundle_id = $1
               AND members.member_path = 'pair.hh.txt'",
            &[&bundle.bundle_id],
        )
        .unwrap();
    assert_eq!(hh_row.get::<_, String>(0), "failed_terminal".to_string());
    assert_eq!(
        hh_row.get::<_, Option<String>>(1),
        Some("dependency_failed".to_string())
    );

    let hh_diagnostic = tx
        .query_one(
            "SELECT message, payload->>'code'
             FROM import.ingest_events
             WHERE bundle_id = $1
               AND bundle_file_id = (
                   SELECT jobs.bundle_file_id
                   FROM import.import_jobs jobs
                   JOIN import.source_file_members members
                     ON members.id = jobs.source_file_member_id
                   WHERE jobs.bundle_id = $1
                     AND members.member_path = 'pair.hh.txt'
               )
               AND event_kind = 'diagnostic_logged'
             ORDER BY sequence_no DESC
             LIMIT 1",
            &[&bundle.bundle_id],
        )
        .unwrap();
    assert_eq!(
        hh_diagnostic.get::<_, Option<String>>(1),
        Some("dependency_failed".to_string())
    );
    assert!(
        hh_diagnostic.get::<_, String>(0).contains("зависимого"),
        "dependency diagnostic should explain why HH was skipped"
    );

    tx.rollback().unwrap();
}
