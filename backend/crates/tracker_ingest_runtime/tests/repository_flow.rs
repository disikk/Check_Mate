use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use postgres::{Client, NoTls};
use tracker_ingest_runtime::{
    BundleStatus, BundleSummary, FileKind, IngestBundleInput, IngestFileInput, enqueue_bundle,
    load_bundle_summary,
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

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn enqueue_bundle_reuses_deduped_source_files_but_creates_fresh_bundle_membership() {
    let _guard = db_test_guard();
    let mut client = Client::connect(&db_url(), NoTls).unwrap();
    apply_all_migrations(&mut client);
    reset_ingest_runtime_tables(&mut client);
    let mut tx = client.transaction().unwrap();
    let (organization_id, user_id, player_profile_id) = seed_actor_shell(&mut tx);

    let input = IngestBundleInput {
        organization_id,
        player_profile_id,
        created_by_user_id: user_id,
        files: vec![IngestFileInput {
            room: "gg".to_string(),
            file_kind: FileKind::HandHistory,
            sha256: "a".repeat(64),
            original_filename: "first-hh.txt".to_string(),
            byte_size: 10,
            storage_uri: "local://first-hh.txt".to_string(),
        }],
    };

    let first = enqueue_bundle(&mut tx, &input).unwrap();
    let second = enqueue_bundle(&mut tx, &input).unwrap();

    assert_ne!(first.bundle_id, second.bundle_id);
    assert_eq!(first.file_jobs.len(), 1);
    assert_eq!(second.file_jobs.len(), 1);
    assert_eq!(
        first.file_jobs[0].source_file_id,
        second.file_jobs[0].source_file_id
    );
    assert_ne!(
        first.file_jobs[0].bundle_file_id,
        second.file_jobs[0].bundle_file_id
    );
    assert_ne!(first.file_jobs[0].job_id, second.file_jobs[0].job_id);

    tx.rollback().unwrap();
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn enqueue_bundle_creates_one_file_job_per_bundle_file_and_defers_finalize() {
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
                    sha256: "b".repeat(64),
                    original_filename: "one-hh.txt".to_string(),
                    byte_size: 10,
                    storage_uri: "local://one-hh.txt".to_string(),
                },
                IngestFileInput {
                    room: "gg".to_string(),
                    file_kind: FileKind::TournamentSummary,
                    sha256: "c".repeat(64),
                    original_filename: "one-ts.txt".to_string(),
                    byte_size: 10,
                    storage_uri: "local://one-ts.txt".to_string(),
                },
            ],
        },
    )
    .unwrap();

    let summary = load_bundle_summary(&mut tx, bundle.bundle_id).unwrap();
    assert_eq!(bundle.file_jobs.len(), 2);
    assert_eq!(
        summary,
        BundleSummary {
            bundle_id: bundle.bundle_id,
            status: BundleStatus::Queued,
            queued_file_jobs: 2,
            running_file_jobs: 0,
            succeeded_file_jobs: 0,
            failed_retriable_file_jobs: 0,
            failed_terminal_file_jobs: 0,
            finalize_job_present: false,
            finalize_job_running: false,
        }
    );

    tx.rollback().unwrap();
}
