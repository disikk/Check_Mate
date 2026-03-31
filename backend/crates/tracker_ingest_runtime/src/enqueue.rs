// Enqueue bundle: создание bundle, source_files, members, jobs в БД.
// Перенесено из lib.rs как часть механического рефакторинга.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use postgres::GenericClient;
use uuid::Uuid;

use crate::events::{append_ingest_event, emit_bundle_event};
use crate::models::*;

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
