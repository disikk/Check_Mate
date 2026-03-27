use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use postgres::{Client, NoTls};
use tracker_ingest_runtime::{
    FileKind, IngestBundleInput, IngestDiagnosticInput, IngestFileInput, IngestMemberInput,
    enqueue_bundle, load_bundle_summary,
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
            "INSERT INTO core.player_profiles (id, organization_id, owner_user_id, room, network, screen_name)
             VALUES ($1, $2, $3, 'gg', 'gg', $4)",
            &[&player_profile_id, &organization_id, &user_id, &format!("Hero-{player_profile_id}")],
        )
        .unwrap();

    (organization_id, user_id, player_profile_id)
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn flat_file_enqueue_creates_synthetic_member_and_member_level_job() {
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
            files: vec![IngestFileInput {
                room: "gg".to_string(),
                file_kind: FileKind::HandHistory,
                sha256: "m".repeat(64),
                original_filename: "one-hh.txt".to_string(),
                byte_size: 10,
                storage_uri: "local://one-hh.txt".to_string(),
                members: vec![],
                diagnostics: vec![],
            }],
        },
    )
    .unwrap();

    let summary = load_bundle_summary(&mut tx, bundle.bundle_id).unwrap();
    assert_eq!(summary.queued_file_jobs, 1);

    let member_row = tx
        .query_one(
            "SELECT members.member_index, members.member_path, members.member_kind
             FROM import.ingest_bundle_files bundle_files
             JOIN import.source_file_members members
               ON members.id = bundle_files.source_file_member_id
             WHERE bundle_files.id = $1",
            &[&bundle.file_jobs[0].bundle_file_id],
        )
        .unwrap();

    assert_eq!(member_row.get::<_, i32>(0), 0);
    assert_eq!(member_row.get::<_, String>(1), "one-hh.txt".to_string());
    assert_eq!(member_row.get::<_, String>(2), "hh".to_string());

    let job_member_id: Uuid = tx
        .query_one(
            "SELECT source_file_member_id
             FROM import.import_jobs
             WHERE id = $1",
            &[&bundle.file_jobs[0].job_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(job_member_id, bundle.file_jobs[0].source_file_member_id);

    tx.rollback().unwrap();
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn archive_enqueue_expands_supported_members_and_logs_skipped_entries() {
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
            files: vec![IngestFileInput {
                room: "gg".to_string(),
                file_kind: FileKind::Archive,
                sha256: "n".repeat(64),
                original_filename: "bundle.zip".to_string(),
                byte_size: 100,
                storage_uri: "local://bundle.zip".to_string(),
                members: vec![
                    IngestMemberInput {
                        member_path: "tables/one.hh".to_string(),
                        member_kind: FileKind::HandHistory,
                        sha256: "o".repeat(64),
                        byte_size: 10,
                    },
                    IngestMemberInput {
                        member_path: "tables/two.ts".to_string(),
                        member_kind: FileKind::TournamentSummary,
                        sha256: "p".repeat(64),
                        byte_size: 10,
                    },
                ],
                diagnostics: vec![IngestDiagnosticInput {
                    code: "unsupported_archive_member".to_string(),
                    message: "Skipping unsupported ZIP member `notes/readme.md`".to_string(),
                    member_path: Some("notes/readme.md".to_string()),
                }],
            }],
        },
    )
    .unwrap();

    assert_eq!(bundle.file_jobs.len(), 2);
    assert_eq!(
        bundle.file_jobs[0].source_file_id,
        bundle.file_jobs[1].source_file_id
    );
    assert_ne!(
        bundle.file_jobs[0].source_file_member_id,
        bundle.file_jobs[1].source_file_member_id
    );

    let diagnostic_events = tx
        .query(
            "SELECT event_kind, message, payload->>'member_path'
             FROM import.ingest_events
             WHERE bundle_id = $1
             ORDER BY sequence_no",
            &[&bundle.bundle_id],
        )
        .unwrap();

    assert!(diagnostic_events.iter().any(|row| {
        row.get::<_, String>(0) == "diagnostic_logged"
            && row.get::<_, String>(1).contains("unsupported ZIP member")
            && row.get::<_, Option<String>>(2) == Some("notes/readme.md".to_string())
    }));

    tx.rollback().unwrap();
}
