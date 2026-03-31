mod archive;
mod batch_sql;
mod compute_rows;
mod context;
mod mbr_domain;
mod persist;
pub(crate) mod profiles;
pub(crate) mod row_models;
mod runner;
mod timezone;
mod util;

#[cfg(test)]
mod tests;

use std::fs;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use postgres::{Client, NoTls};
use tracker_ingest_runtime::{
    BundleStatus as IngestBundleStatus, IngestBundleInput, enqueue_bundle, load_bundle_summary,
};
use tracker_parser_core::EXACT_CORE_RESOLUTION_VERSION;
use uuid::Uuid;

// Items needed only in #[cfg(test)] code within this module and tests.rs.
// The original single-file had all these at the top level; submodule glob
// imports cover internal types, but external crate items still need explicit
// re-exports for tests.rs to see them through `use super::*`.
#[cfg(test)]
use std::{collections::{BTreeMap, BTreeSet}, env};
#[cfg(test)]
use mbr_stats_runtime::materialize_player_hand_features;
#[cfg(test)]
use tracker_ingest_runtime::{
    ClaimedJob as IngestClaimedJob, FailureDisposition,
    FileKind as IngestFileKind,
};
#[cfg(test)]
use tracker_ingest_prepare::{
    PrepareReport, RejectReasonCode, RejectedTournament,
};
#[cfg(test)]
use tracker_parser_core::{
    SourceKind, detect_source_kind,
    models::{ActionType, CanonicalParsedHand, CertaintyState, Street},
    normalizer::normalize_hand,
    parsers::{
        hand_history::{parse_canonical_hand, split_hand_history},
        tournament_summary::parse_tournament_summary,
    },
};

use archive::ArchiveReaderCache;
use context::*;
use runner::*;
use util::*;

// These imports are needed by the #[cfg(test)] functions in this file and in tests.rs
// (import_path_with_database_url, import_tournament_summary, import_hand_history, etc.)
#[cfg(test)]
use compute_rows::*;
#[cfg(test)]
use mbr_domain::*;
#[cfg(test)]
use persist::*;
#[cfg(test)]
use row_models::*;
#[cfg(test)]
use batch_sql::*;
#[cfg(test)]
use archive::load_ingest_job_input;

const HAND_RESOLUTION_VERSION: &str = EXACT_CORE_RESOLUTION_VERSION;
const GG_TIMESTAMP_PROVENANCE_PRESENT: &str = "gg_user_timezone";
const GG_TIMESTAMP_PROVENANCE_MISSING: &str = "gg_user_timezone_missing";
const DEFAULT_RUNNER_WORKER_CAP: usize = 8;

// Re-export all public items that were public in the original module
pub use profiles::{
    ComputeProfile, DirImportReport, IngestE2eProfile, IngestRunProfile, IngestStageProfile,
    LocalImportReport, PrepareProfile, TimezoneUpdateReport,
};

pub use runner::{
    default_runner_worker_count, run_ingest_runner_parallel, run_ingest_runner_until_idle,
    run_ingest_runner_until_idle_with_profile,
};

pub use timezone::{clear_user_timezone, set_user_timezone};

pub fn import_path(path: &str, player_profile_id: Uuid) -> Result<LocalImportReport> {
    let database_url = database_url_from_env()?;
    let input = fs::read_to_string(path).with_context(|| format!("failed to read `{path}`"))?;

    let mut client =
        Client::connect(&database_url, NoTls).context("failed to connect to PostgreSQL")?;
    let mut tx = client
        .transaction()
        .context("failed to start ingest enqueue transaction")?;
    let context = load_import_context(&mut tx, player_profile_id)?;
    let bundle = enqueue_bundle(
        &mut tx,
        &IngestBundleInput {
            organization_id: context.organization_id,
            player_profile_id: context.player_profile_id,
            created_by_user_id: context.user_id,
            files: vec![build_ingest_file_input(path, &input)?],
        },
    )?;
    tx.commit()
        .context("failed to commit ingest enqueue transaction")?;

    let mut executor = LocalImportExecutor {
        report: None,
        run_profile: IngestRunProfile::default(),
        last_finalize_profile: ComputeProfile::default(),
        archive_reader_cache: ArchiveReaderCache::default(),
    };
    loop {
        let claimed = run_next_job_split_tx(&mut client, "parser_worker_local", 3, &mut executor)?;
        let mut tx = client
            .transaction()
            .context("failed to start ingest summary transaction")?;
        let summary = load_bundle_summary(&mut tx, bundle.bundle_id)?;
        tx.commit()
            .context("failed to commit ingest summary transaction")?;

        if matches!(
            summary.status,
            IngestBundleStatus::Succeeded
                | IngestBundleStatus::PartialSuccess
                | IngestBundleStatus::Failed
        ) && !summary.finalize_job_running
        {
            break;
        }

        if claimed.is_none() && !summary.finalize_job_present {
            break;
        }
    }

    let mut report = executor.report.ok_or_else(|| {
        anyhow!("ingest bundle for `{path}` finished without successful file import")
    })?;
    report
        .runtime_profile
        .add_assign(executor.last_finalize_profile);
    report
        .stage_profile
        .add_assign(executor.last_finalize_profile.legacy_stage_profile());
    Ok(report)
}

pub fn dir_import_path(
    path: &str,
    player_profile_id: Uuid,
    worker_count: usize,
) -> Result<DirImportReport> {
    let database_url = database_url_from_env()?;
    dir_import_with_database_url(&database_url, path, player_profile_id, worker_count)
}

fn dir_import_with_database_url(
    database_url: &str,
    path: &str,
    player_profile_id: Uuid,
    worker_count: usize,
) -> Result<DirImportReport> {
    if worker_count == 0 {
        return Err(anyhow!("worker_count must be greater than zero"));
    }

    let e2e_started_at = Instant::now();
    let prepare_report = tracker_ingest_prepare::prepare_path(path)?;
    let rejected_by_reason = summarize_rejected_by_reason(&prepare_report);
    if prepare_report.paired_tournaments.is_empty() {
        let prep_elapsed_ms = e2e_started_at.elapsed().as_millis() as u64;
        let prepare_profile = PrepareProfile {
            scan_ms: prepare_report.scan_ms,
            pair_ms: prepare_report.pair_ms,
            hash_ms: prepare_report.hash_ms,
            enqueue_ms: 0,
        };
        return Ok(DirImportReport {
            prepare_report,
            rejected_by_reason,
            bundle_id: None,
            workers_used: worker_count,
            processed_jobs: 0,
            file_jobs: 0,
            finalize_jobs: 0,
            hands_persisted: 0,
            prep_elapsed_ms,
            runner_elapsed_ms: 0,
            e2e_elapsed_ms: prep_elapsed_ms,
            hands_per_minute: 0.0,
            hands_per_minute_runner: 0.0,
            hands_per_minute_e2e: 0.0,
            e2e_profile: IngestE2eProfile {
                prepare: prepare_profile,
                runtime: ComputeProfile::default(),
                prep_elapsed_ms,
                runner_elapsed_ms: 0,
                e2e_elapsed_ms: prep_elapsed_ms,
            },
            stage_profile: IngestStageProfile::default(),
        });
    }

    let materialize_root =
        std::env::temp_dir().join(format!("check-mate-dir-import-{}", Uuid::new_v4()));
    fs::create_dir_all(&materialize_root).with_context(|| {
        format!(
            "failed to create dir-import temp dir `{}`",
            materialize_root.display()
        )
    })?;

    let result = (|| {
        let enqueue_started_at = Instant::now();
        let materialized = build_prepared_archive_input(&materialize_root, &prepare_report)?
            .ok_or_else(|| {
                anyhow!("prepare report unexpectedly contained no paired tournaments")
            })?;

        let mut client =
            Client::connect(database_url, NoTls).context("failed to connect to PostgreSQL")?;
        let mut tx = client
            .transaction()
            .context("failed to start dir-import enqueue transaction")?;
        let context = load_import_context(&mut tx, player_profile_id)?;
        let bundle = enqueue_bundle(
            &mut tx,
            &IngestBundleInput {
                organization_id: context.organization_id,
                player_profile_id: context.player_profile_id,
                created_by_user_id: context.user_id,
                files: vec![materialized.ingest_file],
            },
        )?;
        tx.commit()
            .context("failed to commit dir-import enqueue transaction")?;
        let enqueue_ms = enqueue_started_at.elapsed().as_millis() as u64;
        let prep_elapsed_ms = e2e_started_at.elapsed().as_millis() as u64;

        let runner_started_at = Instant::now();
        let run_profile =
            run_ingest_runner_parallel(database_url, "parser_worker_dir_import", 3, worker_count)?;
        let runner_elapsed_ms = runner_started_at.elapsed().as_millis() as u64;
        let e2e_elapsed_ms = e2e_started_at.elapsed().as_millis() as u64;
        let hands_per_minute_runner = if runner_elapsed_ms == 0 {
            0.0
        } else {
            (run_profile.hands_persisted as f64) * 60_000.0 / (runner_elapsed_ms as f64)
        };
        let hands_per_minute_e2e = if e2e_elapsed_ms == 0 {
            0.0
        } else {
            (run_profile.hands_persisted as f64) * 60_000.0 / (e2e_elapsed_ms as f64)
        };
        let prepare_profile = PrepareProfile {
            scan_ms: prepare_report.scan_ms,
            pair_ms: prepare_report.pair_ms,
            hash_ms: prepare_report.hash_ms,
            enqueue_ms,
        };
        let e2e_profile = IngestE2eProfile {
            prepare: prepare_profile,
            runtime: run_profile.runtime_profile,
            prep_elapsed_ms,
            runner_elapsed_ms,
            e2e_elapsed_ms,
        };

        Ok(DirImportReport {
            prepare_report,
            rejected_by_reason,
            bundle_id: Some(bundle.bundle_id),
            workers_used: worker_count,
            processed_jobs: run_profile.processed_jobs,
            file_jobs: run_profile.file_jobs,
            finalize_jobs: run_profile.finalize_jobs,
            hands_persisted: run_profile.hands_persisted,
            prep_elapsed_ms,
            runner_elapsed_ms,
            e2e_elapsed_ms,
            hands_per_minute: hands_per_minute_runner,
            hands_per_minute_runner,
            hands_per_minute_e2e,
            e2e_profile,
            stage_profile: run_profile.stage_profile,
        })
    })();

    let _ = fs::remove_dir_all(&materialize_root);
    result
}

#[cfg(test)]
fn import_path_with_database_url(
    database_url: &str,
    path: &str,
    player_profile_id: Uuid,
) -> Result<LocalImportReport> {
    let input = fs::read_to_string(path).with_context(|| format!("failed to read `{path}`"))?;

    let mut client =
        Client::connect(database_url, NoTls).context("failed to connect to PostgreSQL")?;
    let mut tx = client
        .transaction()
        .context("failed to start import transaction")?;
    let context = load_import_context(&mut tx, player_profile_id)?;

    let report = match detect_source_kind(&input)? {
        SourceKind::TournamentSummary => {
            import_tournament_summary(&mut tx, &context, path, &input)?
        }
        SourceKind::HandHistory => import_hand_history(&mut tx, &context, path, &input)?,
    };

    // Legacy path: tournament_hand_order + FT helper are deferred from persist to here
    // (in runner path they run in bundle_finalize instead).
    if report.file_kind == "hh" {
        compute_tournament_hand_order(&mut tx, report.tournament_id)?;
        let source_hands = load_ft_helper_source_hands_from_db(
            &mut tx,
            report.tournament_id,
            context.player_profile_id,
        )?;
        let ft_helper_row = build_mbr_tournament_ft_helper_row(
            report.tournament_id,
            context.player_profile_id,
            &source_hands,
        );
        persist_mbr_tournament_ft_helper(&mut tx, &ft_helper_row)?;
    }

    materialize_player_hand_features(&mut tx, context.organization_id, context.player_profile_id)?;

    tx.commit().context("failed to commit import transaction")?;
    Ok(report)
}

#[cfg(test)]
fn import_tournament_summary(
    tx: &mut postgres::Transaction<'_>,
    context: &ImportContext,
    path: &str,
    input: &str,
) -> Result<LocalImportReport> {
    let source_file_id = insert_source_file(tx, context, path, input, "ts")?;
    let source_file_member_id = insert_source_file_member(tx, source_file_id, path, "ts", input)?;
    let import_job_id = insert_import_job(tx, context.organization_id, source_file_id)?;
    insert_job_attempt(tx, import_job_id)?;
    import_tournament_summary_registered(
        tx,
        context,
        path,
        input,
        source_file_id,
        source_file_member_id,
        import_job_id,
    )
}

#[cfg(test)]
// The batch-persist path is the live contract, but we keep the legacy per-hand helpers around
// during this rollout for focused DB regressions and rollback/debugging.
#[allow(dead_code)]
fn import_tournament_summary_registered(
    tx: &mut impl postgres::GenericClient,
    context: &ImportContext,
    _path: &str,
    input: &str,
    source_file_id: Uuid,
    source_file_member_id: Uuid,
    import_job_id: Uuid,
) -> Result<LocalImportReport> {
    let prepared = prepare_tournament_summary_import(input)?;
    persist_prepared_tournament_summary_registered(
        tx,
        context,
        input,
        source_file_id,
        source_file_member_id,
        import_job_id,
        &prepared,
    )
}

#[cfg(test)]
fn import_hand_history(
    tx: &mut postgres::Transaction<'_>,
    context: &ImportContext,
    path: &str,
    input: &str,
) -> Result<LocalImportReport> {
    let source_file_id = insert_source_file(tx, context, path, input, "hh")?;
    let source_file_member_id = insert_source_file_member(tx, source_file_id, path, "hh", input)?;
    let import_job_id = insert_import_job(tx, context.organization_id, source_file_id)?;
    insert_job_attempt(tx, import_job_id)?;
    import_hand_history_registered(
        tx,
        context,
        path,
        input,
        source_file_id,
        source_file_member_id,
        import_job_id,
    )
}

#[cfg(test)]
#[allow(dead_code)]
fn import_hand_history_registered(
    tx: &mut impl postgres::GenericClient,
    context: &ImportContext,
    _path: &str,
    input: &str,
    source_file_id: Uuid,
    source_file_member_id: Uuid,
    import_job_id: Uuid,
) -> Result<LocalImportReport> {
    let prepared = prepare_hand_history_import(input, context.player_profile_id)?;
    persist_prepared_hand_history_registered(
        tx,
        context,
        source_file_id,
        source_file_member_id,
        import_job_id,
        &prepared,
    )
}
