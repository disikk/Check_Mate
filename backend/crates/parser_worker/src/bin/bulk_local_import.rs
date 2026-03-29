use std::{env, fs, process::ExitCode};

use anyhow::{Context, Result, anyhow};
use parser_worker::local_import::run_ingest_runner_until_idle_with_profile;
use postgres::{Client, NoTls};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tracker_ingest_runtime::{FileKind, IngestBundleInput, IngestFileInput, enqueue_bundle};
use tracker_parser_core::{SourceKind, detect_source_kind};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
struct BulkImportArgs {
    player_profile_id: Uuid,
    list_file: String,
    chunk_size: usize,
    runner_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportActor {
    organization_id: Uuid,
    user_id: Uuid,
}

#[derive(Debug, Serialize)]
struct BulkImportOutput {
    bundle_count: usize,
    file_count: usize,
    chunk_size: usize,
    runner_name: String,
    processed_jobs: usize,
    file_jobs: usize,
    finalize_jobs: usize,
    hands_persisted: usize,
    runner_elapsed_ms: u64,
    hands_per_minute: f64,
    stage_profile: parser_worker::local_import::IngestStageProfile,
    bundle_ids: Vec<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = parse_args(&env::args().skip(1).collect::<Vec<_>>())?;
    let database_url = env::var("CHECK_MATE_DATABASE_URL")
        .context("CHECK_MATE_DATABASE_URL must be set for bulk_local_import")?;
    let paths = read_list_file(&args.list_file)?;

    if paths.is_empty() {
        return Err(anyhow!(
            "list file `{}` contained no import paths",
            args.list_file
        ));
    }

    let mut client =
        Client::connect(&database_url, NoTls).context("failed to connect to PostgreSQL")?;
    let actor = load_import_actor(&mut client, args.player_profile_id)?;
    let mut bundle_ids = Vec::new();

    for chunk in paths.chunks(args.chunk_size) {
        let files = chunk
            .iter()
            .map(|path| build_ingest_file_input(path))
            .collect::<Result<Vec<_>>>()?;

        let mut tx = client
            .transaction()
            .context("failed to start ingest enqueue transaction")?;
        let bundle = enqueue_bundle(
            &mut tx,
            &IngestBundleInput {
                organization_id: actor.organization_id,
                player_profile_id: args.player_profile_id,
                created_by_user_id: actor.user_id,
                files,
            },
        )?;
        tx.commit()
            .context("failed to commit ingest enqueue transaction")?;
        bundle_ids.push(bundle.bundle_id);
    }

    let runner_started_at = std::time::Instant::now();
    let run_profile =
        run_ingest_runner_until_idle_with_profile(&database_url, &args.runner_name, 3)?;
    let runner_elapsed_ms = runner_started_at.elapsed().as_millis() as u64;
    let hands_per_minute = if runner_elapsed_ms == 0 {
        0.0
    } else {
        (run_profile.hands_persisted as f64) * 60_000.0 / (runner_elapsed_ms as f64)
    };
    let output = BulkImportOutput {
        bundle_count: bundle_ids.len(),
        file_count: paths.len(),
        chunk_size: args.chunk_size,
        runner_name: args.runner_name,
        processed_jobs: run_profile.processed_jobs,
        file_jobs: run_profile.file_jobs,
        finalize_jobs: run_profile.finalize_jobs,
        hands_persisted: run_profile.hands_persisted,
        runner_elapsed_ms,
        hands_per_minute,
        stage_profile: run_profile.stage_profile,
        bundle_ids: bundle_ids.into_iter().map(|id| id.to_string()).collect(),
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn parse_args(args: &[String]) -> Result<BulkImportArgs> {
    let mut player_profile_id = None;
    let mut list_file = None;
    let mut chunk_size = 200usize;
    let mut runner_name = "parser_worker_bulk_local".to_string();

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--player-profile-id" => {
                let value = args.get(index + 1).ok_or_else(usage_error)?;
                player_profile_id = Some(
                    Uuid::parse_str(value)
                        .with_context(|| format!("invalid --player-profile-id `{value}`"))?,
                );
                index += 2;
            }
            "--list-file" => {
                let value = args.get(index + 1).ok_or_else(usage_error)?;
                list_file = Some(value.clone());
                index += 2;
            }
            "--chunk-size" => {
                let value = args.get(index + 1).ok_or_else(usage_error)?;
                chunk_size = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid --chunk-size `{value}`"))?;
                if chunk_size == 0 {
                    return Err(anyhow!("--chunk-size must be greater than zero"));
                }
                index += 2;
            }
            "--runner-name" => {
                let value = args.get(index + 1).ok_or_else(usage_error)?;
                runner_name = value.clone();
                index += 2;
            }
            _ => return Err(usage_error()),
        }
    }

    Ok(BulkImportArgs {
        player_profile_id: player_profile_id.ok_or_else(usage_error)?,
        list_file: list_file.ok_or_else(usage_error)?,
        chunk_size,
        runner_name,
    })
}

fn usage_error() -> anyhow::Error {
    anyhow!(
        "usage: cargo run -p parser_worker --bin bulk_local_import -- --player-profile-id <uuid> --list-file <path> [--chunk-size <n>] [--runner-name <name>]"
    )
}

fn read_list_file(path: &str) -> Result<Vec<String>> {
    Ok(fs::read_to_string(path)
        .with_context(|| format!("failed to read list file `{path}`"))?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn load_import_actor(client: &mut Client, player_profile_id: Uuid) -> Result<ImportActor> {
    let row = client
        .query_opt(
            "SELECT
                player_profiles.organization_id,
                player_profiles.owner_user_id
             FROM core.player_profiles AS player_profiles
             WHERE player_profiles.id = $1
               AND player_profiles.room = 'gg'",
            &[&player_profile_id],
        )?
        .ok_or_else(|| {
            anyhow!("player profile `{player_profile_id}` does not exist for room `gg`")
        })?;

    Ok(ImportActor {
        organization_id: row.get(0),
        user_id: row.get(1),
    })
}

fn build_ingest_file_input(path: &str) -> Result<IngestFileInput> {
    let input = fs::read_to_string(path).with_context(|| format!("failed to read `{path}`"))?;
    let file_kind = match detect_source_kind(&input)? {
        SourceKind::TournamentSummary => FileKind::TournamentSummary,
        SourceKind::HandHistory => FileKind::HandHistory,
    };

    Ok(IngestFileInput {
        room: "gg".to_string(),
        file_kind,
        sha256: sha256_hex(&input),
        original_filename: source_filename(path)?,
        byte_size: input.len() as i64,
        storage_uri: format!("local://{}", path.replace('\\', "/")),
        members: vec![],
        diagnostics: vec![],
    })
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn source_filename(path: &str) -> Result<String> {
    std::path::Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("failed to derive filename from `{path}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parser_worker::local_import::IngestStageProfile;
    use serde_json::json;

    #[test]
    fn parse_args_accepts_required_flags_and_defaults() {
        let parsed = parse_args(&[
            "--player-profile-id".to_string(),
            Uuid::nil().to_string(),
            "--list-file".to_string(),
            "/tmp/files.txt".to_string(),
        ])
        .expect("args should parse");

        assert_eq!(parsed.player_profile_id, Uuid::nil());
        assert_eq!(parsed.list_file, "/tmp/files.txt");
        assert_eq!(parsed.chunk_size, 200);
        assert_eq!(parsed.runner_name, "parser_worker_bulk_local");
    }

    #[test]
    fn parse_args_accepts_optional_flags() {
        let parsed = parse_args(&[
            "--player-profile-id".to_string(),
            Uuid::nil().to_string(),
            "--list-file".to_string(),
            "/tmp/files.txt".to_string(),
            "--chunk-size".to_string(),
            "50".to_string(),
            "--runner-name".to_string(),
            "miha_bulk".to_string(),
        ])
        .expect("args should parse");

        assert_eq!(parsed.chunk_size, 50);
        assert_eq!(parsed.runner_name, "miha_bulk");
    }

    #[test]
    fn parse_args_rejects_missing_required_flags() {
        let error = parse_args(&["--list-file".to_string(), "/tmp/files.txt".to_string()])
            .expect_err("missing player id must fail");

        assert!(
            error
                .to_string()
                .contains("--player-profile-id <uuid> --list-file <path>"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn bulk_import_output_serializes_stage_profile_contract() {
        let value = serde_json::to_value(BulkImportOutput {
            bundle_count: 2,
            file_count: 4,
            chunk_size: 2,
            runner_name: "bulk-runner".to_string(),
            processed_jobs: 6,
            bundle_ids: vec![Uuid::nil().to_string()],
            file_jobs: 4,
            finalize_jobs: 2,
            hands_persisted: 10,
            runner_elapsed_ms: 500,
            hands_per_minute: 1_200.0,
            stage_profile: IngestStageProfile {
                parse_ms: 100,
                normalize_ms: 110,
                persist_ms: 120,
                materialize_ms: 130,
                finalize_ms: 140,
            },
        })
        .expect("bulk import output must serialize");

        assert_eq!(
            value,
            json!({
                "bundle_count": 2,
                "file_count": 4,
                "chunk_size": 2,
                "runner_name": "bulk-runner",
                "processed_jobs": 6,
                "bundle_ids": [Uuid::nil().to_string()],
                "file_jobs": 4,
                "finalize_jobs": 2,
                "hands_persisted": 10,
                "runner_elapsed_ms": 500,
                "hands_per_minute": 1200.0,
                "stage_profile": {
                    "parse_ms": 100,
                    "normalize_ms": 110,
                    "persist_ms": 120,
                    "materialize_ms": 130,
                    "finalize_ms": 140
                }
            })
        );
    }
}
