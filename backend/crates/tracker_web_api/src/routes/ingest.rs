use axum::{
    Json,
    extract::{Multipart, Path as AxumPath, State, WebSocketUpgrade},
    response::Response,
};
use tokio::fs;
use tracker_ingest_runtime::{
    IngestBundleInput, enqueue_bundle, load_bundle_snapshot,
};
use uuid::Uuid;

use crate::AppState;
use crate::dto::{ApiBundleSnapshot, CreateBundleResponse};
use crate::errors::ApiError;
use crate::routes::dashboard::ensure_bundle_access;
use crate::routes::session::{load_session_context, run_db_api};
use crate::upload_spool::{build_upload_inputs, store_upload_field};
use crate::ws::stream_bundle_events;

/// POST /api/ingest/bundles — spool uploaded files and enqueue a bundle.
pub(crate) async fn create_ingest_bundle(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<CreateBundleResponse>, ApiError> {
    let session = load_session_context(&state).await?;
    fs::create_dir_all(&state.config.spool_dir)
        .await
        .map_err(ApiError::internal)?;

    let upload_root = state
        .config
        .spool_dir
        .join(format!("batch-{}", Uuid::new_v4()));
    fs::create_dir_all(&upload_root)
        .await
        .map_err(ApiError::internal)?;

    let mut uploads = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(ApiError::internal)? {
        uploads.push(store_upload_field(&upload_root, field).await?);
    }

    if uploads.is_empty() {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "multipart upload must contain at least one file",
        ));
    }

    let files = build_upload_inputs(&upload_root, &uploads).await?;

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
            snapshot: crate::ws::map_bundle_snapshot(snapshot),
        })
    })
    .await?;

    Ok(Json(response))
}

/// GET /api/ingest/bundles/{bundle_id} — current bundle snapshot.
pub(crate) async fn get_bundle_snapshot_handler(
    State(state): State<AppState>,
    AxumPath(bundle_id): AxumPath<Uuid>,
) -> Result<Json<ApiBundleSnapshot>, ApiError> {
    let session = load_session_context(&state).await?;

    let snapshot = run_db_api(&state, move |client| {
        ensure_bundle_access(client, bundle_id, &session)?;
        load_bundle_snapshot(client, bundle_id)
            .map(crate::ws::map_bundle_snapshot)
            .map_err(ApiError::internal)
    })
    .await?;

    Ok(Json(snapshot))
}

/// GET /api/ingest/bundles/{bundle_id}/ws — upgrade to WebSocket event stream.
pub(crate) async fn bundle_events_ws(
    State(state): State<AppState>,
    AxumPath(bundle_id): AxumPath<Uuid>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let session = load_session_context(&state).await?;
    let bootstrap = run_db_api(&state, move |client| {
        ensure_bundle_access(client, bundle_id, &session)?;
        let snapshot = load_bundle_snapshot(client, bundle_id).map_err(ApiError::internal)?;
        let last_sequence_no =
            tracker_ingest_runtime::load_bundle_events_since(client, bundle_id, None)
                .map_err(ApiError::internal)?
                .last()
                .map(|event| event.sequence_no);

        Ok((crate::ws::map_bundle_snapshot(snapshot), last_sequence_no))
    })
    .await?;

    Ok(ws.on_upgrade(move |socket| stream_bundle_events(socket, state, bundle_id, bootstrap)))
}
