use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use postgres::{Client, NoTls};
use tracker_ingest_runtime::{
    BundleStatus, FileKind, IngestBundleInput, IngestDiagnosticInput, IngestFileInput,
    IngestMemberInput, JobExecutionError, JobExecutor, enqueue_bundle, load_bundle_events_since,
    load_bundle_snapshot, run_next_job,
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

struct ScriptedExecutor {
    file_results: VecDeque<Result<(), JobExecutionError>>,
    finalize_calls: usize,
}

impl JobExecutor for ScriptedExecutor {
    fn execute_file_job<C: postgres::GenericClient>(
        &mut self,
        _client: &mut C,
        _job: &tracker_ingest_runtime::ClaimedJob,
    ) -> Result<(), JobExecutionError> {
        self.file_results
            .pop_front()
            .unwrap_or(Ok(()))
    }

    fn finalize_bundle<C: postgres::GenericClient>(
        &mut self,
        _client: &mut C,
        _job: &tracker_ingest_runtime::ClaimedJob,
    ) -> Result<(), JobExecutionError> {
        self.finalize_calls += 1;
        Ok(())
    }
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn bundle_snapshot_exposes_ui_friendly_progress_and_archive_diagnostics() {
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
                sha256: "q".repeat(64),
                original_filename: "real-bundle.zip".to_string(),
                byte_size: 100,
                storage_uri: "local://real-bundle.zip".to_string(),
                members: vec![IngestMemberInput {
                    member_path: "tables/one.hh".to_string(),
                    member_kind: FileKind::HandHistory,
                    sha256: "r".repeat(64),
                    byte_size: 10,
                }],
                diagnostics: vec![IngestDiagnosticInput {
                    code: "unsupported_archive_member".to_string(),
                    message: "Skipping unsupported ZIP member `notes/readme.md`".to_string(),
                    member_path: Some("notes/readme.md".to_string()),
                }],
            }],
        },
    )
    .unwrap();

    let snapshot = load_bundle_snapshot(&mut tx, bundle.bundle_id).unwrap();

    assert_eq!(snapshot.status, BundleStatus::Queued);
    assert_eq!(snapshot.progress_percent, 40);
    assert_eq!(snapshot.stage_label, "Проверка структуры".to_string());
    assert_eq!(snapshot.total_files, 1);
    assert_eq!(snapshot.completed_files, 0);
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].member_path, "tables/one.hh".to_string());
    assert_eq!(snapshot.files[0].stage_label, "Проверка структуры".to_string());
    assert_eq!(snapshot.files[0].progress_percent, 40);
    assert!(snapshot.activity_log.iter().any(|event| {
        event.event_kind == "diagnostic_logged"
            && event.payload.get("member_path").and_then(|value| value.as_str())
                == Some("notes/readme.md")
    }));

    tx.rollback().unwrap();
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn load_bundle_events_since_returns_ordered_updates_with_single_terminal_event() {
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
                sha256: "s".repeat(64),
                original_filename: "ordered-bundle.zip".to_string(),
                byte_size: 100,
                storage_uri: "local://ordered-bundle.zip".to_string(),
                members: vec![IngestMemberInput {
                    member_path: "tables/two.hh".to_string(),
                    member_kind: FileKind::HandHistory,
                    sha256: "t".repeat(64),
                    byte_size: 10,
                }],
                diagnostics: vec![IngestDiagnosticInput {
                    code: "unsupported_archive_member".to_string(),
                    message: "Skipping unsupported ZIP member `notes/info.json`".to_string(),
                    member_path: Some("notes/info.json".to_string()),
                }],
            }],
        },
    )
    .unwrap();

    let initial_events = load_bundle_events_since(&mut tx, bundle.bundle_id, None).unwrap();
    assert_eq!(
        initial_events
            .iter()
            .map(|event| event.event_kind.as_str())
            .collect::<Vec<_>>(),
        vec!["diagnostic_logged", "bundle_updated"]
    );
    let initial_cursor = initial_events.last().unwrap().sequence_no;

    let mut executor = ScriptedExecutor {
        file_results: VecDeque::from(vec![Ok(())]),
        finalize_calls: 0,
    };

    assert!(run_next_job(&mut tx, "event-stream-test", 3, &mut executor)
        .unwrap()
        .is_some());
    assert!(run_next_job(&mut tx, "event-stream-test", 3, &mut executor)
        .unwrap()
        .is_some());

    let streamed_events = load_bundle_events_since(&mut tx, bundle.bundle_id, Some(initial_cursor))
        .unwrap();
    assert_eq!(
        streamed_events
            .iter()
            .map(|event| event.event_kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "file_updated",
            "bundle_updated",
            "file_updated",
            "bundle_updated",
            "bundle_terminal",
        ]
    );
    assert_eq!(
        streamed_events
            .iter()
            .filter(|event| event.event_kind == "bundle_terminal")
            .count(),
        1
    );
    assert_eq!(
        streamed_events.last().map(|event| event.event_kind.as_str()),
        Some("bundle_terminal")
    );
    assert_eq!(executor.finalize_calls, 1);

    let terminal_snapshot = load_bundle_snapshot(&mut tx, bundle.bundle_id).unwrap();
    assert_eq!(terminal_snapshot.status, BundleStatus::Succeeded);
    assert_eq!(terminal_snapshot.progress_percent, 100);
    assert_eq!(terminal_snapshot.stage_label, "Готово".to_string());
    assert_eq!(terminal_snapshot.completed_files, 1);

    tx.rollback().unwrap();
}
