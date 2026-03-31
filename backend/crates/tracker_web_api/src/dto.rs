use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// Stub session context resolved from DB — crate-internal.
#[derive(Debug, Clone)]
pub(crate) struct SessionContext {
    pub(crate) user_id: Uuid,
    pub(crate) user_email: String,
    pub(crate) organization_id: Uuid,
    pub(crate) organization_name: String,
    pub(crate) player_profile_id: Uuid,
    pub(crate) player_screen_name: String,
}

/// GET /api/session response.
#[derive(Debug, Clone, Serialize)]
pub struct SessionResponse {
    pub user_id: Uuid,
    pub user_email: String,
    pub organization_id: Uuid,
    pub organization_name: String,
    pub player_profile_id: Uuid,
    pub player_screen_name: String,
}

impl From<SessionContext> for SessionResponse {
    fn from(value: SessionContext) -> Self {
        Self {
            user_id: value.user_id,
            user_email: value.user_email,
            organization_id: value.organization_id,
            organization_name: value.organization_name,
            player_profile_id: value.player_profile_id,
            player_screen_name: value.player_screen_name,
        }
    }
}

/// Single diagnostic attached to a bundle file.
#[derive(Debug, Clone, Serialize)]
pub struct ApiBundleFileDiagnostic {
    pub code: Option<String>,
    pub message: String,
    pub member_path: Option<String>,
}

/// Per-file status inside a bundle snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct ApiBundleFile {
    pub bundle_file_id: Uuid,
    pub source_file_id: Uuid,
    pub source_file_member_id: Uuid,
    pub member_path: String,
    pub status: String,
    pub stage_label: String,
    pub progress_percent: i32,
    pub diagnostics: Vec<ApiBundleFileDiagnostic>,
}

/// Single entry in the activity log.
#[derive(Debug, Clone, Serialize)]
pub struct ApiActivityLogEntry {
    pub sequence_no: i64,
    pub event_kind: String,
    pub message: String,
    pub payload: JsonValue,
}

/// Full bundle snapshot returned by HTTP and WebSocket bootstrap.
#[derive(Debug, Clone, Serialize)]
pub struct ApiBundleSnapshot {
    pub bundle_id: Uuid,
    pub status: String,
    pub progress_percent: i32,
    pub stage_label: String,
    pub total_files: i64,
    pub completed_files: i64,
    pub files: Vec<ApiBundleFile>,
    pub activity_log: Vec<ApiActivityLogEntry>,
}

/// POST /api/ingest/bundles response.
#[derive(Debug, Clone, Serialize)]
pub struct CreateBundleResponse {
    pub bundle_id: Uuid,
    pub snapshot: ApiBundleSnapshot,
}

/// Query parameters for GET /api/ft/dashboard.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FtDashboardQuery {
    pub(crate) buyin: Option<String>,
    pub(crate) bundle_id: Option<Uuid>,
    pub(crate) date_from: Option<String>,
    pub(crate) date_to: Option<String>,
    pub(crate) timezone: Option<String>,
}

/// Tagged envelope for WebSocket server-to-client messages.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum WsServerMessage {
    BundleSnapshot(ApiBundleSnapshot),
    BundleUpdated(ApiActivityLogEntry),
    FileUpdated(ApiActivityLogEntry),
    DiagnosticLogged(ApiActivityLogEntry),
    BundleTerminal(ApiActivityLogEntry),
}
