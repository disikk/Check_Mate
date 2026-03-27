use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerConfig {
    pub runner_name: String,
    pub max_attempts: i32,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            runner_name: "tracker_ingest_runner".to_string(),
            max_attempts: 3,
        }
    }
}

pub fn drain_once(database_url: &str, config: &RunnerConfig) -> Result<usize> {
    parser_worker::local_import::run_ingest_runner_until_idle(
        database_url,
        &config.runner_name,
        config.max_attempts,
    )
}
