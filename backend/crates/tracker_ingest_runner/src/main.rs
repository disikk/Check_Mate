use std::{env, process::ExitCode, thread, time::Duration};

use anyhow::{Context, Result};
use tracker_ingest_runner::{RunnerConfig, drain_once};

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeArgs {
    once: bool,
    worker_count: Option<usize>,
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
    let args = parse_runtime_args(&env::args().skip(1).collect::<Vec<_>>())?;
    let database_url = env::var("CHECK_MATE_DATABASE_URL")
        .context("CHECK_MATE_DATABASE_URL is required for tracker_ingest_runner")?;
    let config = RunnerConfig {
        runner_name: env::var("CHECK_MATE_INGEST_RUNNER_NAME")
            .unwrap_or_else(|_| RunnerConfig::default().runner_name),
        max_attempts: env::var("CHECK_MATE_INGEST_MAX_ATTEMPTS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3),
        worker_count: args
            .worker_count
            .or(parse_worker_count_env("CHECK_MATE_INGEST_RUNNER_WORKERS")?)
            .unwrap_or_else(|| RunnerConfig::default().worker_count),
    };
    let poll_ms = env::var("CHECK_MATE_INGEST_RUNNER_POLL_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(250);

    if args.once {
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

fn parse_runtime_args(args: &[String]) -> Result<RuntimeArgs> {
    let mut once = false;
    let mut worker_count = None;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--once" => {
                once = true;
                index += 1;
            }
            "--workers" => {
                let value = args.get(index + 1).context("missing value for --workers")?;
                worker_count = Some(parse_worker_count("--workers", value)?);
                index += 2;
            }
            other => {
                anyhow::bail!("unsupported argument `{other}`");
            }
        }
    }

    Ok(RuntimeArgs { once, worker_count })
}

fn parse_worker_count(flag: &str, value: &str) -> Result<usize> {
    let worker_count = value
        .parse::<usize>()
        .with_context(|| format!("invalid {flag} value `{value}`"))?;
    if worker_count == 0 {
        anyhow::bail!("{flag} must be greater than zero");
    }
    Ok(worker_count)
}

fn parse_worker_count_env(name: &str) -> Result<Option<usize>> {
    match env::var(name) {
        Ok(value) => Ok(Some(parse_worker_count(name, &value)?)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(anyhow::anyhow!("failed to read {name}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_runtime_args_accepts_workers_override() {
        let parsed = parse_runtime_args(&[
            "--once".to_string(),
            "--workers".to_string(),
            "4".to_string(),
        ])
        .expect("args should parse");

        assert!(parsed.once);
        assert_eq!(parsed.worker_count, Some(4));
    }

    #[test]
    fn parse_runtime_args_rejects_zero_workers() {
        let error = parse_runtime_args(&["--workers".to_string(), "0".to_string()])
            .expect_err("zero workers must fail");

        assert!(
            error
                .to_string()
                .contains("--workers must be greater than zero"),
            "unexpected error: {error:#}"
        );
    }
}
