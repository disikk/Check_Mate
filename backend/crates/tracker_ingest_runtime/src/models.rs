// Все struct/enum модели ingest runtime.
// Перенесено из lib.rs как часть механического рефакторинга.

use serde_json::Value as JsonValue;
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
    pub(crate) fn as_str(self) -> &'static str {
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
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::FailedRetriable => "failed_retriable",
            Self::FailedTerminal => "failed_terminal",
        }
    }

    pub(crate) fn from_db(value: &str) -> Self {
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
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::HandHistory => "hh",
            Self::TournamentSummary => "ts",
            Self::Archive => "archive",
        }
    }

    pub(crate) fn from_db(value: &str) -> Self {
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

// Внутренний struct для count-based bundle progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BundleProgressSummary {
    pub(crate) bundle_id: Uuid,
    pub(crate) status: BundleStatus,
    pub(crate) total_files: i64,
    pub(crate) completed_files: i64,
}

// Внутренний struct для single-file job status lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileJobStatusSummary {
    pub(crate) bundle_file_id: Uuid,
    pub(crate) source_file_id: Uuid,
    pub(crate) source_file_member_id: Uuid,
    pub(crate) member_path: String,
    pub(crate) status: FileJobStatus,
    pub(crate) stage: String,
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
    pub(crate) fn from_db(value: &str) -> Self {
        match value {
            "file_ingest" => Self::FileIngest,
            "bundle_finalize" => Self::BundleFinalize,
            other => panic!("unexpected job kind: {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobExecutionError {
    pub(crate) disposition: FailureDisposition,
    pub(crate) error_code: String,
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

    pub fn disposition(&self) -> FailureDisposition {
        self.disposition
    }

    pub fn error_code(&self) -> &str {
        &self.error_code
    }
}
