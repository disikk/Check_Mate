use std::process::ExitCode;

use anyhow::Result;
use tokio::net::TcpListener;
use tracker_web_api::{WebApiConfig, serve};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let config = WebApiConfig::from_env()?;
    let listener = TcpListener::bind(config.bind_addr).await?;
    serve(listener, config).await
}
