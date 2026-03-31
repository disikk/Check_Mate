// Внутренние event-helper функции для записи ingest_events.
// Перенесено из lib.rs как часть механического рефакторинга.

use anyhow::Result;
use postgres::GenericClient;
use serde_json::json;
use uuid::Uuid;

use crate::snapshot::{
    bundle_progress_percent_from_summary, bundle_stage_label_from_summary,
    file_progress_percent, file_stage_label, load_bundle_progress_summary,
    load_file_job_status,
};

pub(crate) fn append_ingest_event(
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

pub(crate) fn emit_bundle_event(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    event_kind: &str,
    message: &str,
) -> Result<()> {
    let summary = load_bundle_progress_summary(client, bundle_id)?;

    append_ingest_event(
        client,
        bundle_id,
        None,
        event_kind,
        message,
        &json!({
            "bundle_id": summary.bundle_id,
            "status": summary.status.as_str(),
            "progress_percent": bundle_progress_percent_from_summary(&summary),
            "stage_label": bundle_stage_label_from_summary(&summary),
            "total_files": summary.total_files,
            "completed_files": summary.completed_files,
        }),
    )
}

pub(crate) fn emit_file_updated_event(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    bundle_file_id: Uuid,
    message: &str,
) -> Result<()> {
    let file = load_file_job_status(client, bundle_id, bundle_file_id)?;

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
            "stage_label": file_stage_label(file.status, &file.stage),
            "progress_percent": file_progress_percent(file.status, &file.stage),
        }),
    )
}

pub(crate) fn emit_dependency_failed_events(
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
