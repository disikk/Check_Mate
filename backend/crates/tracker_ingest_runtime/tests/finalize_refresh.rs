use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use postgres::{Client, GenericClient, NoTls};
use tracker_ingest_runtime::{
    BundleStatus, ClaimedJob, FileKind, IngestBundleInput, IngestFileInput, JobExecutionError,
    JobExecutor, enqueue_bundle, load_bundle_summary, run_next_job,
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

fn seed_actor_shell(client: &mut impl GenericClient) -> (Uuid, Uuid, Uuid) {
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

struct CountingExecutor {
    file_results: VecDeque<std::result::Result<(), JobExecutionError>>,
    finalize_calls: usize,
}

impl JobExecutor for CountingExecutor {
    fn execute_file_job<C: GenericClient>(
        &mut self,
        _client: &mut C,
        _job: &ClaimedJob,
    ) -> std::result::Result<(), JobExecutionError> {
        self.file_results
            .pop_front()
            .expect("file result must exist for every claimed file job")
    }

    fn finalize_bundle<C: GenericClient>(
        &mut self,
        _client: &mut C,
        _job: &ClaimedJob,
    ) -> std::result::Result<(), JobExecutionError> {
        self.finalize_calls += 1;
        Ok(())
    }
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn bundle_finalize_runs_once_after_all_file_jobs_terminalize() {
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
                    sha256: "g".repeat(64),
                    original_filename: "one-hh.txt".to_string(),
                    byte_size: 10,
                    storage_uri: "local://one-hh.txt".to_string(),
                    members: vec![],
                    diagnostics: vec![],
                },
                IngestFileInput {
                    room: "gg".to_string(),
                    file_kind: FileKind::TournamentSummary,
                    sha256: "h".repeat(64),
                    original_filename: "two-ts.txt".to_string(),
                    byte_size: 10,
                    storage_uri: "local://two-ts.txt".to_string(),
                    members: vec![],
                    diagnostics: vec![],
                },
            ],
        },
    )
    .unwrap();

    let mut executor = CountingExecutor {
        file_results: VecDeque::from(vec![
            Ok(()),
            Err(JobExecutionError::terminal("parse_error")),
        ]),
        finalize_calls: 0,
    };

    assert!(run_next_job(&mut tx, "finalize-test", 3, &mut executor)
        .unwrap()
        .is_some());
    assert!(run_next_job(&mut tx, "finalize-test", 3, &mut executor)
        .unwrap()
        .is_some());

    let summary_before_finalize = load_bundle_summary(&mut tx, bundle.bundle_id).unwrap();
    assert!(summary_before_finalize.finalize_job_present);
    assert_eq!(summary_before_finalize.status, BundleStatus::Finalizing);

    let finalize_job = run_next_job(&mut tx, "finalize-test", 3, &mut executor)
        .unwrap()
        .expect("bundle finalize job must be claimed");
    assert_eq!(executor.finalize_calls, 1);
    assert!(matches!(
        finalize_job.job_kind,
        tracker_ingest_runtime::JobKind::BundleFinalize
    ));

    let final_summary = load_bundle_summary(&mut tx, bundle.bundle_id).unwrap();
    assert_eq!(final_summary.status, BundleStatus::PartialSuccess);
    assert_eq!(executor.finalize_calls, 1);

    tx.rollback().unwrap();
}
