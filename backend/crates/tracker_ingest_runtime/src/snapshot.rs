// Snapshot/summary загрузка: load_bundle_snapshot, load_bundle_summary, load_bundle_events_since
// и внутренние helpers для progress/stage label вычислений.
// Перенесено из lib.rs как часть механического рефакторинга.

use anyhow::Result;
use postgres::GenericClient;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::models::*;
use crate::queue::derive_bundle_status_from_counts;

// --- Внутренние helpers для file/bundle stage/progress ---

pub(crate) fn file_stage_label(status: FileJobStatus, stage: &str) -> &'static str {
    match status {
        FileJobStatus::Queued => "Проверка структуры",
        FileJobStatus::Running | FileJobStatus::FailedRetriable => match stage {
            "queued" | "register" => "Проверка структуры",
            "split" | "parse" | "normalize" | "derive" | "persist" => "Парсинг раздач",
            "materialize_refresh" => "Подготовка индекса",
            _ => "Парсинг раздач",
        },
        FileJobStatus::Succeeded => "Готово",
        FileJobStatus::FailedTerminal => "Импорт завершился с ошибкой",
    }
}

pub(crate) fn file_progress_percent(status: FileJobStatus, stage: &str) -> i32 {
    match status {
        FileJobStatus::Queued => 40,
        FileJobStatus::Running | FileJobStatus::FailedRetriable => match stage {
            "queued" | "register" => 40,
            "split" | "parse" | "normalize" | "derive" | "persist" => 72,
            "materialize_refresh" => 95,
            "done" | "failed" => 100,
            _ => 72,
        },
        FileJobStatus::Succeeded | FileJobStatus::FailedTerminal => 100,
    }
}

fn bundle_stage_label(status: BundleStatus, files: &[BundleFileSnapshot]) -> String {
    match status {
        BundleStatus::Queued => "Проверка структуры".to_string(),
        BundleStatus::Running => files
            .iter()
            .find(|file| {
                matches!(
                    file.status,
                    FileJobStatus::Queued | FileJobStatus::Running | FileJobStatus::FailedRetriable
                )
            })
            .map(|file| file.stage_label.clone())
            .unwrap_or_else(|| "Парсинг раздач".to_string()),
        BundleStatus::Finalizing => "Подготовка индекса".to_string(),
        BundleStatus::Succeeded => "Готово".to_string(),
        BundleStatus::PartialSuccess => "Готово с ошибками".to_string(),
        BundleStatus::Failed => "Импорт завершился с ошибкой".to_string(),
    }
}

fn bundle_progress_percent(status: BundleStatus, files: &[BundleFileSnapshot]) -> i32 {
    match status {
        BundleStatus::Succeeded | BundleStatus::PartialSuccess | BundleStatus::Failed => 100,
        BundleStatus::Finalizing => 95,
        BundleStatus::Queued | BundleStatus::Running => {
            if files.is_empty() {
                0
            } else {
                (files
                    .iter()
                    .map(|file| i64::from(file.progress_percent))
                    .sum::<i64>()
                    / files.len() as i64) as i32
            }
        }
    }
}

pub(crate) fn bundle_progress_percent_from_summary(summary: &BundleProgressSummary) -> i32 {
    match summary.status {
        BundleStatus::Succeeded | BundleStatus::PartialSuccess | BundleStatus::Failed => 100,
        BundleStatus::Finalizing => 95,
        BundleStatus::Queued | BundleStatus::Running => {
            if summary.total_files == 0 {
                0
            } else {
                ((summary.completed_files * 100) / summary.total_files) as i32
            }
        }
    }
}

pub(crate) fn bundle_stage_label_from_summary(summary: &BundleProgressSummary) -> String {
    match summary.status {
        BundleStatus::Queued => "Проверка структуры".to_string(),
        BundleStatus::Running => "Парсинг раздач".to_string(),
        BundleStatus::Finalizing => "Подготовка индекса".to_string(),
        BundleStatus::Succeeded => "Готово".to_string(),
        BundleStatus::PartialSuccess => "Готово с ошибками".to_string(),
        BundleStatus::Failed => "Импорт завершился с ошибкой".to_string(),
    }
}

// --- DB-level helpers ---

fn parse_json_payload_text(payload: &str) -> JsonValue {
    serde_json::from_str(payload).unwrap_or_else(|_| JsonValue::Object(Default::default()))
}

fn load_file_diagnostics(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    bundle_file_id: Uuid,
) -> Result<Vec<BundleFileDiagnostic>> {
    let rows = client.query(
        "SELECT message, payload::text
         FROM import.ingest_events
         WHERE bundle_id = $1
           AND bundle_file_id = $2
           AND event_kind = 'diagnostic_logged'
         ORDER BY sequence_no",
        &[&bundle_id, &bundle_file_id],
    )?;

    rows.into_iter()
        .map(|row| {
            let message: String = row.get(0);
            let payload = parse_json_payload_text(&row.get::<_, String>(1));

            Ok(BundleFileDiagnostic {
                code: payload
                    .get("code")
                    .and_then(JsonValue::as_str)
                    .map(ToOwned::to_owned),
                message,
                member_path: payload
                    .get("member_path")
                    .and_then(JsonValue::as_str)
                    .map(ToOwned::to_owned),
            })
        })
        .collect()
}

pub(crate) fn load_bundle_progress_summary(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
) -> Result<BundleProgressSummary> {
    let counts_row = client.query_one(
        "SELECT
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest') AS total_file_jobs,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'succeeded') AS succeeded_file_jobs,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'queued') AS queued_file_jobs,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'running') AS running_file_jobs,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'failed_retriable') AS failed_retriable_file_jobs,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'failed_terminal') AS failed_terminal_file_jobs,
             COUNT(*) FILTER (WHERE job_kind = 'bundle_finalize') AS finalize_job_present,
             COUNT(*) FILTER (WHERE job_kind = 'bundle_finalize' AND status IN ('queued', 'running', 'failed_retriable')) AS finalize_job_running
         FROM import.import_jobs
         WHERE bundle_id = $1",
        &[&bundle_id],
    )?;

    let total_files = counts_row.get::<_, i64>(0);
    let completed_files = counts_row.get::<_, i64>(1);
    let queued_file_jobs = counts_row.get::<_, i64>(2);
    let running_file_jobs = counts_row.get::<_, i64>(3);
    let failed_retriable_file_jobs = counts_row.get::<_, i64>(4);
    let failed_terminal_file_jobs = counts_row.get::<_, i64>(5);
    let finalize_job_present = counts_row.get::<_, i64>(6) > 0;
    let finalize_job_running = counts_row.get::<_, i64>(7) > 0;
    let status = derive_bundle_status_from_counts(
        queued_file_jobs,
        running_file_jobs,
        completed_files,
        failed_retriable_file_jobs,
        failed_terminal_file_jobs,
        finalize_job_present,
        finalize_job_running,
    );

    Ok(BundleProgressSummary {
        bundle_id,
        status,
        total_files,
        completed_files,
    })
}

pub(crate) fn load_file_job_status(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    bundle_file_id: Uuid,
) -> Result<FileJobStatusSummary> {
    let row = client.query_one(
        "SELECT
             bundle_files.source_file_id,
             bundle_files.source_file_member_id,
             members.member_path,
             jobs.status,
             COALESCE(jobs.stage, 'queued')
         FROM import.ingest_bundle_files bundle_files
         JOIN import.source_file_members members
           ON members.id = bundle_files.source_file_member_id
         LEFT JOIN import.import_jobs jobs
           ON jobs.bundle_file_id = bundle_files.id
          AND jobs.job_kind = 'file_ingest'
         WHERE bundle_files.bundle_id = $1
           AND bundle_files.id = $2",
        &[&bundle_id, &bundle_file_id],
    )?;

    Ok(FileJobStatusSummary {
        bundle_file_id,
        source_file_id: row.get(0),
        source_file_member_id: row.get(1),
        member_path: row.get(2),
        status: FileJobStatus::from_db(&row.get::<_, String>(3)),
        stage: row.get(4),
    })
}

// --- Public snapshot/summary/events ---

pub fn load_bundle_events_since(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    after_sequence_no: Option<i64>,
) -> Result<Vec<PersistedIngestEvent>> {
    let rows = client.query(
        "SELECT sequence_no, event_kind, message, payload::text
         FROM import.ingest_events
         WHERE bundle_id = $1
           AND ($2::bigint IS NULL OR sequence_no > $2)
         ORDER BY sequence_no",
        &[&bundle_id, &after_sequence_no],
    )?;

    rows.into_iter()
        .map(|row| {
            let payload_text: String = row.get(3);
            Ok(PersistedIngestEvent {
                sequence_no: row.get(0),
                event_kind: row.get(1),
                message: row.get(2),
                payload: parse_json_payload_text(&payload_text),
            })
        })
        .collect()
}

pub fn load_bundle_snapshot(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
) -> Result<BundleSnapshot> {
    load_bundle_snapshot_inner(client, bundle_id, true)
}

fn load_bundle_snapshot_inner(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    persist_status: bool,
) -> Result<BundleSnapshot> {
    let summary = load_bundle_summary_inner(client, bundle_id, persist_status)?;
    let rows = client.query(
        "SELECT
             bundle_files.id,
             bundle_files.source_file_id,
             bundle_files.source_file_member_id,
             members.member_path,
             jobs.status,
             jobs.stage
         FROM import.ingest_bundle_files bundle_files
         JOIN import.source_file_members members
           ON members.id = bundle_files.source_file_member_id
         LEFT JOIN import.import_jobs jobs
           ON jobs.bundle_file_id = bundle_files.id
          AND jobs.job_kind = 'file_ingest'
         WHERE bundle_files.bundle_id = $1
         ORDER BY bundle_files.file_order_index, bundle_files.id",
        &[&bundle_id],
    )?;

    let mut files = Vec::with_capacity(rows.len());
    for row in rows {
        let bundle_file_id: Uuid = row.get(0);
        let status = FileJobStatus::from_db(&row.get::<_, String>(4));
        let stage: String = row.get(5);

        files.push(BundleFileSnapshot {
            bundle_file_id,
            source_file_id: row.get(1),
            source_file_member_id: row.get(2),
            member_path: row.get(3),
            status,
            stage_label: file_stage_label(status, &stage).to_string(),
            progress_percent: file_progress_percent(status, &stage),
            diagnostics: load_file_diagnostics(client, bundle_id, bundle_file_id)?,
        });
    }

    let completed_files = files
        .iter()
        .filter(|file| {
            matches!(
                file.status,
                FileJobStatus::Succeeded | FileJobStatus::FailedTerminal
            )
        })
        .count() as i64;
    let activity_log = {
        let mut events = load_bundle_events_since(client, bundle_id, None)?;
        events.reverse();
        events
    };

    Ok(BundleSnapshot {
        bundle_id,
        status: summary.status,
        progress_percent: bundle_progress_percent(summary.status, &files),
        stage_label: bundle_stage_label(summary.status, &files),
        total_files: files.len() as i64,
        completed_files,
        files,
        activity_log,
    })
}

pub fn load_bundle_summary(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
) -> Result<BundleSummary> {
    load_bundle_summary_inner(client, bundle_id, true)
}

fn load_bundle_summary_inner(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    persist_status: bool,
) -> Result<BundleSummary> {
    let stored_bundle_status: String = client
        .query_one(
            "SELECT status
             FROM import.ingest_bundles
             WHERE id = $1",
            &[&bundle_id],
        )?
        .get(0);

    let counts_row = client.query_one(
        "SELECT
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'queued') AS queued_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'running') AS running_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'succeeded') AS succeeded_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'failed_retriable') AS failed_retriable_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'failed_terminal') AS failed_terminal_count,
             COUNT(*) FILTER (WHERE job_kind = 'bundle_finalize') AS finalize_count,
             COUNT(*) FILTER (WHERE job_kind = 'bundle_finalize' AND status IN ('queued', 'running', 'failed_retriable')) AS finalize_active_count
         FROM import.import_jobs jobs
         WHERE bundle_id = $1",
        &[&bundle_id],
    )?;

    let queued_file_jobs: i64 = counts_row.get(0);
    let running_file_jobs: i64 = counts_row.get(1);
    let succeeded_file_jobs: i64 = counts_row.get(2);
    let failed_retriable_file_jobs: i64 = counts_row.get(3);
    let failed_terminal_file_jobs: i64 = counts_row.get(4);
    let finalize_job_present = counts_row.get::<_, i64>(5) > 0;
    let finalize_job_running = counts_row.get::<_, i64>(6) > 0;
    let derived_status = derive_bundle_status_from_counts(
        queued_file_jobs,
        running_file_jobs,
        succeeded_file_jobs,
        failed_retriable_file_jobs,
        failed_terminal_file_jobs,
        finalize_job_present,
        finalize_job_running,
    );

    if persist_status && stored_bundle_status != derived_status.as_str() {
        client.execute(
            "UPDATE import.ingest_bundles
             SET status = $2
             WHERE id = $1",
            &[&bundle_id, &derived_status.as_str()],
        )?;
    }

    Ok(BundleSummary {
        bundle_id,
        status: derived_status,
        queued_file_jobs,
        running_file_jobs,
        succeeded_file_jobs,
        failed_retriable_file_jobs,
        failed_terminal_file_jobs,
        finalize_job_present,
        finalize_job_running,
    })
}
