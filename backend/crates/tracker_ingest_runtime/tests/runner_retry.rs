use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use postgres::{Client, NoTls};
use tracker_ingest_runtime::{
    BundleStatus, FailureDisposition, FileKind, IngestBundleInput, IngestFileInput,
    claim_next_job, enqueue_bundle, load_bundle_summary, mark_job_failed, retry_failed_job,
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
        let sql = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("failed to read migration {}: {error}", path.display())
        });
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
