use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use postgres::GenericClient;
use serde_json::{Value as JsonValue, json};
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

    fn from_db(value: &str) -> Self {
        match value {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "failed_retriable" => Self::FailedRetriable,
            "failed_terminal" => Self::FailedTerminal,
            other => panic!("unexpected file job status: {other}"),
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
    Archive,
}

impl FileKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::HandHistory => "hh",
            Self::TournamentSummary => "ts",
            Self::Archive => "archive",
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "hh" => Self::HandHistory,
            "ts" => Self::TournamentSummary,
            "archive" => Self::Archive,
            other => panic!("unexpected file kind: {other}"),
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
    pub members: Vec<IngestMemberInput>,
    pub diagnostics: Vec<IngestDiagnosticInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestMemberInput {
    pub member_path: String,
    pub member_kind: FileKind,
    pub sha256: String,
    pub byte_size: i64,
    pub depends_on_member_index: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestDiagnosticInput {
    pub code: String,
    pub message: String,
    pub member_path: Option<String>,
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
    pub source_file_member_id: Uuid,
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
pub struct BundleFileDiagnostic {
    pub code: Option<String>,
    pub message: String,
    pub member_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleFileSnapshot {
    pub bundle_file_id: Uuid,
    pub source_file_id: Uuid,
    pub source_file_member_id: Uuid,
    pub member_path: String,
    pub status: FileJobStatus,
    pub stage_label: String,
    pub progress_percent: i32,
    pub diagnostics: Vec<BundleFileDiagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersistedIngestEvent {
    pub sequence_no: i64,
    pub event_kind: String,
    pub message: String,
    pub payload: JsonValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleSnapshot {
    pub bundle_id: Uuid,
    pub status: BundleStatus,
    pub progress_percent: i32,
    pub stage_label: String,
    pub total_files: i64,
    pub completed_files: i64,
    pub files: Vec<BundleFileSnapshot>,
    pub activity_log: Vec<PersistedIngestEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedJob {
    pub job_id: Uuid,
    pub bundle_id: Uuid,
    pub bundle_file_id: Option<Uuid>,
    pub source_file_id: Option<Uuid>,
    pub source_file_member_id: Option<Uuid>,
    pub job_kind: JobKind,
    pub organization_id: Uuid,
    pub player_profile_id: Uuid,
    pub storage_uri: Option<String>,
    pub source_file_kind: Option<FileKind>,
    pub member_path: Option<String>,
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

    let mut file_jobs = Vec::new();
    let mut next_file_order_index: i32 = 0;
    for file in &input.files {
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

        let executable_members = if matches!(file.file_kind, FileKind::Archive) {
            file.members.clone()
        } else {
            vec![IngestMemberInput {
                member_path: file.original_filename.clone(),
                member_kind: file.file_kind,
                sha256: file.sha256.clone(),
                byte_size: file.byte_size,
                depends_on_member_index: None,
            }]
        };
        let mut job_id_by_member_index = BTreeMap::<i32, Uuid>::new();

        for diagnostic in &file.diagnostics {
            append_ingest_event(
                client,
                bundle_id,
                None,
                "diagnostic_logged",
                &diagnostic.message,
                &serde_json::json!({
                    "code": diagnostic.code,
                    "member_path": diagnostic.member_path,
                }),
            )?;
        }

        for (member_index, member) in executable_members.iter().enumerate() {
            let member_index = member_index as i32;
            let source_file_member_id = upsert_source_file_member(
                client,
                source_file_id,
                member_index,
                &member.member_path,
                member.member_kind,
                &member.sha256,
                member.byte_size,
            )?;
            let depends_on_job_id = match member.depends_on_member_index {
                Some(depends_on_member_index) => Some(
                    *job_id_by_member_index
                        .get(&depends_on_member_index)
                        .ok_or_else(|| {
                            anyhow!(
                                "member `{}` depends on missing earlier member index {}",
                                member.member_path,
                                depends_on_member_index
                            )
                        })?,
                ),
                None => None,
            };

            let bundle_file_id: Uuid = client
                .query_one(
                    "INSERT INTO import.ingest_bundle_files (
                        bundle_id,
                        source_file_id,
                        source_file_member_id,
                        file_order_index
                    )
                    VALUES ($1, $2, $3, $4)
                    RETURNING id",
                    &[
                        &bundle_id,
                        &source_file_id,
                        &source_file_member_id,
                        &next_file_order_index,
                    ],
                )?
                .get(0);

            let job_id: Uuid = client
                .query_one(
                    "INSERT INTO import.import_jobs (
                        organization_id,
                        bundle_id,
                        bundle_file_id,
                        source_file_id,
                        source_file_member_id,
                        depends_on_job_id,
                        job_kind,
                        status,
                        stage
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, 'file_ingest', $7, 'queued')
                    RETURNING id",
                    &[
                        &input.organization_id,
                        &bundle_id,
                        &bundle_file_id,
                        &source_file_id,
                        &source_file_member_id,
                        &depends_on_job_id,
                        &FileJobStatus::Queued.as_str(),
                    ],
                )?
                .get(0);
            job_id_by_member_index.insert(member_index, job_id);

            file_jobs.push(EnqueuedFileJob {
                bundle_file_id,
                source_file_id,
                source_file_member_id,
                job_id,
            });

            next_file_order_index += 1;
        }
    }

    let bundle = EnqueuedBundle {
        bundle_id,
        file_jobs,
    };

    emit_bundle_event(
        client,
        bundle_id,
        "bundle_updated",
        "Партия файлов поставлена в очередь.",
    )?;

    Ok(bundle)
}

fn upsert_source_file_member(
    client: &mut impl GenericClient,
    source_file_id: Uuid,
    member_index: i32,
    member_path: &str,
    member_kind: FileKind,
    sha256: &str,
    byte_size: i64,
) -> Result<Uuid> {
    Ok(client
        .query_one(
            "INSERT INTO import.source_file_members (
                source_file_id,
                member_index,
                member_path,
                member_kind,
                sha256,
                byte_size
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (source_file_id, member_index)
            DO UPDATE SET
                member_path = EXCLUDED.member_path,
                member_kind = EXCLUDED.member_kind,
                sha256 = EXCLUDED.sha256,
                byte_size = EXCLUDED.byte_size
            RETURNING id",
            &[
                &source_file_id,
                &member_index,
                &member_path,
                &member_kind.as_str(),
                &sha256,
                &byte_size,
            ],
        )?
        .get(0))
}

fn append_ingest_event(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    bundle_file_id: Option<Uuid>,
    event_kind: &str,
    message: &str,
    payload: &serde_json::Value,
) -> Result<()> {
    client.execute(
        "INSERT INTO import.ingest_events (
            bundle_id,
            bundle_file_id,
            event_kind,
            message,
            payload
        )
        VALUES ($1, $2, $3, $4, ($5::text)::jsonb)",
        &[
            &bundle_id,
            &bundle_file_id,
            &event_kind,
            &message,
            &payload.to_string(),
        ],
    )?;

    Ok(())
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

fn parse_json_payload_text(payload: &str) -> JsonValue {
    serde_json::from_str(payload).unwrap_or_else(|_| JsonValue::Object(Default::default()))
}

fn file_stage_label(status: FileJobStatus, stage: &str) -> &'static str {
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

fn file_progress_percent(status: FileJobStatus, stage: &str) -> i32 {
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

fn load_bundle_snapshot_readonly(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
) -> Result<BundleSnapshot> {
    load_bundle_snapshot_inner(client, bundle_id, false)
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

fn emit_bundle_event(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    event_kind: &str,
    message: &str,
) -> Result<()> {
    let snapshot = load_bundle_snapshot_readonly(client, bundle_id)?;

    append_ingest_event(
        client,
        bundle_id,
        None,
        event_kind,
        message,
        &json!({
            "bundle_id": snapshot.bundle_id,
            "status": snapshot.status.as_str(),
            "progress_percent": snapshot.progress_percent,
            "stage_label": snapshot.stage_label,
            "total_files": snapshot.total_files,
            "completed_files": snapshot.completed_files,
        }),
    )
}

fn emit_file_updated_event(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    bundle_file_id: Uuid,
    message: &str,
) -> Result<()> {
    let snapshot = load_bundle_snapshot_readonly(client, bundle_id)?;
    let file = snapshot
        .files
        .iter()
        .find(|file| file.bundle_file_id == bundle_file_id)
        .expect("bundle file snapshot must exist for file_updated event");

    append_ingest_event(
        client,
        bundle_id,
        Some(bundle_file_id),
        "file_updated",
        message,
        &json!({
            "bundle_file_id": file.bundle_file_id,
            "source_file_id": file.source_file_id,
            "source_file_member_id": file.source_file_member_id,
            "member_path": file.member_path,
            "status": file.status.as_str(),
            "stage_label": file.stage_label,
            "progress_percent": file.progress_percent,
        }),
    )
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

fn emit_dependency_failed_events(
    client: &mut impl GenericClient,
    propagated_bundle_files: &[(Uuid, Uuid)],
) -> Result<()> {
    for (bundle_id, bundle_file_id) in propagated_bundle_files {
        let member_path = client
            .query_one(
                "SELECT members.member_path
                 FROM import.ingest_bundle_files AS bundle_files
                 INNER JOIN import.source_file_members AS members
                    ON members.id = bundle_files.source_file_member_id
                 WHERE bundle_files.id = $1",
                &[bundle_file_id],
            )?
            .get::<_, String>(0);

        append_ingest_event(
            client,
            *bundle_id,
            Some(*bundle_file_id),
            "diagnostic_logged",
            &format!("Файл `{member_path}` пропущен из-за ошибки зависимого TS-job."),
            &json!({
                "code": "dependency_failed",
                "member_path": member_path,
            }),
        )?;
        emit_file_updated_event(
            client,
            *bundle_id,
            *bundle_file_id,
            &format!("Файл `{member_path}` пропущен из-за ошибки зависимости."),
        )?;
    }

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

    emit_bundle_event(
        client,
        bundle_id,
        "bundle_updated",
        "Подготовка индекса запущена.",
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

fn refresh_bundle_status(client: &mut impl GenericClient, bundle_id: Uuid) -> Result<BundleStatus> {
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
