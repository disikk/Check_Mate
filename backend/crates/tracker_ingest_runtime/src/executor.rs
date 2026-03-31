// JobExecutor trait и run_next_job оркестрация.
// Перенесено из lib.rs как часть механического рефакторинга.

use anyhow::Result;
use postgres::GenericClient;

use crate::finalize::maybe_enqueue_finalize_job;
use crate::models::*;
use crate::queue::{claim_next_job, mark_job_failed, mark_job_succeeded, retry_failed_job};

pub trait JobExecutor {
    fn execute_file_job<C: GenericClient>(
        &mut self,
        client: &mut C,
        job: &ClaimedJob,
    ) -> std::result::Result<(), JobExecutionError>;

    fn finalize_bundle<C: GenericClient>(
        &mut self,
        client: &mut C,
        job: &ClaimedJob,
    ) -> std::result::Result<(), JobExecutionError>;
}

pub fn run_next_job<E: JobExecutor>(
    client: &mut impl GenericClient,
    runner_name: &str,
    max_attempts: i32,
    executor: &mut E,
) -> Result<Option<ClaimedJob>> {
    let Some(job) = claim_next_job(client, runner_name)? else {
        return Ok(None);
    };

    client.batch_execute("SAVEPOINT ingest_job_execution")?;
    let execution_result = match job.job_kind {
        JobKind::FileIngest => executor.execute_file_job(client, &job),
        JobKind::BundleFinalize => executor.finalize_bundle(client, &job),
    };

    match execution_result {
        Ok(()) => {
            client.batch_execute("RELEASE SAVEPOINT ingest_job_execution")?;
            mark_job_succeeded(client, job.job_id, job.attempt_no)?;
            if matches!(job.job_kind, JobKind::FileIngest) {
                let _ = maybe_enqueue_finalize_job(client, job.bundle_id)?;
            }
        }
        Err(error) => {
            client.batch_execute(
                "ROLLBACK TO SAVEPOINT ingest_job_execution;
                 RELEASE SAVEPOINT ingest_job_execution;",
            )?;
            mark_job_failed(
                client,
                job.job_id,
                job.attempt_no,
                error.disposition,
                &error.error_code,
            )?;
            if matches!(error.disposition, FailureDisposition::Retriable) {
                retry_failed_job(client, job.job_id, max_attempts)?;
            }
            if matches!(job.job_kind, JobKind::FileIngest) {
                let _ = maybe_enqueue_finalize_job(client, job.bundle_id)?;
            }
        }
    }

    Ok(Some(job))
}
