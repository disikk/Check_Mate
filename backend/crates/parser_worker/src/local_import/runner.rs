use std::thread;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use mbr_stats_runtime::{
    load_bundle_tournament_ids,
    materialize_player_hand_features_for_tournaments,
};
use postgres::{Client, NoTls};
use tracker_ingest_runtime::{
    ClaimedJob as IngestClaimedJob, FailureDisposition,
    FileKind as IngestFileKind, JobExecutionError, JobExecutor, JobKind as IngestJobKind,
    claim_next_job, mark_job_failed, mark_job_succeeded,
    maybe_enqueue_finalize_job, retry_failed_job,
};

use super::archive::{ArchiveReaderCache, load_ingest_job_input};
use super::compute_rows::*;
use super::context::load_existing_context;
use super::mbr_domain::*;
use super::persist::*;
use super::profiles::*;
use super::row_models::*;
use super::DEFAULT_RUNNER_WORKER_CAP;

pub(crate) struct LocalImportExecutor {
    pub(crate) report: Option<LocalImportReport>,
    pub(crate) run_profile: IngestRunProfile,
    pub(crate) last_finalize_profile: ComputeProfile,
    pub(crate) archive_reader_cache: ArchiveReaderCache,
}

pub fn run_ingest_runner_until_idle(
    database_url: &str,
    runner_name: &str,
    max_attempts: i32,
) -> Result<usize> {
    Ok(
        run_ingest_runner_until_idle_with_profile(database_url, runner_name, max_attempts)?
            .processed_jobs,
    )
}

pub fn run_ingest_runner_until_idle_with_profile(
    database_url: &str,
    runner_name: &str,
    max_attempts: i32,
) -> Result<IngestRunProfile> {
    run_ingest_runner_parallel(database_url, runner_name, max_attempts, 1)
}

pub fn default_runner_worker_count() -> usize {
    thread::available_parallelism()
        .map(|value| value.get().min(DEFAULT_RUNNER_WORKER_CAP))
        .unwrap_or(1)
}

pub fn run_ingest_runner_parallel(
    database_url: &str,
    runner_name: &str,
    max_attempts: i32,
    worker_count: usize,
) -> Result<IngestRunProfile> {
    if worker_count == 0 {
        return Err(anyhow!("worker_count must be greater than zero"));
    }

    if worker_count == 1 {
        return run_ingest_runner_worker(database_url, runner_name, max_attempts);
    }

    let mut handles = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let database_url = database_url.to_string();
        let runner_name = runner_name.to_string();
        handles.push(thread::spawn(move || {
            run_ingest_runner_worker(&database_url, &runner_name, max_attempts)
        }));
    }

    let mut run_profile = IngestRunProfile::default();
    for handle in handles {
        let worker_profile = handle
            .join()
            .map_err(|_| anyhow!("ingest runner worker thread panicked"))??;
        run_profile.add_assign(worker_profile);
    }

    Ok(run_profile)
}

pub(crate) fn run_ingest_runner_worker(
    database_url: &str,
    runner_name: &str,
    max_attempts: i32,
) -> Result<IngestRunProfile> {
    let mut client =
        Client::connect(database_url, NoTls).context("failed to connect to PostgreSQL")?;
    let mut executor = LocalImportExecutor {
        report: None,
        run_profile: IngestRunProfile::default(),
        last_finalize_profile: ComputeProfile::default(),
        archive_reader_cache: ArchiveReaderCache::default(),
    };

    loop {
        let claimed = run_next_job_split_tx(&mut client, runner_name, max_attempts, &mut executor)?;
        let Some(_job) = claimed else {
            break;
        };

        executor.run_profile.processed_jobs += 1;
    }

    Ok(executor.run_profile)
}

pub(crate) fn run_next_job_split_tx(
    client: &mut Client,
    runner_name: &str,
    max_attempts: i32,
    executor: &mut LocalImportExecutor,
) -> Result<Option<IngestClaimedJob>> {
    let mut tx = client
        .transaction()
        .context("failed to start ingest runner transaction")?;
    let claimed = claim_next_job(&mut tx, runner_name)?;
    tx.commit()
        .context("failed to commit ingest runner transaction")?;

    let Some(job) = claimed else {
        return Ok(None);
    };

    match job.job_kind {
        IngestJobKind::FileIngest => {
            let prepared = executor.compute_file_job(&job);
            match prepared {
                Ok(prepared) => {
                    let persist_result = {
                        let mut tx = client
                            .transaction()
                            .context("failed to start ingest file persist transaction")?;
                        tx.batch_execute("SAVEPOINT ingest_job_execution")
                            .context("failed to create ingest savepoint")?;
                        let result = executor.persist_prepared_file_job(&mut tx, &job, prepared);
                        match result {
                            Ok(()) => {
                                tx.batch_execute("RELEASE SAVEPOINT ingest_job_execution")
                                    .context("failed to release ingest savepoint")?;
                                mark_job_succeeded(&mut tx, job.job_id, job.attempt_no)?;
                                let _ = maybe_enqueue_finalize_job(&mut tx, job.bundle_id)?;
                                tx.commit()
                                    .context("failed to commit ingest file persist transaction")?;
                                Ok(())
                            }
                            Err(error) => {
                                tx.batch_execute(
                                    "ROLLBACK TO SAVEPOINT ingest_job_execution;
                                     RELEASE SAVEPOINT ingest_job_execution;",
                                )
                                .context("failed to roll back ingest savepoint")?;
                                tx.commit().context(
                                    "failed to commit rolled-back ingest persist transaction",
                                )?;
                                Err(error)
                            }
                        }
                    };
                    if let Err(error) = persist_result {
                        handle_split_job_failure(client, &job, error, max_attempts)?;
                    }
                }
                Err(error) => {
                    handle_split_job_failure(client, &job, error, max_attempts)?;
                }
            }
        }
        IngestJobKind::BundleFinalize => {
            let finalize_result = {
                let mut tx = client
                    .transaction()
                    .context("failed to start bundle finalize transaction")?;
                tx.batch_execute("SAVEPOINT ingest_job_execution")
                    .context("failed to create finalize savepoint")?;
                let result = executor.finalize_bundle_split(&mut tx, &job);
                match result {
                    Ok(()) => {
                        tx.batch_execute("RELEASE SAVEPOINT ingest_job_execution")
                            .context("failed to release finalize savepoint")?;
                        mark_job_succeeded(&mut tx, job.job_id, job.attempt_no)?;
                        tx.commit()
                            .context("failed to commit bundle finalize transaction")?;
                        Ok(())
                    }
                    Err(error) => {
                        tx.batch_execute(
                            "ROLLBACK TO SAVEPOINT ingest_job_execution;
                             RELEASE SAVEPOINT ingest_job_execution;",
                        )
                        .context("failed to roll back finalize savepoint")?;
                        tx.commit()
                            .context("failed to commit rolled-back bundle finalize transaction")?;
                        Err(error)
                    }
                }
            };
            if let Err(error) = finalize_result {
                handle_split_job_failure(client, &job, error, max_attempts)?;
            }
        }
    }

    Ok(Some(job))
}

pub(crate) fn handle_split_job_failure(
    client: &mut Client,
    job: &IngestClaimedJob,
    error: ExecutionFailure,
    max_attempts: i32,
) -> Result<()> {
    let mut tx = client
        .transaction()
        .context("failed to start job failure transaction")?;
    let disposition = error.disposition;
    let error_code = error.error_code;
    mark_job_failed(
        &mut tx,
        job.job_id,
        job.attempt_no,
        disposition,
        &error_code,
    )?;
    if matches!(disposition, FailureDisposition::Retriable) {
        retry_failed_job(&mut tx, job.job_id, max_attempts)?;
    }
    if matches!(job.job_kind, IngestJobKind::FileIngest) {
        let _ = maybe_enqueue_finalize_job(&mut tx, job.bundle_id)?;
    }
    tx.commit()
        .context("failed to commit job failure transaction")?;
    Ok(())
}

impl LocalImportExecutor {
    pub(crate) fn compute_file_job(
        &mut self,
        job: &IngestClaimedJob,
    ) -> std::result::Result<PreparedRuntimeFileJob, ExecutionFailure> {
        let (_path, input) = load_ingest_job_input(&mut self.archive_reader_cache, job)
            .map_err(ExecutionFailure::from)?;
        match job.file_kind {
            Some(IngestFileKind::TournamentSummary) => {
                Ok(PreparedRuntimeFileJob::TournamentSummary {
                    prepared: prepare_tournament_summary_import(&input)
                        .map_err(|error| ExecutionFailure::terminal(format!("{error:#}")))?,
                    input,
                })
            }
            Some(IngestFileKind::HandHistory) => Ok(PreparedRuntimeFileJob::HandHistory(
                prepare_hand_history_import(&input, job.player_profile_id)
                    .map_err(|error| ExecutionFailure::terminal(format!("{error:#}")))?,
            )),
            Some(IngestFileKind::Archive) => Err(ExecutionFailure::terminal(
                "archive top-level kind cannot be executed as a parsed member job",
            )),
            None => Err(ExecutionFailure::terminal("missing_file_kind")),
        }
    }

    pub(crate) fn persist_prepared_file_job<C: postgres::GenericClient>(
        &mut self,
        client: &mut C,
        job: &IngestClaimedJob,
        prepared: PreparedRuntimeFileJob,
    ) -> std::result::Result<(), ExecutionFailure> {
        let context = load_existing_context(client, job.organization_id, job.player_profile_id)
            .map_err(|_| ExecutionFailure::terminal("missing_execution_context"))?;
        let report = match prepared {
            PreparedRuntimeFileJob::TournamentSummary { input, prepared } => {
                persist_prepared_tournament_summary_registered(
                    client,
                    &context,
                    &input,
                    job.source_file_id
                        .ok_or_else(|| ExecutionFailure::terminal("missing_source_file_id"))?,
                    job.source_file_member_id.ok_or_else(|| {
                        ExecutionFailure::terminal("missing_source_file_member_id")
                    })?,
                    job.job_id,
                    &prepared,
                )
            }
            PreparedRuntimeFileJob::HandHistory(prepared) => {
                persist_prepared_hand_history_registered(
                    client,
                    &context,
                    job.source_file_id
                        .ok_or_else(|| ExecutionFailure::terminal("missing_source_file_id"))?,
                    job.source_file_member_id.ok_or_else(|| {
                        ExecutionFailure::terminal("missing_source_file_member_id")
                    })?,
                    job.job_id,
                    &prepared,
                )
            }
        }
        .map_err(|error| ExecutionFailure::terminal(format!("{error:#}")))?;

        self.run_profile.record_file_job(&report);
        self.report = Some(report);
        Ok(())
    }

    pub(crate) fn finalize_bundle_split<C: postgres::GenericClient>(
        &mut self,
        client: &mut C,
        job: &IngestClaimedJob,
    ) -> std::result::Result<(), ExecutionFailure> {
        let finalize_started_at = Instant::now();

        // Step 1: Load tournament_ids via member-aware path (same SQL as materializer).
        let tournament_ids = load_bundle_tournament_ids(
            client,
            job.bundle_id,
            job.organization_id,
            job.player_profile_id,
        )
        .map_err(|error| ExecutionFailure::retriable(error.to_string()))?;

        // Step 2: Compute tournament_hand_order once per tournament (deferred from file jobs
        // to eliminate row lock contention between workers on the same tournament).
        let t_hand_order = Instant::now();
        for tournament_id in &tournament_ids {
            compute_tournament_hand_order(client, *tournament_id)
                .map_err(|error| ExecutionFailure::retriable(error.to_string()))?;
        }
        let hand_order_ms = t_hand_order.elapsed().as_millis() as u64;

        // Step 3: Build and persist FT helpers from DB (all data already persisted by file jobs).
        let t_ft_helper = Instant::now();
        for tournament_id in &tournament_ids {
            let source_hands =
                load_ft_helper_source_hands_from_db(client, *tournament_id, job.player_profile_id)
                    .map_err(|error| ExecutionFailure::retriable(error.to_string()))?;
            let ft_helper_row = build_mbr_tournament_ft_helper_row(
                *tournament_id,
                job.player_profile_id,
                &source_hands,
            );
            persist_mbr_tournament_ft_helper(client, &ft_helper_row)
                .map_err(|error| ExecutionFailure::retriable(error.to_string()))?;
        }
        let _ft_helper_ms = t_ft_helper.elapsed().as_millis() as u64;

        // Step 4: Materialize analytics features directly with already-loaded tournament_ids
        // (skips redundant load_bundle_tournament_ids inside _for_bundle).
        let materialize_started_at = Instant::now();
        materialize_player_hand_features_for_tournaments(
            client,
            job.organization_id,
            job.player_profile_id,
            &tournament_ids,
        )
        .map_err(|error| ExecutionFailure::retriable(error.to_string()))?;
        let materialize_ms = materialize_started_at.elapsed().as_millis() as u64;

        let finalize_ms = finalize_started_at.elapsed().as_millis() as u64;
        let runtime_profile = ComputeProfile {
            materialize_ms,
            finalize_ms,
            persist_hand_order_ms: hand_order_ms,
            ..ComputeProfile::default()
        };
        self.last_finalize_profile = runtime_profile;
        self.run_profile.record_finalize_job(runtime_profile);
        Ok(())
    }
}

impl JobExecutor for LocalImportExecutor {
    fn execute_file_job<C: postgres::GenericClient>(
        &mut self,
        client: &mut C,
        job: &IngestClaimedJob,
    ) -> std::result::Result<(), JobExecutionError> {
        let prepared = self
            .compute_file_job(job)
            .map_err(|error| match error.disposition {
                FailureDisposition::Retriable => JobExecutionError::retriable(error.error_code),
                FailureDisposition::Terminal => JobExecutionError::terminal(error.error_code),
            })?;
        self.persist_prepared_file_job(client, job, prepared)
            .map_err(|error| match error.disposition {
                FailureDisposition::Retriable => JobExecutionError::retriable(error.error_code),
                FailureDisposition::Terminal => JobExecutionError::terminal(error.error_code),
            })
    }

    fn finalize_bundle<C: postgres::GenericClient>(
        &mut self,
        client: &mut C,
        job: &IngestClaimedJob,
    ) -> std::result::Result<(), JobExecutionError> {
        self.finalize_bundle_split(client, job)
            .map_err(|error| match error.disposition {
                FailureDisposition::Retriable => JobExecutionError::retriable(error.error_code),
                FailureDisposition::Terminal => JobExecutionError::terminal(error.error_code),
            })
    }
}
