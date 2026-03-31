use axum::extract::ws::{Message, WebSocket};
use tokio::time::sleep;
use tracker_ingest_runtime::{
    BundleFileSnapshot, BundleFileDiagnostic, BundleSnapshot, BundleStatus, FileJobStatus,
    PersistedIngestEvent, load_bundle_events_since,
};
use uuid::Uuid;

use crate::AppState;
use crate::dto::{
    ApiActivityLogEntry, ApiBundleFile, ApiBundleFileDiagnostic, ApiBundleSnapshot,
    WsServerMessage,
};
use crate::errors::ApiError;
use crate::routes::session::run_db_api;

/// Drive the WebSocket event stream after the initial bootstrap snapshot.
pub(crate) async fn stream_bundle_events(
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

/// Map a runtime `BundleSnapshot` into the API DTO.
pub(crate) fn map_bundle_snapshot(snapshot: BundleSnapshot) -> ApiBundleSnapshot {
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
