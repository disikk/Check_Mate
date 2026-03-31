// Claim/succeed/fail/retry job логика и status transition.
// Перенесено из lib.rs как часть механического рефакторинга.

use anyhow::Result;
use postgres::GenericClient;
use uuid::Uuid;

use crate::events::{emit_bundle_event, emit_dependency_failed_events, emit_file_updated_event};
use crate::models::*;
use crate::snapshot::load_bundle_summary;

pub fn claim_next_job(
    client: &mut impl GenericClient,
    runner_name: &str,
) -> Result<Option<ClaimedJob>> {
    let Some(row) = client.query_opt(
        "WITH next_job AS (
             SELECT
                 jobs.id,
                 jobs.bundle_id,
                 jobs.bundle_file_id,
                 jobs.source_file_id,
                 jobs.source_file_member_id,
                 jobs.job_kind,
                 jobs.organization_id
             FROM import.import_jobs AS jobs
             LEFT JOIN import.ingest_bundle_files AS bundle_files
               ON bundle_files.id = jobs.bundle_file_id
             LEFT JOIN import.ingest_bundles AS bundles
               ON bundles.id = jobs.bundle_id
             LEFT JOIN import.import_jobs AS dependency_jobs
               ON dependency_jobs.id = jobs.depends_on_job_id
             WHERE jobs.status = 'queued'
               AND jobs.job_kind IN ('file_ingest', 'bundle_finalize')
               AND (
                   jobs.depends_on_job_id IS NULL
                   OR dependency_jobs.status = 'succeeded'
               )
             ORDER BY
                 bundles.queue_order NULLS LAST,
                 CASE WHEN jobs.job_kind = 'file_ingest' THEN 0 ELSE 1 END,
                 COALESCE(bundle_files.file_order_index, 2147483647),
                 jobs.created_at,
                 jobs.id
             FOR UPDATE OF jobs SKIP LOCKED
             LIMIT 1
         )
         SELECT
             next_job.id,
             next_job.bundle_id,
             next_job.bundle_file_id,
             next_job.source_file_id,
             next_job.source_file_member_id,
             next_job.job_kind,
             next_job.organization_id,
             bundles.player_profile_id,
             files.storage_uri,
             files.file_kind,
             members.member_path,
             members.member_kind
         FROM next_job
         LEFT JOIN import.ingest_bundles bundles
           ON bundles.id = next_job.bundle_id
         LEFT JOIN import.source_files files
           ON files.id = next_job.source_file_id
         LEFT JOIN import.source_file_members members
           ON members.id = next_job.source_file_member_id",
        &[],
    )?
    else {
        return Ok(None);
    };

    let job_id: Uuid = row.get(0);
    let bundle_id: Uuid = row.get(1);
    let bundle_file_id: Option<Uuid> = row.get(2);
    let source_file_id: Option<Uuid> = row.get(3);
    let source_file_member_id: Option<Uuid> = row.get(4);
    let job_kind = JobKind::from_db(&row.get::<_, String>(5));
    let organization_id: Uuid = row.get(6);
    let player_profile_id: Uuid = row.get(7);
    let storage_uri: Option<String> = row.get(8);
    let source_file_kind = row
        .get::<_, Option<String>>(9)
        .map(|value| FileKind::from_db(value.as_str()));
    let member_path: Option<String> = row.get(10);
    let file_kind = row
        .get::<_, Option<String>>(11)
        .map(|value| FileKind::from_db(value.as_str()));
    let attempt_no: i32 = client
        .query_one(
            "SELECT COALESCE(MAX(attempt_no), 0) + 1
             FROM import.job_attempts
             WHERE import_job_id = $1",
            &[&job_id],
        )?
        .get(0);

    client.execute(
        "UPDATE import.import_jobs
         SET status = 'running',
             stage = CASE
                 WHEN job_kind = 'bundle_finalize' THEN 'materialize_refresh'
                 ELSE 'parse'
             END,
             claimed_by = $2,
             claimed_at = now(),
             started_at = COALESCE(started_at, now())
         WHERE id = $1",
        &[&job_id, &runner_name],
    )?;

    client.execute(
        "INSERT INTO import.job_attempts (
            import_job_id,
            attempt_no,
            status,
            stage,
            started_at
        )
        VALUES (
            $1,
            $2,
            'running',
            CASE
                WHEN (SELECT job_kind FROM import.import_jobs WHERE id = $1) = 'bundle_finalize'
                    THEN 'materialize_refresh'
                ELSE 'parse'
            END,
            now()
        )",
        &[&job_id, &attempt_no],
    )?;

    if let Some(bundle_file_id) = bundle_file_id {
        let member_path = member_path.clone().unwrap_or_else(|| "файл".to_string());
        emit_file_updated_event(
            client,
            bundle_id,
            bundle_file_id,
            &format!("Файл `{member_path}` принят в работу."),
        )?;
        emit_bundle_event(
            client,
            bundle_id,
            "bundle_updated",
            "Партия файлов обрабатывается.",
        )?;
    }

    Ok(Some(ClaimedJob {
        job_id,
        bundle_id,
        bundle_file_id,
        source_file_id,
        source_file_member_id,
        job_kind,
        organization_id,
        player_profile_id,
        storage_uri,
        source_file_kind,
        member_path,
        file_kind,
        attempt_no,
    }))
}

pub fn mark_job_succeeded(
    client: &mut impl GenericClient,
    job_id: Uuid,
    attempt_no: i32,
) -> Result<()> {
    let row = client.query_one(
        "SELECT bundle_id, bundle_file_id, job_kind
             FROM import.import_jobs
             WHERE id = $1",
        &[&job_id],
    )?;
    let bundle_id: Uuid = row.get(0);
    let bundle_file_id: Option<Uuid> = row.get(1);
    let job_kind = JobKind::from_db(&row.get::<_, String>(2));

    client.execute(
        "UPDATE import.import_jobs
         SET status = 'succeeded',
             stage = 'done',
             finished_at = now()
         WHERE id = $1",
        &[&job_id],
    )?;
    client.execute(
        "UPDATE import.job_attempts
         SET status = 'succeeded',
             stage = 'done',
             finished_at = now()
         WHERE import_job_id = $1
           AND attempt_no = $2",
        &[&job_id, &attempt_no],
    )?;

    refresh_bundle_status(client, bundle_id)?;
    match job_kind {
        JobKind::FileIngest => {
            if let Some(bundle_file_id) = bundle_file_id {
                emit_file_updated_event(
                    client,
                    bundle_id,
                    bundle_file_id,
                    "Файл успешно обработан.",
                )?;
            }
        }
        JobKind::BundleFinalize => {
            let summary = load_bundle_summary(client, bundle_id)?;
            let message = match summary.status {
                BundleStatus::Succeeded => "Партия успешно импортирована.",
                BundleStatus::PartialSuccess => "Партия импортирована с ошибками.",
                BundleStatus::Failed => "Импорт партии завершился ошибкой.",
                _ => "Подготовка индекса завершена.",
            };
            emit_bundle_event(client, bundle_id, "bundle_terminal", message)?;
        }
    }
    Ok(())
}

pub fn mark_job_failed(
    client: &mut impl GenericClient,
    job_id: Uuid,
    attempt_no: i32,
    disposition: FailureDisposition,
    error_code: &str,
) -> Result<()> {
    let row = client.query_one(
        "SELECT bundle_id, bundle_file_id, job_kind
             FROM import.import_jobs
             WHERE id = $1",
        &[&job_id],
    )?;
    let bundle_id: Uuid = row.get(0);
    let bundle_file_id: Option<Uuid> = row.get(1);
    let job_kind = JobKind::from_db(&row.get::<_, String>(2));

    let status = match disposition {
        FailureDisposition::Retriable => "failed_retriable",
        FailureDisposition::Terminal => "failed_terminal",
    };

    client.execute(
        "UPDATE import.import_jobs
         SET status = $2,
             stage = 'failed',
             error_code = $3,
             finished_at = now()
         WHERE id = $1",
        &[&job_id, &status, &error_code],
    )?;
    client.execute(
        "UPDATE import.job_attempts
         SET status = $3,
             stage = 'failed',
             error_code = $4,
             finished_at = now()
         WHERE import_job_id = $1
           AND attempt_no = $2",
        &[&job_id, &attempt_no, &status, &error_code],
    )?;

    let propagated_bundle_files = if matches!(job_kind, JobKind::FileIngest)
        && matches!(disposition, FailureDisposition::Terminal)
    {
        propagate_dependency_failure(client, job_id)?
    } else {
        Vec::new()
    };

    refresh_bundle_status(client, bundle_id)?;
    match job_kind {
        JobKind::FileIngest => {
            if let Some(bundle_file_id) = bundle_file_id {
                let message = match disposition {
                    FailureDisposition::Retriable => "Файл завершился временной ошибкой.",
                    FailureDisposition::Terminal => "Файл завершился ошибкой.",
                };
                emit_file_updated_event(client, bundle_id, bundle_file_id, message)?;
            }
            emit_dependency_failed_events(client, &propagated_bundle_files)?;
        }
        JobKind::BundleFinalize => {
            emit_bundle_event(
                client,
                bundle_id,
                "bundle_terminal",
                "Подготовка индекса завершилась ошибкой.",
            )?;
        }
    }
    Ok(())
}

pub fn retry_failed_job(
    client: &mut impl GenericClient,
    job_id: Uuid,
    max_attempts: i32,
) -> Result<()> {
    let row = client.query_one(
        "SELECT bundle_id, bundle_file_id, status
         FROM import.import_jobs
         WHERE id = $1",
        &[&job_id],
    )?;
    let bundle_id: Uuid = row.get(0);
    let bundle_file_id: Option<Uuid> = row.get(1);
    let status: String = row.get(2);

    if status != "failed_retriable" {
        refresh_bundle_status(client, bundle_id)?;
        return Ok(());
    }

    let attempt_count: i64 = client
        .query_one(
            "SELECT COUNT(*)
             FROM import.job_attempts
             WHERE import_job_id = $1",
            &[&job_id],
        )?
        .get(0);

    let propagated_bundle_files = if attempt_count >= i64::from(max_attempts) {
        client.execute(
            "UPDATE import.import_jobs
             SET status = 'failed_terminal'
             WHERE id = $1",
            &[&job_id],
        )?;
        propagate_dependency_failure(client, job_id)?
    } else {
        client.execute(
            "UPDATE import.import_jobs
             SET status = 'queued',
                 stage = 'queued',
                 claimed_by = NULL,
                 claimed_at = NULL,
                 finished_at = NULL
             WHERE id = $1",
            &[&job_id],
        )?;
        Vec::new()
    };

    refresh_bundle_status(client, bundle_id)?;
    if let Some(bundle_file_id) = bundle_file_id {
        emit_file_updated_event(
            client,
            bundle_id,
            bundle_file_id,
            "Файл повторно поставлен в очередь.",
        )?;
        emit_bundle_event(
            client,
            bundle_id,
            "bundle_updated",
            "Партия ожидает повторной обработки.",
        )?;
    }
    emit_dependency_failed_events(client, &propagated_bundle_files)?;
    Ok(())
}

fn propagate_dependency_failure(
    client: &mut impl GenericClient,
    failed_job_id: Uuid,
) -> Result<Vec<(Uuid, Uuid)>> {
    let rows = client.query(
        "WITH RECURSIVE dependent_jobs AS (
             SELECT jobs.id, jobs.bundle_id, jobs.bundle_file_id
             FROM import.import_jobs AS jobs
             WHERE jobs.depends_on_job_id = $1
               AND jobs.status = 'queued'
             UNION ALL
             SELECT jobs.id, jobs.bundle_id, jobs.bundle_file_id
             FROM import.import_jobs AS jobs
             INNER JOIN dependent_jobs
               ON jobs.depends_on_job_id = dependent_jobs.id
             WHERE jobs.status = 'queued'
         )
         UPDATE import.import_jobs AS jobs
         SET status = 'failed_terminal',
             stage = 'failed',
             error_code = 'dependency_failed',
             finished_at = now()
         FROM dependent_jobs
         WHERE jobs.id = dependent_jobs.id
         RETURNING jobs.bundle_id, jobs.bundle_file_id",
        &[&failed_job_id],
    )?;

    Ok(rows
        .into_iter()
        .filter_map(|row| Some((row.get::<_, Uuid>(0), row.get::<_, Option<Uuid>>(1)?)))
        .collect())
}

pub(crate) fn refresh_bundle_status(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
) -> Result<BundleStatus> {
    let bundle_row = client.query_one(
        "SELECT
             status,
             started_at IS NOT NULL AS started_at_present,
             finished_at IS NOT NULL AS finished_at_present
         FROM import.ingest_bundles
         WHERE id = $1",
        &[&bundle_id],
    )?;
    let stored_status: String = bundle_row.get(0);
    let started_at_present: bool = bundle_row.get(1);
    let finished_at_present: bool = bundle_row.get(2);

    let counts_row = client.query_one(
        "SELECT
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'queued') AS queued_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'running') AS running_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'succeeded') AS succeeded_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'failed_retriable') AS failed_retriable_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'failed_terminal') AS failed_terminal_count,
             COUNT(*) FILTER (WHERE job_kind = 'bundle_finalize') AS finalize_count,
             COUNT(*) FILTER (WHERE job_kind = 'bundle_finalize' AND status IN ('queued', 'running', 'failed_retriable')) AS finalize_active_count
         FROM import.import_jobs
         WHERE bundle_id = $1",
        &[&bundle_id],
    )?;

    let queued_count: i64 = counts_row.get(0);
    let running_count: i64 = counts_row.get(1);
    let succeeded_count: i64 = counts_row.get(2);
    let failed_retriable_count: i64 = counts_row.get(3);
    let failed_terminal_count: i64 = counts_row.get(4);
    let finalize_job_present = counts_row.get::<_, i64>(5) > 0;
    let finalize_job_running = counts_row.get::<_, i64>(6) > 0;

    let status = derive_bundle_status_from_counts(
        queued_count,
        running_count,
        succeeded_count,
        failed_retriable_count,
        failed_terminal_count,
        finalize_job_present,
        finalize_job_running,
    );

    let should_have_started_at = matches!(
        status,
        BundleStatus::Running
            | BundleStatus::Finalizing
            | BundleStatus::Succeeded
            | BundleStatus::PartialSuccess
            | BundleStatus::Failed
    );
    let should_have_finished_at = matches!(
        status,
        BundleStatus::Succeeded | BundleStatus::PartialSuccess | BundleStatus::Failed
    );
    let needs_update = stored_status != status.as_str()
        || (should_have_started_at && !started_at_present)
        || (should_have_finished_at && !finished_at_present)
        || (!should_have_finished_at && finished_at_present);

    if needs_update {
        client.execute(
            "UPDATE import.ingest_bundles
             SET status = $2,
                 started_at = CASE WHEN $2 IN ('running', 'finalizing', 'succeeded', 'partial_success', 'failed')
                     THEN COALESCE(started_at, now())
                     ELSE started_at
                 END,
                 finished_at = CASE WHEN $2 IN ('succeeded', 'partial_success', 'failed')
                     THEN now()
                     ELSE NULL
                 END
             WHERE id = $1",
            &[&bundle_id, &status.as_str()],
        )?;
    }

    Ok(status)
}

pub(crate) fn derive_bundle_status_from_counts(
    queued_count: i64,
    running_count: i64,
    succeeded_count: i64,
    failed_retriable_count: i64,
    failed_terminal_count: i64,
    finalize_job_present: bool,
    finalize_job_running: bool,
) -> BundleStatus {
    if finalize_job_running {
        BundleStatus::Finalizing
    } else if running_count > 0 || failed_retriable_count > 0 {
        BundleStatus::Running
    } else if queued_count > 0 {
        BundleStatus::Queued
    } else if finalize_job_present {
        if succeeded_count > 0 && failed_terminal_count > 0 {
            BundleStatus::PartialSuccess
        } else if succeeded_count > 0 {
            BundleStatus::Succeeded
        } else {
            BundleStatus::Failed
        }
    } else if succeeded_count > 0 && failed_terminal_count > 0 {
        BundleStatus::PartialSuccess
    } else if succeeded_count > 0 {
        BundleStatus::Succeeded
    } else if failed_terminal_count > 0 {
        BundleStatus::Failed
    } else {
        BundleStatus::Queued
    }
}
