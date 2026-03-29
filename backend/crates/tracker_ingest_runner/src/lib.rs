use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerConfig {
    pub runner_name: String,
    pub max_attempts: i32,
    pub worker_count: usize,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            runner_name: "tracker_ingest_runner".to_string(),
            max_attempts: 3,
            worker_count: parser_worker::local_import::default_runner_worker_count(),
        }
    }
}

pub fn drain_once(database_url: &str, config: &RunnerConfig) -> Result<usize> {
    Ok(parser_worker::local_import::run_ingest_runner_parallel(
        database_url,
        &config.runner_name,
        config.max_attempts,
        config.worker_count,
    )?
    .processed_jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_runner_config_caps_worker_count_by_cpu_budget() {
        let expected = std::thread::available_parallelism()
            .map(|value| value.get().min(8))
            .unwrap_or(1);

        let config = RunnerConfig::default();

        assert_eq!(config.worker_count, expected);
    }
}
