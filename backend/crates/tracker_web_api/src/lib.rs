use std::{
    io::{Cursor, Read},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{
        Multipart, Path as AxumPath, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use mbr_stats_runtime::{FtDashboardFilters, query_ft_dashboard};
use postgres::{Client, GenericClient, NoTls};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use sha2::{Digest, Sha256};
use tokio::{fs, net::TcpListener, task, time::sleep};
use tracker_ingest_runtime::{
    BundleFileDiagnostic, BundleFileSnapshot, BundleSnapshot, BundleStatus, FileJobStatus,
    FileKind, IngestBundleInput, IngestDiagnosticInput, IngestFileInput, IngestMemberInput,
    PersistedIngestEvent, enqueue_bundle, load_bundle_events_since, load_bundle_snapshot,
};
use tracker_parser_core::{SourceKind, detect_source_kind};
use uuid::Uuid;
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct StubSessionSeed {
    pub organization_name: String,
    pub user_email: String,
    pub player_screen_name: String,
}

impl Default for StubSessionSeed {
    fn default() -> Self {
        Self {
            organization_name: "Check Mate Web Org".to_string(),
            user_email: "web-stub@example.com".to_string(),
            player_screen_name: "Hero".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WebApiConfig {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub spool_dir: PathBuf,
    pub session_seed: StubSessionSeed,
    pub ws_poll_interval: Duration,
}

impl WebApiConfig {
    pub fn from_env() -> Result<Self> {
        let bind_addr = std::env::var("CHECK_MATE_WEB_API_BIND")
            .unwrap_or_else(|_| "127.0.0.1:3001".to_string())
            .parse()
            .context("failed to parse CHECK_MATE_WEB_API_BIND")?;

        Ok(Self {
            bind_addr,
            database_url: std::env::var("CHECK_MATE_DATABASE_URL")
                .context("CHECK_MATE_DATABASE_URL is required for tracker_web_api")?,
            spool_dir: std::env::var("CHECK_MATE_WEB_SPOOL_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(".local/upload_spool")),
            session_seed: StubSessionSeed {
                organization_name: std::env::var("CHECK_MATE_WEB_ORG_NAME")
                    .unwrap_or_else(|_| StubSessionSeed::default().organization_name),
                user_email: std::env::var("CHECK_MATE_WEB_USER_EMAIL")
                    .unwrap_or_else(|_| StubSessionSeed::default().user_email),
                player_screen_name: std::env::var("CHECK_MATE_WEB_PLAYER_NAME")
                    .unwrap_or_else(|_| StubSessionSeed::default().player_screen_name),
            },
            ws_poll_interval: Duration::from_millis(
                std::env::var("CHECK_MATE_WEB_WS_POLL_MS")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(250),
            ),
        })
    }
}

#[derive(Clone)]
struct AppState {
    config: Arc<WebApiConfig>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionResponse {
    pub user_id: Uuid,
    pub user_email: String,
    pub organization_id: Uuid,
    pub organization_name: String,
    pub player_profile_id: Uuid,
    pub player_screen_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiBundleFileDiagnostic {
    pub code: Option<String>,
    pub message: String,
    pub member_path: Option<String>,
}

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

#[derive(Debug, Clone, Serialize)]
pub struct ApiActivityLogEntry {
    pub sequence_no: i64,
    pub event_kind: String,
    pub message: String,
    pub payload: JsonValue,
}

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

#[derive(Debug, Clone, Serialize)]
pub struct CreateBundleResponse {
    pub bundle_id: Uuid,
    pub snapshot: ApiBundleSnapshot,
}

#[derive(Debug, Clone, Deserialize)]
struct FtDashboardQuery {
    buyin: Option<String>,
    bundle_id: Option<Uuid>,
    date_from: Option<String>,
    date_to: Option<String>,
    timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum WsServerMessage {
    BundleSnapshot(ApiBundleSnapshot),
    BundleUpdated(ApiActivityLogEntry),
    FileUpdated(ApiActivityLogEntry),
    DiagnosticLogged(ApiActivityLogEntry),
    BundleTerminal(ApiActivityLogEntry),
}

#[derive(Debug, Clone)]
struct SessionContext {
    user_id: Uuid,
    user_email: String,
    organization_id: Uuid,
    organization_name: String,
    player_profile_id: Uuid,
    player_screen_name: String,
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

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.message,
            })),
        )
            .into_response()
    }
}

pub fn build_app(config: WebApiConfig) -> Router {
    let state = AppState {
        config: Arc::new(config),
    };

    Router::new()
        .route("/api/session", get(get_session))
        .route("/api/ingest/bundles", post(create_ingest_bundle))
        .route("/api/ft/dashboard", get(get_ft_dashboard))
        .route(
            "/api/ingest/bundles/{bundle_id}",
            get(get_bundle_snapshot_handler),
        )
        .route("/api/ingest/bundles/{bundle_id}/ws", get(bundle_events_ws))
        .with_state(state)
}

pub async fn serve(listener: TcpListener, config: WebApiConfig) -> Result<()> {
    axum::serve(listener, build_app(config))
        .await
        .context("tracker_web_api server failed")
}

async fn get_session(State(state): State<AppState>) -> Result<Json<SessionResponse>, ApiError> {
    let session = load_session_context(&state).await?;
    Ok(Json(session.into()))
}

async fn create_ingest_bundle(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<CreateBundleResponse>, ApiError> {
    let session = load_session_context(&state).await?;
    fs::create_dir_all(&state.config.spool_dir)
        .await
        .map_err(ApiError::internal)?;

    let mut files = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(ApiError::internal)? {
        files.push(store_and_classify_upload_field(&state.config.spool_dir, field).await?);
    }

    if files.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "multipart upload must contain at least one file",
        ));
    }

    let input = IngestBundleInput {
        organization_id: session.organization_id,
        player_profile_id: session.player_profile_id,
        created_by_user_id: session.user_id,
        files,
    };

    let response = run_db_api(&state, move |client| {
        let mut tx = client.transaction().map_err(ApiError::internal)?;
        let bundle = enqueue_bundle(&mut tx, &input).map_err(ApiError::internal)?;
        let snapshot =
            load_bundle_snapshot(&mut tx, bundle.bundle_id).map_err(ApiError::internal)?;
        tx.commit().map_err(ApiError::internal)?;

        Ok(CreateBundleResponse {
            bundle_id: bundle.bundle_id,
            snapshot: map_bundle_snapshot(snapshot),
        })
    })
    .await?;

    Ok(Json(response))
}

async fn get_ft_dashboard(
    State(state): State<AppState>,
    Query(query): Query<FtDashboardQuery>,
) -> Result<Json<mbr_stats_runtime::FtDashboardSnapshot>, ApiError> {
    let session = load_session_context(&state).await?;
    let timezone_name = query.timezone.ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "timezone query parameter is required",
        )
    })?;
    let buyin_total_cents = parse_buyin_filter(query.buyin.as_deref())?;
    validate_local_datetime_range(query.date_from.as_deref(), query.date_to.as_deref())?;

    let snapshot = run_db_api(&state, move |client| {
        validate_timezone_name(client, &timezone_name)?;
        if let Some(bundle_id) = query.bundle_id {
            ensure_bundle_access(client, bundle_id, &session)?;
        }
        validate_local_datetime(client, query.date_from.as_deref(), "date_from")?;
        validate_local_datetime(client, query.date_to.as_deref(), "date_to")?;

        query_ft_dashboard(
            client,
            FtDashboardFilters {
                organization_id: session.organization_id,
                player_profile_id: session.player_profile_id,
                buyin_total_cents,
                bundle_id: query.bundle_id,
                date_from_local: query.date_from,
                date_to_local: query.date_to,
                timezone_name,
            },
        )
        .map_err(ApiError::internal)
    })
    .await?;

    Ok(Json(snapshot))
}

async fn get_bundle_snapshot_handler(
    State(state): State<AppState>,
    AxumPath(bundle_id): AxumPath<Uuid>,
) -> Result<Json<ApiBundleSnapshot>, ApiError> {
    let session = load_session_context(&state).await?;

    let snapshot = run_db_api(&state, move |client| {
        ensure_bundle_access(client, bundle_id, &session)?;
        load_bundle_snapshot(client, bundle_id)
            .map(map_bundle_snapshot)
            .map_err(ApiError::internal)
    })
    .await?;

    Ok(Json(snapshot))
}

async fn bundle_events_ws(
    State(state): State<AppState>,
    AxumPath(bundle_id): AxumPath<Uuid>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let session = load_session_context(&state).await?;
    let bootstrap = run_db_api(&state, move |client| {
        ensure_bundle_access(client, bundle_id, &session)?;
        let snapshot = load_bundle_snapshot(client, bundle_id).map_err(ApiError::internal)?;
        let last_sequence_no = load_bundle_events_since(client, bundle_id, None)
            .map_err(ApiError::internal)?
            .last()
            .map(|event| event.sequence_no);

        Ok((map_bundle_snapshot(snapshot), last_sequence_no))
    })
    .await?;

    Ok(ws.on_upgrade(move |socket| stream_bundle_events(socket, state, bundle_id, bootstrap)))
}

async fn stream_bundle_events(
    mut socket: WebSocket,
    state: AppState,
    bundle_id: Uuid,
    bootstrap: (ApiBundleSnapshot, Option<i64>),
) {
    let (snapshot, mut cursor) = bootstrap;
    if send_ws_message(&mut socket, &WsServerMessage::BundleSnapshot(snapshot))
        .await
        .is_err()
    {
        return;
    }

    loop {
        sleep(state.config.ws_poll_interval).await;

        let result = run_db_api(&state, move |client| {
            load_bundle_events_since(client, bundle_id, cursor).map_err(ApiError::internal)
        })
        .await;

        let events = match result {
            Ok(events) => events,
            Err(_) => return,
        };

        for event in events {
            cursor = Some(event.sequence_no);
            let message = map_ws_message(event);
            let is_terminal = matches!(message, WsServerMessage::BundleTerminal(_));
            if send_ws_message(&mut socket, &message).await.is_err() {
                return;
            }
            if is_terminal {
                return;
            }
        }
    }
}

async fn send_ws_message(socket: &mut WebSocket, message: &WsServerMessage) -> Result<(), ()> {
    let payload = serde_json::to_string(message).map_err(|_| ())?;
    socket
        .send(Message::Text(payload.into()))
        .await
        .map_err(|_| ())
}

fn map_ws_message(event: PersistedIngestEvent) -> WsServerMessage {
    let entry = map_activity_entry(event.clone());

    match event.event_kind.as_str() {
        "bundle_updated" => WsServerMessage::BundleUpdated(entry),
        "file_updated" => WsServerMessage::FileUpdated(entry),
        "diagnostic_logged" => WsServerMessage::DiagnosticLogged(entry),
        "bundle_terminal" => WsServerMessage::BundleTerminal(entry),
        other => WsServerMessage::DiagnosticLogged(ApiActivityLogEntry {
            event_kind: other.to_string(),
            ..entry
        }),
    }
}

fn map_bundle_snapshot(snapshot: BundleSnapshot) -> ApiBundleSnapshot {
    ApiBundleSnapshot {
        bundle_id: snapshot.bundle_id,
        status: bundle_status_code(snapshot.status).to_string(),
        progress_percent: snapshot.progress_percent,
        stage_label: snapshot.stage_label,
        total_files: snapshot.total_files,
        completed_files: snapshot.completed_files,
        files: snapshot.files.into_iter().map(map_bundle_file).collect(),
        activity_log: snapshot
            .activity_log
            .into_iter()
            .map(map_activity_entry)
            .collect(),
    }
}

fn map_bundle_file(file: BundleFileSnapshot) -> ApiBundleFile {
    ApiBundleFile {
        bundle_file_id: file.bundle_file_id,
        source_file_id: file.source_file_id,
        source_file_member_id: file.source_file_member_id,
        member_path: file.member_path,
        status: file_status_code(file.status).to_string(),
        stage_label: file.stage_label,
        progress_percent: file.progress_percent,
        diagnostics: file.diagnostics.into_iter().map(map_diagnostic).collect(),
    }
}

fn map_diagnostic(diagnostic: BundleFileDiagnostic) -> ApiBundleFileDiagnostic {
    ApiBundleFileDiagnostic {
        code: diagnostic.code,
        message: diagnostic.message,
        member_path: diagnostic.member_path,
    }
}

fn map_activity_entry(event: PersistedIngestEvent) -> ApiActivityLogEntry {
    ApiActivityLogEntry {
        sequence_no: event.sequence_no,
        event_kind: event.event_kind,
        message: event.message,
        payload: event.payload,
    }
}

fn bundle_status_code(status: BundleStatus) -> &'static str {
    match status {
        BundleStatus::Queued => "queued",
        BundleStatus::Running => "running",
        BundleStatus::Finalizing => "finalizing",
        BundleStatus::Succeeded => "succeeded",
        BundleStatus::PartialSuccess => "partial_success",
        BundleStatus::Failed => "failed",
    }
}

fn file_status_code(status: FileJobStatus) -> &'static str {
    match status {
        FileJobStatus::Queued => "queued",
        FileJobStatus::Running => "running",
        FileJobStatus::Succeeded => "succeeded",
        FileJobStatus::FailedRetriable => "failed_retriable",
        FileJobStatus::FailedTerminal => "failed_terminal",
    }
}

async fn load_session_context(state: &AppState) -> Result<SessionContext, ApiError> {
    let seed = state.config.session_seed.clone();
    run_db_api(state, move |client| {
        let mut tx = client.transaction().map_err(ApiError::internal)?;
        let session = ensure_stub_session_context(&mut tx, &seed).map_err(ApiError::internal)?;
        tx.commit().map_err(ApiError::internal)?;
        Ok(session)
    })
    .await
}

async fn run_db_api<T, F>(state: &AppState, operation: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&mut Client) -> Result<T, ApiError> + Send + 'static,
{
    let database_url = state.config.database_url.clone();

    task::spawn_blocking(move || {
        let mut client = Client::connect(&database_url, NoTls).map_err(ApiError::internal)?;
        operation(&mut client)
    })
    .await
    .map_err(ApiError::internal)?
}

fn ensure_stub_session_context(
    client: &mut impl GenericClient,
    seed: &StubSessionSeed,
) -> Result<SessionContext> {
    let organization_id = if let Some(row) = client.query_opt(
        "SELECT id
         FROM org.organizations
         WHERE name = $1",
        &[&seed.organization_name],
    )? {
        row.get(0)
    } else {
        client
            .query_one(
                "INSERT INTO org.organizations (name)
                 VALUES ($1)
                 RETURNING id",
                &[&seed.organization_name],
            )?
            .get(0)
    };

    let user_id = if let Some(row) = client.query_opt(
        "SELECT id
         FROM auth.users
         WHERE email = $1",
        &[&seed.user_email],
    )? {
        row.get(0)
    } else {
        client
            .query_one(
                "INSERT INTO auth.users (email, auth_provider, status)
                 VALUES ($1, 'stub_web', 'active')
                 RETURNING id",
                &[&seed.user_email],
            )?
            .get(0)
    };

    client.execute(
        "INSERT INTO org.organization_memberships (organization_id, user_id, role)
         VALUES ($1, $2, 'student')
         ON CONFLICT (organization_id, user_id) DO NOTHING",
        &[&organization_id, &user_id],
    )?;

    let player_profile_id = if let Some(row) = client.query_opt(
        "SELECT id
         FROM core.player_profiles
         WHERE organization_id = $1
           AND room = 'gg'
           AND screen_name = $2",
        &[&organization_id, &seed.player_screen_name],
    )? {
        row.get(0)
    } else {
        client
            .query_one(
                "INSERT INTO core.player_profiles (
                    organization_id,
                    owner_user_id,
                    room,
                    network,
                    screen_name
                )
                VALUES ($1, $2, 'gg', 'gg', $3)
                RETURNING id",
                &[&organization_id, &user_id, &seed.player_screen_name],
            )?
            .get(0)
    };

    client.execute(
        "INSERT INTO core.player_aliases (
            organization_id,
            player_profile_id,
            room,
            alias,
            is_primary,
            source
        )
        VALUES ($1, $2, 'gg', $3, TRUE, 'stub_web_session')
        ON CONFLICT (player_profile_id, room, alias)
        DO UPDATE SET
            is_primary = TRUE,
            source = EXCLUDED.source",
        &[
            &organization_id,
            &player_profile_id,
            &seed.player_screen_name,
        ],
    )?;

    Ok(SessionContext {
        user_id,
        user_email: seed.user_email.clone(),
        organization_id,
        organization_name: seed.organization_name.clone(),
        player_profile_id,
        player_screen_name: seed.player_screen_name.clone(),
    })
}

fn ensure_bundle_access(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    session: &SessionContext,
) -> Result<(), ApiError> {
    let Some(row) = client
        .query_opt(
            "SELECT organization_id, player_profile_id
             FROM import.ingest_bundles
             WHERE id = $1",
            &[&bundle_id],
        )
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("bundle `{bundle_id}` was not found"),
        ));
    };

    let organization_id: Uuid = row.get(0);
    let player_profile_id: Uuid = row.get(1);
    if organization_id != session.organization_id || player_profile_id != session.player_profile_id
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "bundle is outside the current session scope",
        ));
    }

    Ok(())
}

fn parse_buyin_filter(buyin: Option<&str>) -> Result<Option<Vec<i64>>, ApiError> {
    match buyin {
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => value
            .parse::<i64>()
            .map(|parsed| Some(vec![parsed]))
            .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "buyin must be an integer amount in cents")),
        None => Ok(None),
    }
}

fn validate_local_datetime_range(
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> Result<(), ApiError> {
    if let (Some(date_from), Some(date_to)) = (date_from, date_to)
        && date_from > date_to
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "date_from must be less than or equal to date_to",
        ));
    }

    Ok(())
}

fn validate_timezone_name(
    client: &mut impl GenericClient,
    timezone_name: &str,
) -> Result<(), ApiError> {
    client
        .query_one("SELECT now() AT TIME ZONE $1", &[&timezone_name])
        .map_err(|_| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("invalid IANA timezone `{timezone_name}`"),
            )
        })?;
    Ok(())
}

fn validate_local_datetime(
    client: &mut impl GenericClient,
    value: Option<&str>,
    field_name: &str,
) -> Result<(), ApiError> {
    let Some(value) = value else {
        return Ok(());
    };

    client
        .query_one(
            "SELECT replace($1, 'T', ' ')::timestamp",
            &[&value],
        )
        .map_err(|_| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("{field_name} must be a valid datetime-local value"),
            )
        })?;
    Ok(())
}

async fn store_and_classify_upload_field(
    spool_dir: &Path,
    field: axum::extract::multipart::Field<'_>,
) -> Result<IngestFileInput, ApiError> {
    let filename = field.file_name().map(ToOwned::to_owned).ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "multipart field is missing filename",
        )
    })?;
    let bytes = field.bytes().await.map_err(ApiError::internal)?;

    if !is_supported_upload_filename(&filename) {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            format!("unsupported upload file `{filename}`"),
        ));
    }

    let spool_path = spool_dir.join(format!(
        "{}-{}",
        Uuid::new_v4(),
        sanitize_filename(&filename)
    ));
    fs::write(&spool_path, &bytes)
        .await
        .map_err(ApiError::internal)?;
    let storage_uri = format!("local://{}", spool_path.display());

    classify_upload_file(&filename, bytes.as_ref(), storage_uri)
}

fn classify_upload_file(
    filename: &str,
    bytes: &[u8],
    storage_uri: String,
) -> Result<IngestFileInput, ApiError> {
    if filename.to_lowercase().ends_with(".zip") {
        classify_archive_upload(filename, bytes, storage_uri)
    } else {
        classify_flat_upload(filename, bytes, storage_uri)
    }
}

fn classify_flat_upload(
    filename: &str,
    bytes: &[u8],
    storage_uri: String,
) -> Result<IngestFileInput, ApiError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("flat upload `{filename}` must be UTF-8 text"),
        )
    })?;
    let file_kind = match detect_source_kind(text).map_err(|error| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("failed to classify `{filename}`: {error}"),
        )
    })? {
        SourceKind::HandHistory => FileKind::HandHistory,
        SourceKind::TournamentSummary => FileKind::TournamentSummary,
    };

    Ok(IngestFileInput {
        room: "gg".to_string(),
        file_kind,
        sha256: sha256_bytes_hex(bytes),
        original_filename: filename.to_string(),
        byte_size: bytes.len() as i64,
        storage_uri,
        members: vec![],
        diagnostics: vec![],
    })
}

fn classify_archive_upload(
    filename: &str,
    bytes: &[u8],
    storage_uri: String,
) -> Result<IngestFileInput, ApiError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    let mut members = Vec::new();
    let mut diagnostics = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
        if entry.is_dir() {
            continue;
        }

        let member_path = entry.name().to_string();
        let mut member_bytes = Vec::new();
        entry
            .read_to_end(&mut member_bytes)
            .map_err(|error| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;

        let Ok(text) = std::str::from_utf8(&member_bytes) else {
            diagnostics.push(IngestDiagnosticInput {
                code: "unsupported_archive_member".to_string(),
                message: format!("Skipping unsupported ZIP member `{member_path}`"),
                member_path: Some(member_path),
            });
            continue;
        };

        let detected_kind = match detect_source_kind(text) {
            Ok(SourceKind::HandHistory) => Some(FileKind::HandHistory),
            Ok(SourceKind::TournamentSummary) => Some(FileKind::TournamentSummary),
            Err(_) => None,
        };

        if let Some(member_kind) = detected_kind {
            members.push(IngestMemberInput {
                member_path,
                member_kind,
                sha256: sha256_bytes_hex(&member_bytes),
                byte_size: member_bytes.len() as i64,
            });
        } else {
            diagnostics.push(IngestDiagnosticInput {
                code: "unsupported_archive_member".to_string(),
                message: format!("Skipping unsupported ZIP member `{member_path}`"),
                member_path: Some(member_path),
            });
        }
    }

    Ok(IngestFileInput {
        room: "gg".to_string(),
        file_kind: FileKind::Archive,
        sha256: sha256_bytes_hex(bytes),
        original_filename: filename.to_string(),
        byte_size: bytes.len() as i64,
        storage_uri,
        members,
        diagnostics,
    })
}

fn is_supported_upload_filename(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".txt") || lower.ends_with(".hh") || lower.ends_with(".zip")
}

fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '_',
            _ => ch,
        })
        .collect()
}

fn sha256_bytes_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
