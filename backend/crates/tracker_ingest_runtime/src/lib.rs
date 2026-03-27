use anyhow::Result;
use postgres::GenericClient;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleStatus {
    Queued,
    Running,
    Finalizing,
    Succeeded,
    PartialSuccess,
    Failed,
}

impl BundleStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Finalizing => "finalizing",
            Self::Succeeded => "succeeded",
            Self::PartialSuccess => "partial_success",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileJobStatus {
    Queued,
    Running,
    Succeeded,
    FailedRetriable,
    FailedTerminal,
}

impl FileJobStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::FailedRetriable => "failed_retriable",
            Self::FailedTerminal => "failed_terminal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeReadiness {
    NotReady,
    Ready,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    HandHistory,
    TournamentSummary,
}

impl FileKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::HandHistory => "hh",
            Self::TournamentSummary => "ts",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureDisposition {
    Retriable,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestFileInput {
    pub room: String,
    pub file_kind: FileKind,
    pub sha256: String,
    pub original_filename: String,
    pub byte_size: i64,
    pub storage_uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestBundleInput {
    pub organization_id: Uuid,
    pub player_profile_id: Uuid,
    pub created_by_user_id: Uuid,
    pub files: Vec<IngestFileInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueuedFileJob {
    pub bundle_file_id: Uuid,
    pub source_file_id: Uuid,
    pub job_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueuedBundle {
    pub bundle_id: Uuid,
    pub file_jobs: Vec<EnqueuedFileJob>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleSummary {
    pub bundle_id: Uuid,
    pub status: BundleStatus,
    pub queued_file_jobs: i64,
    pub running_file_jobs: i64,
    pub succeeded_file_jobs: i64,
    pub failed_retriable_file_jobs: i64,
    pub failed_terminal_file_jobs: i64,
    pub finalize_job_present: bool,
    pub finalize_job_running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedJob {
    pub job_id: Uuid,
    pub bundle_id: Uuid,
    pub bundle_file_id: Option<Uuid>,
    pub source_file_id: Option<Uuid>,
    pub job_kind: JobKind,
    pub organization_id: Uuid,
    pub player_profile_id: Uuid,
    pub storage_uri: Option<String>,
    pub file_kind: Option<FileKind>,
    pub attempt_no: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    FileIngest,
    BundleFinalize,
}

impl JobKind {
    fn from_db(value: &str) -> Self {
        match value {
            "file_ingest" => Self::FileIngest,
            "bundle_finalize" => Self::BundleFinalize,
            other => panic!("unexpected job kind: {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobExecutionError {
    disposition: FailureDisposition,
    error_code: String,
}

impl JobExecutionError {
    pub fn retriable(error_code: impl Into<String>) -> Self {
        Self {
            disposition: FailureDisposition::Retriable,
            error_code: error_code.into(),
        }
    }

    pub fn terminal(error_code: impl Into<String>) -> Self {
        Self {
            disposition: FailureDisposition::Terminal,
            error_code: error_code.into(),
        }
    }
}

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

pub fn enqueue_bundle(
    client: &mut impl GenericClient,
    input: &IngestBundleInput,
) -> Result<EnqueuedBundle> {
    let bundle_id: Uuid = client
        .query_one(
            "INSERT INTO import.ingest_bundles (
                organization_id,
                player_profile_id,
                created_by_user_id,
                status
            )
            VALUES ($1, $2, $3, $4)
            RETURNING id",
            &[
                &input.organization_id,
                &input.player_profile_id,
                &input.created_by_user_id,
                &BundleStatus::Queued.as_str(),
            ],
        )?
        .get(0);

    let mut file_jobs = Vec::with_capacity(input.files.len());
    for (index, file) in input.files.iter().enumerate() {
        let source_file_id: Uuid = client
            .query_one(
                "INSERT INTO import.source_files (
                    organization_id,
                    uploaded_by_user_id,
                    owner_user_id,
                    player_profile_id,
                    room,
                    file_kind,
                    sha256,
                    original_filename,
                    byte_size,
                    storage_uri
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (player_profile_id, room, file_kind, sha256)
                DO UPDATE SET
                    organization_id = EXCLUDED.organization_id,
                    uploaded_by_user_id = EXCLUDED.uploaded_by_user_id,
                    owner_user_id = EXCLUDED.owner_user_id,
                    original_filename = EXCLUDED.original_filename,
                    byte_size = EXCLUDED.byte_size,
                    storage_uri = EXCLUDED.storage_uri
                RETURNING id",
                &[
                    &input.organization_id,
                    &input.created_by_user_id,
                    &input.created_by_user_id,
                    &input.player_profile_id,
                    &file.room,
                    &file.file_kind.as_str(),
                    &file.sha256,
                    &file.original_filename,
                    &file.byte_size,
                    &file.storage_uri,
                ],
            )?
            .get(0);

        let bundle_file_id: Uuid = client
            .query_one(
                "INSERT INTO import.ingest_bundle_files (
                    bundle_id,
                    source_file_id,
                    file_order_index
                )
                VALUES ($1, $2, $3)
                RETURNING id",
                &[&bundle_id, &source_file_id, &(index as i32)],
            )?
            .get(0);

        let job_id: Uuid = client
            .query_one(
                "INSERT INTO import.import_jobs (
                    organization_id,
                    bundle_id,
                    bundle_file_id,
                    source_file_id,
                    job_kind,
                    status,
                    stage
                )
                VALUES ($1, $2, $3, $4, 'file_ingest', $5, 'queued')
                RETURNING id",
                &[
                    &input.organization_id,
                    &bundle_id,
                    &bundle_file_id,
                    &source_file_id,
                    &FileJobStatus::Queued.as_str(),
                ],
            )?
            .get(0);

        file_jobs.push(EnqueuedFileJob {
            bundle_file_id,
            source_file_id,
            job_id,
        });
    }

    Ok(EnqueuedBundle {
        bundle_id,
        file_jobs,
    })
}

pub fn load_bundle_summary(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
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

    if stored_bundle_status != derived_status.as_str() {
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

pub fn claim_next_job(client: &mut impl GenericClient, runner_name: &str) -> Result<Option<ClaimedJob>> {
    let Some(row) = client.query_opt(
        "WITH next_job AS (
             SELECT
                 id,
                 bundle_id,
                 bundle_file_id,
                 source_file_id,
                 job_kind,
                 organization_id
             FROM import.import_jobs
             WHERE status = 'queued'
               AND job_kind IN ('file_ingest', 'bundle_finalize')
             ORDER BY created_at, id
             FOR UPDATE SKIP LOCKED
             LIMIT 1
         )
         SELECT
             next_job.id,
             next_job.bundle_id,
             next_job.bundle_file_id,
             next_job.source_file_id,
             next_job.job_kind,
             next_job.organization_id,
             bundles.player_profile_id,
             files.storage_uri,
             files.file_kind
         FROM next_job
         LEFT JOIN import.ingest_bundles bundles
           ON bundles.id = next_job.bundle_id
         LEFT JOIN import.source_files files
           ON files.id = next_job.source_file_id",
        &[],
    )? else {
        return Ok(None);
    };

    let job_id: Uuid = row.get(0);
    let bundle_id: Uuid = row.get(1);
    let bundle_file_id: Option<Uuid> = row.get(2);
    let source_file_id: Option<Uuid> = row.get(3);
    let job_kind = JobKind::from_db(&row.get::<_, String>(4));
    let organization_id: Uuid = row.get(5);
    let player_profile_id: Uuid = row.get(6);
    let storage_uri: Option<String> = row.get(7);
    let file_kind = row
        .get::<_, Option<String>>(8)
        .map(|value| match value.as_str() {
            "hh" => FileKind::HandHistory,
            "ts" => FileKind::TournamentSummary,
            other => panic!("unexpected file kind: {other}"),
        });
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

    refresh_bundle_status(client, bundle_id)?;

    Ok(Some(ClaimedJob {
        job_id,
        bundle_id,
        bundle_file_id,
        source_file_id,
        job_kind,
        organization_id,
        player_profile_id,
        storage_uri,
        file_kind,
        attempt_no,
    }))
}

pub fn mark_job_succeeded(
    client: &mut impl GenericClient,
    job_id: Uuid,
    attempt_no: i32,
) -> Result<()> {
    let bundle_id: Uuid = client
        .query_one(
            "SELECT bundle_id
             FROM import.import_jobs
             WHERE id = $1",
            &[&job_id],
        )?
        .get(0);

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
    Ok(())
}

pub fn mark_job_failed(
    client: &mut impl GenericClient,
    job_id: Uuid,
    attempt_no: i32,
    disposition: FailureDisposition,
    error_code: &str,
) -> Result<()> {
    let bundle_id: Uuid = client
        .query_one(
            "SELECT bundle_id
             FROM import.import_jobs
             WHERE id = $1",
            &[&job_id],
        )?
        .get(0);

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

    refresh_bundle_status(client, bundle_id)?;
    Ok(())
}

pub fn retry_failed_job(
    client: &mut impl GenericClient,
    job_id: Uuid,
    max_attempts: i32,
) -> Result<()> {
    let row = client.query_one(
        "SELECT bundle_id, status
         FROM import.import_jobs
         WHERE id = $1",
        &[&job_id],
    )?;
    let bundle_id: Uuid = row.get(0);
    let status: String = row.get(1);

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

    if attempt_count >= i64::from(max_attempts) {
        client.execute(
            "UPDATE import.import_jobs
             SET status = 'failed_terminal'
             WHERE id = $1",
            &[&job_id],
        )?;
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
    }

    refresh_bundle_status(client, bundle_id)?;
    Ok(())
}

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

    Ok(Some(job_id))
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

    let execution_result = match job.job_kind {
        JobKind::FileIngest => executor.execute_file_job(client, &job),
        JobKind::BundleFinalize => executor.finalize_bundle(client, &job),
    };

    match execution_result {
        Ok(()) => {
            mark_job_succeeded(client, job.job_id, job.attempt_no)?;
            if matches!(job.job_kind, JobKind::FileIngest) {
                let _ = maybe_enqueue_finalize_job(client, job.bundle_id)?;
            }
        }
        Err(error) => {
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

fn refresh_bundle_status(client: &mut impl GenericClient, bundle_id: Uuid) -> Result<BundleStatus> {
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

    Ok(status)
}

fn derive_bundle_status_from_counts(
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
    } else if running_count > 0 {
        BundleStatus::Running
    } else if failed_retriable_count > 0 {
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
