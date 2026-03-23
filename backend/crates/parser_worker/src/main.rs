use std::{env, fs, process::ExitCode};

use anyhow::{Context, Result};
use serde::Serialize;
use tracker_parser_core::{
    SourceKind, detect_source_kind,
    parsers::{hand_history::split_hand_history, tournament_summary::parse_tournament_summary},
};

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
    let (mode, path) = parse_args(&args)?;

    let output = match mode {
        WorkerMode::Summarize => summarize_path(&path)?,
        WorkerMode::ImportLocal => {
            let report = local_import::import_path(&path)?;
            WorkerOutput::LocalImport {
                file_kind: report.file_kind.to_string(),
                source_file_id: report.source_file_id.to_string(),
                import_job_id: report.import_job_id.to_string(),
                tournament_id: report.tournament_id.to_string(),
                fragments_persisted: report.fragments_persisted,
                hands_persisted: report.hands_persisted,
            }
        }
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum WorkerMode {
    Summarize,
    ImportLocal,
}

fn parse_args(args: &[String]) -> Result<(WorkerMode, String)> {
    match args {
        [path] => Ok((WorkerMode::Summarize, path.clone())),
        [mode, path] if mode == "import-local" => Ok((WorkerMode::ImportLocal, path.clone())),
        _ => Err(anyhow::anyhow!(
            "usage: cargo run -p parser_worker -- <path-to-hh-or-ts>\n       cargo run -p parser_worker -- import-local <path-to-hh-or-ts>"
        )),
    }
}

fn summarize_path(path: &str) -> Result<WorkerOutput> {
    let input = fs::read_to_string(&path).with_context(|| format!("failed to read `{path}`"))?;

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
