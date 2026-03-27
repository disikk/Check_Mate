use std::{env, fs, process::ExitCode};

use anyhow::{Context, Result};
use serde::Serialize;
use tracker_parser_core::{
    SourceKind, detect_source_kind,
    parsers::{hand_history::split_hand_history, tournament_summary::parse_tournament_summary},
};
use uuid::Uuid;

mod local_import;

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
            }
        }
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
    Summarize { path: String },
    ImportLocal { path: String, player_profile_id: Uuid },
    SetUserTimezone { user_id: Uuid, timezone_name: String },
    ClearUserTimezone { user_id: Uuid },
}

fn parse_args(args: &[String]) -> Result<WorkerCommand> {
    match args {
        [path] => Ok(WorkerCommand::Summarize { path: path.clone() }),
        [mode, flag, player_profile_id, path]
            if mode == "import-local" && flag == "--player-profile-id" =>
        {
            Ok(WorkerCommand::ImportLocal {
                path: path.clone(),
                player_profile_id: parse_uuid_flag("--player-profile-id", player_profile_id)?,
            })
        }
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

fn parse_uuid_flag(flag: &str, value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).with_context(|| format!("invalid {flag} value `{value}`"))
}

fn usage_error() -> anyhow::Error {
    anyhow::anyhow!(
        "usage: cargo run -p parser_worker -- <path-to-hh-or-ts>\n       cargo run -p parser_worker -- import-local --player-profile-id <uuid> <path-to-hh-or-ts>\n       cargo run -p parser_worker -- set-user-timezone --user-id <uuid> --timezone <iana-timezone>\n       cargo run -p parser_worker -- clear-user-timezone --user-id <uuid>"
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
    use super::*;
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
}
