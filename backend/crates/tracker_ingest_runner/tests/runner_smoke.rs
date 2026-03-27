use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use postgres::{Client, NoTls};
use sha2::{Digest, Sha256};
use tracker_ingest_runner::{RunnerConfig, drain_once};
use tracker_ingest_runtime::{
    FileKind, IngestBundleInput, IngestFileInput, enqueue_bundle, load_bundle_summary,
};
use uuid::Uuid;

fn backend_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("backend root must exist")
        .to_path_buf()
}

fn migrations_dir() -> PathBuf {
    backend_root().join("migrations")
}

fn fixture_path(relative: &str) -> PathBuf {
    backend_root().join(relative)
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
        .expect("CHECK_MATE_DATABASE_URL must exist for ingest runner DB tests")
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
            "DELETE FROM import.ingest_events;
             DELETE FROM import.job_attempts;
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
            "INSERT INTO org.organization_memberships (organization_id, user_id, role)
             VALUES ($1, $2, 'student')
             ON CONFLICT (organization_id, user_id) DO NOTHING",
            &[&organization_id, &user_id],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO core.player_profiles (id, organization_id, owner_user_id, room, network, screen_name)
             VALUES ($1, $2, $3, 'gg', 'gg', $4)",
            &[&player_profile_id, &organization_id, &user_id, &format!("Hero-{player_profile_id}")],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO core.player_aliases (
                organization_id,
                player_profile_id,
                room,
                alias,
                is_primary,
                source
            )
            VALUES ($1, $2, 'gg', $3, TRUE, 'runner_smoke')
            ON CONFLICT (player_profile_id, room, alias)
            DO UPDATE SET
                is_primary = TRUE,
                source = EXCLUDED.source",
            &[&organization_id, &player_profile_id, &format!("Hero-{player_profile_id}")],
        )
        .unwrap();

    (organization_id, user_id, player_profile_id)
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn separate_runner_process_helper_drains_ingest_queue_until_idle() {
    let _guard = db_test_guard();
    let mut client = Client::connect(&db_url(), NoTls).unwrap();
    apply_all_migrations(&mut client);
    reset_ingest_runtime_tables(&mut client);

    let ts_path = fixture_path("fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt");
    let ts_text = fs::read_to_string(&ts_path).unwrap();

    let bundle_id = {
        let mut tx = client.transaction().unwrap();
        let (organization_id, user_id, player_profile_id) = seed_actor_shell(&mut tx);
        let bundle = enqueue_bundle(
            &mut tx,
            &IngestBundleInput {
                organization_id,
                player_profile_id,
                created_by_user_id: user_id,
                files: vec![IngestFileInput {
                    room: "gg".to_string(),
                    file_kind: FileKind::TournamentSummary,
                    sha256: sha256_hex(&ts_text),
                    original_filename: ts_path.file_name().unwrap().to_string_lossy().to_string(),
                    byte_size: ts_text.len() as i64,
                    storage_uri: format!("local://{}", ts_path.display()),
                    members: vec![],
                    diagnostics: vec![],
                }],
            },
        )
        .unwrap();
        tx.commit().unwrap();
        bundle.bundle_id
    };

    let processed_jobs = drain_once(
        &db_url(),
        &RunnerConfig {
            runner_name: "runner-smoke".to_string(),
            max_attempts: 3,
        },
    )
    .unwrap();
    assert_eq!(processed_jobs, 2);

    let mut check_client = Client::connect(&db_url(), NoTls).unwrap();
    let summary = load_bundle_summary(&mut check_client, bundle_id).unwrap();
    assert_eq!(summary.status, tracker_ingest_runtime::BundleStatus::Succeeded);
    assert_eq!(summary.queued_file_jobs, 0);
    assert_eq!(summary.running_file_jobs, 0);
    assert!(summary.finalize_job_present);
}
