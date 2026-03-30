use std::{env, fs, process::ExitCode};

use anyhow::{Context, Result};
use parser_worker::local_import;
use serde::Serialize;
use tracker_ingest_prepare::PrepareReport;
use tracker_parser_core::{
    SourceKind, detect_source_kind,
    parsers::{hand_history::split_hand_history, tournament_summary::parse_tournament_summary},
};
use uuid::Uuid;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WorkerOutput {
    TournamentSummary {
        tournament_id: u64,
        finish_place: u32,
        payout_cents: i64,
    },
    HandHistory {
        hand_count: usize,
        tournament_id: u64,
        first_hand_id: String,
        last_hand_id: String,
    },
    LocalImport {
        file_kind: String,
        source_file_id: String,
        import_job_id: String,
        tournament_id: String,
        fragments_persisted: usize,
        hands_persisted: usize,
        stage_profile: local_import::IngestStageProfile,
    },
    DirImport {
        report: Box<local_import::DirImportReport>,
    },
    DirImportPrepare {
        report: PrepareReport,
    },
    UserTimezoneUpdated {
        user_id: String,
        timezone_name: String,
        affected_profiles: usize,
        tournaments_recomputed: u64,
        hands_recomputed: u64,
    },
    UserTimezoneCleared {
        user_id: String,
        affected_profiles: usize,
        tournaments_recomputed: u64,
        hands_recomputed: u64,
    },
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
    let args: Vec<String> = env::args().skip(1).collect();
    let command = parse_args(&args)?;

    let output = match command {
        WorkerCommand::Summarize { path } => summarize_path(&path)?,
        WorkerCommand::ImportLocal {
            path,
            player_profile_id,
        } => {
            let report = local_import::import_path(&path, player_profile_id)?;
            WorkerOutput::LocalImport {
                file_kind: report.file_kind.to_string(),
                source_file_id: report.source_file_id.to_string(),
                import_job_id: report.import_job_id.to_string(),
                tournament_id: report.tournament_id.to_string(),
                fragments_persisted: report.fragments_persisted,
                hands_persisted: report.hands_persisted,
                stage_profile: report.stage_profile,
            }
        }
        WorkerCommand::DirImport {
            path,
            player_profile_id,
            worker_count,
        } => WorkerOutput::DirImport {
            report: Box::new(local_import::dir_import_path(
                &path,
                player_profile_id,
                worker_count,
            )?),
        },
        WorkerCommand::DirImportPrepare { path } => WorkerOutput::DirImportPrepare {
            report: tracker_ingest_prepare::prepare_path(&path)?,
        },
        WorkerCommand::SetUserTimezone {
            user_id,
            timezone_name,
        } => {
            let report = local_import::set_user_timezone(user_id, &timezone_name)?;
            WorkerOutput::UserTimezoneUpdated {
                user_id: report.user_id.to_string(),
                timezone_name: report.timezone_name.unwrap_or_default(),
                affected_profiles: report.affected_profiles,
                tournaments_recomputed: report.tournaments_recomputed,
                hands_recomputed: report.hands_recomputed,
            }
        }
        WorkerCommand::ClearUserTimezone { user_id } => {
            let report = local_import::clear_user_timezone(user_id)?;
            WorkerOutput::UserTimezoneCleared {
                user_id: report.user_id.to_string(),
                affected_profiles: report.affected_profiles,
                tournaments_recomputed: report.tournaments_recomputed,
                hands_recomputed: report.hands_recomputed,
            }
        }
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WorkerCommand {
    Summarize {
        path: String,
    },
    ImportLocal {
        path: String,
        player_profile_id: Uuid,
    },
    DirImportPrepare {
        path: String,
    },
    DirImport {
        path: String,
        player_profile_id: Uuid,
        worker_count: usize,
    },
    SetUserTimezone {
        user_id: Uuid,
        timezone_name: String,
    },
    ClearUserTimezone {
        user_id: Uuid,
    },
}

fn parse_args(args: &[String]) -> Result<WorkerCommand> {
    match args {
        [path] => Ok(WorkerCommand::Summarize { path: path.clone() }),
        [mode, rest @ ..] if mode == "import-local" => parse_import_local_args(rest),
        [mode, rest @ ..] if mode == "dir-import" => parse_dir_import_args(rest),
        [mode, flag1, user_id, flag2, timezone_name]
            if mode == "set-user-timezone" && flag1 == "--user-id" && flag2 == "--timezone" =>
        {
            Ok(WorkerCommand::SetUserTimezone {
                user_id: parse_uuid_flag("--user-id", user_id)?,
                timezone_name: timezone_name.clone(),
            })
        }
        [mode, flag, user_id] if mode == "clear-user-timezone" && flag == "--user-id" => {
            Ok(WorkerCommand::ClearUserTimezone {
                user_id: parse_uuid_flag("--user-id", user_id)?,
            })
        }
        _ => Err(usage_error()),
    }
}

fn parse_import_local_args(args: &[String]) -> Result<WorkerCommand> {
    match args {
        [flag, player_profile_id, path] if flag == "--player-profile-id" => {
            Ok(WorkerCommand::ImportLocal {
                path: path.clone(),
                player_profile_id: parse_uuid_flag("--player-profile-id", player_profile_id)?,
            })
        }
        _ => Err(usage_error()),
    }
}

fn parse_dir_import_args(args: &[String]) -> Result<WorkerCommand> {
    if let [flag, path] = args
        && flag == "--prepare-only"
    {
        return Ok(WorkerCommand::DirImportPrepare { path: path.clone() });
    }

    let mut player_profile_id = None;
    let mut worker_count = local_import::default_runner_worker_count();
    let mut path = None;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--player-profile-id" => {
                let value = args.get(index + 1).ok_or_else(usage_error)?;
                player_profile_id = Some(parse_uuid_flag("--player-profile-id", value)?);
                index += 2;
            }
            "--workers" => {
                let value = args.get(index + 1).ok_or_else(usage_error)?;
                worker_count = parse_usize_flag("--workers", value)?;
                index += 2;
            }
            value if !value.starts_with("--") && path.is_none() => {
                path = Some(value.to_string());
                index += 1;
            }
            _ => return Err(usage_error()),
        }
    }

    Ok(WorkerCommand::DirImport {
        path: path.ok_or_else(usage_error)?,
        player_profile_id: player_profile_id.ok_or_else(usage_error)?,
        worker_count,
    })
}

fn parse_uuid_flag(flag: &str, value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).with_context(|| format!("invalid {flag} value `{value}`"))
}

fn parse_usize_flag(flag: &str, value: &str) -> Result<usize> {
    let parsed = value
        .parse::<usize>()
        .with_context(|| format!("invalid {flag} value `{value}`"))?;
    if parsed == 0 {
        anyhow::bail!("{flag} must be greater than zero");
    }
    Ok(parsed)
}

fn usage_error() -> anyhow::Error {
    anyhow::anyhow!(
        "usage: cargo run -p parser_worker -- <path-to-hh-or-ts>\n       cargo run -p parser_worker -- import-local --player-profile-id <uuid> <path-to-hh-or-ts>\n       cargo run -p parser_worker -- dir-import --prepare-only <path-to-directory-or-archive>\n       cargo run -p parser_worker -- dir-import --player-profile-id <uuid> [--workers <n>] <path-to-directory-or-archive>\n       cargo run -p parser_worker -- set-user-timezone --user-id <uuid> --timezone <iana-timezone>\n       cargo run -p parser_worker -- clear-user-timezone --user-id <uuid>"
    )
}

fn summarize_path(path: &str) -> Result<WorkerOutput> {
    let input = fs::read_to_string(path).with_context(|| format!("failed to read `{path}`"))?;

    match detect_source_kind(&input)? {
        SourceKind::TournamentSummary => {
            let summary = parse_tournament_summary(&input)?;
            Ok(WorkerOutput::TournamentSummary {
                tournament_id: summary.tournament_id,
                finish_place: summary.finish_place,
                payout_cents: summary.payout_cents,
            })
        }
        SourceKind::HandHistory => {
            let hands = split_hand_history(&input)?;
            let first = hands.first().context("hand history contains no hands")?;
            let last = hands.last().context("hand history contains no hands")?;

            Ok(WorkerOutput::HandHistory {
                hand_count: hands.len(),
                tournament_id: first.header.tournament_id,
                first_hand_id: first.header.hand_id.clone(),
                last_hand_id: last.header.hand_id.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use parser_worker::local_import::{
        ComputeProfile, DirImportReport, IngestE2eProfile, IngestStageProfile, PrepareProfile,
    };
    use serde_json::json;
    use tracker_ingest_prepare::PrepareReport;
    use uuid::Uuid;

    #[test]
    fn parse_args_requires_player_profile_id_for_import_local() {
        let error = parse_args(&["import-local".to_string(), "fixture.txt".to_string()])
            .expect_err("import-local without explicit player profile must fail");

        assert!(
            error
                .to_string()
                .contains("--player-profile-id <uuid> <path-to-hh-or-ts>"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn parse_args_accepts_import_local_with_explicit_player_profile_id() {
        let player_profile_id = Uuid::new_v4().to_string();

        let parsed = parse_args(&[
            "import-local".to_string(),
            "--player-profile-id".to_string(),
            player_profile_id.clone(),
            "fixture.txt".to_string(),
        ]);

        assert!(
            parsed.is_ok(),
            "import-local with explicit player profile should parse, got {parsed:?}"
        );
    }

    #[test]
    fn parse_args_accepts_set_user_timezone_command() {
        let parsed = parse_args(&[
            "set-user-timezone".to_string(),
            "--user-id".to_string(),
            Uuid::new_v4().to_string(),
            "--timezone".to_string(),
            "Asia/Krasnoyarsk".to_string(),
        ]);

        assert!(
            parsed.is_ok(),
            "set-user-timezone should parse, got {parsed:?}"
        );
    }

    #[test]
    fn parse_args_accepts_dir_import_prepare_only_command() {
        let parsed = parse_args(&[
            "dir-import".to_string(),
            "--prepare-only".to_string(),
            "fixtures/mbr/quarantine_sample".to_string(),
        ]);

        assert!(
            parsed.is_ok(),
            "dir-import --prepare-only should parse, got {parsed:?}"
        );
    }

    #[test]
    fn parse_args_accepts_dir_import_with_player_profile_and_workers() {
        let player_profile_id = Uuid::new_v4();

        let parsed = parse_args(&[
            "dir-import".to_string(),
            "--player-profile-id".to_string(),
            player_profile_id.to_string(),
            "--workers".to_string(),
            "4".to_string(),
            "fixtures/mbr/quarantine_sample".to_string(),
        ])
        .expect("dir-import with player profile and workers should parse");

        assert_eq!(
            parsed,
            WorkerCommand::DirImport {
                path: "fixtures/mbr/quarantine_sample".to_string(),
                player_profile_id,
                worker_count: 4,
            }
        );
    }

    #[test]
    fn parse_args_accepts_clear_user_timezone_command() {
        let parsed = parse_args(&[
            "clear-user-timezone".to_string(),
            "--user-id".to_string(),
            Uuid::new_v4().to_string(),
        ]);

        assert!(
            parsed.is_ok(),
            "clear-user-timezone should parse, got {parsed:?}"
        );
    }

    #[test]
    fn local_import_output_serializes_stage_profile_contract() {
        let output = WorkerOutput::LocalImport {
            file_kind: "hh".to_string(),
            source_file_id: Uuid::nil().to_string(),
            import_job_id: Uuid::nil().to_string(),
            tournament_id: Uuid::nil().to_string(),
            fragments_persisted: 3,
            hands_persisted: 2,
            stage_profile: IngestStageProfile {
                parse_ms: 11,
                normalize_ms: 12,
                persist_ms: 13,
                materialize_ms: 14,
                finalize_ms: 15,
            },
        };

        let value = serde_json::to_value(output).expect("worker output must serialize");
        assert_eq!(
            value,
            json!({
                "kind": "local_import",
                "file_kind": "hh",
                "source_file_id": Uuid::nil().to_string(),
                "import_job_id": Uuid::nil().to_string(),
                "tournament_id": Uuid::nil().to_string(),
                "fragments_persisted": 3,
                "hands_persisted": 2,
                "stage_profile": {
                    "parse_ms": 11,
                    "normalize_ms": 12,
                    "persist_ms": 13,
                    "materialize_ms": 14,
                    "finalize_ms": 15
                }
            })
        );
    }

    #[test]
    fn dir_import_output_serializes_e2e_profile_contract() {
        let report = DirImportReport {
            prepare_report: PrepareReport {
                scanned_files: 2,
                paired_tournaments: vec![],
                rejected_tournaments: vec![],
                scan_ms: 11,
                pair_ms: 12,
                hash_ms: 13,
            },
            rejected_by_reason: BTreeMap::from([("missing_hh".to_string(), 1usize)]),
            bundle_id: Some(Uuid::nil()),
            workers_used: 4,
            processed_jobs: 3,
            file_jobs: 2,
            finalize_jobs: 1,
            hands_persisted: 321,
            prep_elapsed_ms: 20,
            runner_elapsed_ms: 40,
            e2e_elapsed_ms: 60,
            hands_per_minute: 4_815.0,
            hands_per_minute_runner: 4_815.0,
            hands_per_minute_e2e: 3_210.0,
            e2e_profile: IngestE2eProfile {
                prepare: PrepareProfile {
                    scan_ms: 11,
                    pair_ms: 12,
                    hash_ms: 13,
                    enqueue_ms: 14,
                },
                runtime: ComputeProfile {
                    parse_ms: 21,
                    normalize_ms: 22,
                    derive_hand_local_ms: 23,
                    derive_tournament_ms: 24,
                    persist_db_ms: 25,
                    materialize_ms: 26,
                    finalize_ms: 27,
                },
                prep_elapsed_ms: 20,
                runner_elapsed_ms: 40,
                e2e_elapsed_ms: 60,
            },
            stage_profile: IngestStageProfile {
                parse_ms: 21,
                normalize_ms: 22,
                persist_ms: 72,
                materialize_ms: 26,
                finalize_ms: 27,
            },
        };
        let output = WorkerOutput::DirImport {
            report: Box::new(report),
        };

        let value = serde_json::to_value(output).expect("worker output must serialize");
        assert_eq!(
            value,
            json!({
                "kind": "dir_import",
                "report": {
                    "prepare_report": {
                        "scanned_files": 2,
                        "paired_tournaments": [],
                        "rejected_tournaments": [],
                        "scan_ms": 11,
                        "pair_ms": 12,
                        "hash_ms": 13
                    },
                    "rejected_by_reason": {
                        "missing_hh": 1
                    },
                    "bundle_id": Uuid::nil(),
                    "workers_used": 4,
                    "processed_jobs": 3,
                    "file_jobs": 2,
                    "finalize_jobs": 1,
                    "hands_persisted": 321,
                    "prep_elapsed_ms": 20,
                    "runner_elapsed_ms": 40,
                    "e2e_elapsed_ms": 60,
                    "hands_per_minute": 4815.0,
                    "hands_per_minute_runner": 4815.0,
                    "hands_per_minute_e2e": 3210.0,
                    "e2e_profile": {
                        "prepare": {
                            "scan_ms": 11,
                            "pair_ms": 12,
                            "hash_ms": 13,
                            "enqueue_ms": 14
                        },
                        "runtime": {
                            "parse_ms": 21,
                            "normalize_ms": 22,
                            "derive_hand_local_ms": 23,
                            "derive_tournament_ms": 24,
                            "persist_db_ms": 25,
                            "materialize_ms": 26,
                            "finalize_ms": 27
                        },
                        "prep_elapsed_ms": 20,
                        "runner_elapsed_ms": 40,
                        "e2e_elapsed_ms": 60
                    },
                    "stage_profile": {
                        "parse_ms": 21,
                        "normalize_ms": 22,
                        "persist_ms": 72,
                        "materialize_ms": 26,
                        "finalize_ms": 27
                    }
                }
            })
        );
    }
}
