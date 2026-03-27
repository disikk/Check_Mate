use std::{env, process::ExitCode, thread, time::Duration};

use anyhow::{Context, Result};
use tracker_ingest_runner::{RunnerConfig, drain_once};

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
    let database_url = env::var("CHECK_MATE_DATABASE_URL")
        .context("CHECK_MATE_DATABASE_URL is required for tracker_ingest_runner")?;
    let config = RunnerConfig {
        runner_name: env::var("CHECK_MATE_INGEST_RUNNER_NAME")
            .unwrap_or_else(|_| RunnerConfig::default().runner_name),
        max_attempts: env::var("CHECK_MATE_INGEST_MAX_ATTEMPTS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3),
    };
    let once = env::args().skip(1).any(|arg| arg == "--once");
    let poll_ms = env::var("CHECK_MATE_INGEST_RUNNER_POLL_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(250);

    if once {
        drain_once(&database_url, &config)?;
        return Ok(());
    }

    loop {
        let processed = drain_once(&database_url, &config)?;
        if processed == 0 {
            thread::sleep(Duration::from_millis(poll_ms));
        }
    }
}
