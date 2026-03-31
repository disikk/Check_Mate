// tracker_ingest_runtime: модульная структура.
// Публичный API сохранён через re-exports.

pub mod models;
mod events;
mod enqueue;
mod snapshot;
mod queue;
mod finalize;
mod executor;

// Re-export всех публичных типов и функций для сохранения обратной совместимости.
pub use models::*;
pub use enqueue::enqueue_bundle;
pub use snapshot::{load_bundle_events_since, load_bundle_snapshot, load_bundle_summary};
pub use queue::{claim_next_job, mark_job_succeeded, mark_job_failed, retry_failed_job};
pub use finalize::maybe_enqueue_finalize_job;
pub use executor::{JobExecutor, run_next_job};

/// Чистая функция: вычисляет BundleStatus по набору file statuses и finalize readiness.
pub fn compute_bundle_status(
    file_statuses: &[FileJobStatus],
    finalize: FinalizeReadiness,
) -> BundleStatus {
    if matches!(finalize, FinalizeReadiness::Failed) {
        return BundleStatus::Failed;
    }

    if file_statuses
        .iter()
        .any(|status| matches!(status, FileJobStatus::Queued | FileJobStatus::Running))
    {
        return BundleStatus::Running;
    }

    if file_statuses
        .iter()
        .any(|status| matches!(status, FileJobStatus::FailedRetriable))
    {
        return BundleStatus::Running;
    }

    if matches!(finalize, FinalizeReadiness::Ready) {
        return BundleStatus::Finalizing;
    }

    if matches!(finalize, FinalizeReadiness::Completed) {
        let success_count = file_statuses
            .iter()
            .filter(|status| matches!(status, FileJobStatus::Succeeded))
            .count();
        let terminal_failure_count = file_statuses
            .iter()
            .filter(|status| matches!(status, FileJobStatus::FailedTerminal))
            .count();

        if success_count > 0 && terminal_failure_count == 0 {
            return BundleStatus::Succeeded;
        }
        if success_count > 0 && terminal_failure_count > 0 {
            return BundleStatus::PartialSuccess;
        }
        return BundleStatus::Failed;
    }

    BundleStatus::Queued
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{
        fs,
        path::PathBuf,
        sync::{Mutex, OnceLock},
    };

    use postgres::{Client, NoTls};
    use serde_json::Value as JsonValue;
    use uuid::Uuid;

    fn migrations_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
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

    // Эти внутренние helpers переехали в snapshot.rs, но тесты вызывают их
    // через crate-internal path. Для тестов в lib.rs импортируем напрямую.
    use crate::snapshot::{
        bundle_progress_percent_from_summary, file_progress_percent, file_stage_label,
        load_bundle_progress_summary,
    };
    use crate::events::emit_bundle_event;

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn load_bundle_progress_summary_uses_counts_for_progress_and_status() {
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
                        sha256: "a".repeat(64),
                        original_filename: "one.hh".to_string(),
                        byte_size: 10,
                        storage_uri: "local://one.hh".to_string(),
                        members: vec![IngestMemberInput {
                            member_path: "one.hh".to_string(),
                            member_kind: FileKind::HandHistory,
                            sha256: "b".repeat(64),
                            byte_size: 10,
                            depends_on_member_index: None,
                        }],
                        diagnostics: vec![],
                    },
                    IngestFileInput {
                        room: "gg".to_string(),
                        file_kind: FileKind::HandHistory,
                        sha256: "c".repeat(64),
                        original_filename: "two.hh".to_string(),
                        byte_size: 10,
                        storage_uri: "local://two.hh".to_string(),
                        members: vec![IngestMemberInput {
                            member_path: "two.hh".to_string(),
                            member_kind: FileKind::HandHistory,
                            sha256: "d".repeat(64),
                            byte_size: 10,
                            depends_on_member_index: None,
                        }],
                        diagnostics: vec![],
                    },
                ],
            },
        )
        .unwrap();

        let first_job_id: Uuid = tx
            .query_one(
                "SELECT id
                 FROM import.import_jobs
                 WHERE bundle_id = $1 AND job_kind = 'file_ingest'
                 ORDER BY created_at
                 LIMIT 1",
                &[&bundle.bundle_id],
            )
            .unwrap()
            .get(0);
        tx.execute(
            "UPDATE import.import_jobs
             SET status = 'succeeded',
                 stage = 'done'
             WHERE id = $1",
            &[&first_job_id],
        )
        .unwrap();

        let summary = load_bundle_progress_summary(&mut tx, bundle.bundle_id).unwrap();

        assert_eq!(summary.bundle_id, bundle.bundle_id);
        assert_eq!(summary.status, BundleStatus::Queued);
        assert_eq!(summary.total_files, 2);
        assert_eq!(summary.completed_files, 1);
        assert_eq!(bundle_progress_percent_from_summary(&summary), 50);

        tx.rollback().unwrap();
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn load_file_job_status_reads_one_bundle_file_without_full_snapshot() {
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
                    sha256: "e".repeat(64),
                    original_filename: "single.hh".to_string(),
                    byte_size: 10,
                    storage_uri: "local://single.hh".to_string(),
                    members: vec![IngestMemberInput {
                        member_path: "single.hh".to_string(),
                        member_kind: FileKind::HandHistory,
                        sha256: "f".repeat(64),
                        byte_size: 10,
                        depends_on_member_index: None,
                    }],
                    diagnostics: vec![],
                }],
            },
        )
        .unwrap();

        let bundle_file_id: Uuid = tx
            .query_one(
                "SELECT id
                 FROM import.ingest_bundle_files
                 WHERE bundle_id = $1
                 LIMIT 1",
                &[&bundle.bundle_id],
            )
            .unwrap()
            .get(0);
        let job_id: Uuid = tx
            .query_one(
                "SELECT id
                 FROM import.import_jobs
                 WHERE bundle_file_id = $1
                 LIMIT 1",
                &[&bundle_file_id],
            )
            .unwrap()
            .get(0);
        tx.execute(
            "UPDATE import.import_jobs
             SET status = 'running',
                 stage = 'parse'
             WHERE id = $1",
            &[&job_id],
        )
        .unwrap();

        let source_file_id: Uuid = tx
            .query_one(
                "SELECT source_file_id
                 FROM import.ingest_bundle_files
                 WHERE id = $1",
                &[&bundle_file_id],
            )
            .unwrap()
            .get(0);
        let file_status = crate::snapshot::load_file_job_status(&mut tx, bundle.bundle_id, bundle_file_id).unwrap();

        assert_eq!(file_status.bundle_file_id, bundle_file_id);
        assert_eq!(file_status.source_file_id, source_file_id);
        assert_eq!(file_status.status, FileJobStatus::Running);
        assert_eq!(file_status.member_path, "single.hh");
        assert_eq!(
            file_stage_label(file_status.status, &file_status.stage),
            "Парсинг раздач"
        );
        assert_eq!(
            file_progress_percent(file_status.status, &file_status.stage),
            72
        );

        tx.rollback().unwrap();
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn emit_bundle_event_uses_count_based_progress_payload() {
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
                    sha256: "g".repeat(64),
                    original_filename: "queued.hh".to_string(),
                    byte_size: 10,
                    storage_uri: "local://queued.hh".to_string(),
                    members: vec![IngestMemberInput {
                        member_path: "queued.hh".to_string(),
                        member_kind: FileKind::HandHistory,
                        sha256: "h".repeat(64),
                        byte_size: 10,
                        depends_on_member_index: None,
                    }],
                    diagnostics: vec![],
                }],
            },
        )
        .unwrap();

        emit_bundle_event(
            &mut tx,
            bundle.bundle_id,
            "bundle_updated",
            "bundle telemetry check",
        )
        .unwrap();

        let event = load_bundle_events_since(&mut tx, bundle.bundle_id, None)
            .unwrap()
            .into_iter()
            .find(|event| event.event_kind == "bundle_updated")
            .expect("bundle_updated event must exist");

        assert_eq!(
            event
                .payload
                .get("progress_percent")
                .and_then(JsonValue::as_i64),
            Some(0)
        );
        assert_eq!(
            event.payload.get("total_files").and_then(JsonValue::as_i64),
            Some(1)
        );
        assert_eq!(
            event
                .payload
                .get("completed_files")
                .and_then(JsonValue::as_i64),
            Some(0)
        );

        tx.rollback().unwrap();
    }
}
