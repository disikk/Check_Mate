// Finalize job enqueue: проверка готовности и создание bundle_finalize job.
// Перенесено из lib.rs как часть механического рефакторинга.

use anyhow::Result;
use postgres::GenericClient;
use uuid::Uuid;

use crate::events::emit_bundle_event;

pub fn maybe_enqueue_finalize_job(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
) -> Result<Option<Uuid>> {
    let counts = client.query_one(
        "SELECT
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'queued') AS queued_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'running') AS running_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'failed_retriable') AS failed_retriable_count,
             COUNT(*) FILTER (WHERE job_kind = 'file_ingest' AND status = 'succeeded') AS succeeded_count,
             COUNT(*) FILTER (WHERE job_kind = 'bundle_finalize') AS finalize_count
         FROM import.import_jobs
         WHERE bundle_id = $1",
        &[&bundle_id],
    )?;

    let queued_count: i64 = counts.get(0);
    let running_count: i64 = counts.get(1);
    let failed_retriable_count: i64 = counts.get(2);
    let succeeded_count: i64 = counts.get(3);
    let finalize_count: i64 = counts.get(4);

    if queued_count > 0 || running_count > 0 || failed_retriable_count > 0 || finalize_count > 0 {
        return Ok(None);
    }
    if succeeded_count == 0 {
        return Ok(None);
    }

    let organization_id: Uuid = client
        .query_one(
            "SELECT organization_id
             FROM import.ingest_bundles
             WHERE id = $1",
            &[&bundle_id],
        )?
        .get(0);

    let job_id: Uuid = client
        .query_one(
            "INSERT INTO import.import_jobs (
                organization_id,
                bundle_id,
                job_kind,
                status,
                stage
            )
            VALUES ($1, $2, 'bundle_finalize', 'queued', 'queued')
            RETURNING id",
            &[&organization_id, &bundle_id],
        )?
        .get(0);

    client.execute(
        "UPDATE import.ingest_bundles
         SET status = 'finalizing'
         WHERE id = $1",
        &[&bundle_id],
    )?;

    emit_bundle_event(
        client,
        bundle_id,
        "bundle_updated",
        "Подготовка индекса запущена.",
    )?;

    Ok(Some(job_id))
}
