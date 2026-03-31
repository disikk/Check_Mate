mod dto;
mod errors;
mod routes;
mod upload_spool;
mod ws;

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use axum::{Router, routing::{get, post}};
use tokio::net::TcpListener;

// Re-export public API consumed by main.rs and integration tests.
pub use dto::{
    ApiBundleFile, ApiBundleFileDiagnostic, ApiBundleSnapshot, ApiActivityLogEntry,
    CreateBundleResponse, SessionResponse, WsServerMessage,
};

/// Stub session seed for web-only development flow.
#[derive(Debug, Clone)]
pub struct StubSessionSeed {
    pub organization_name: String,
    pub user_email: String,
    pub player_screen_name: String,
}

impl Default for StubSessionSeed {
    fn default() -> Self {
        Self {
            organization_name: "Check Mate Web Org".to_string(),
            user_email: "web-stub@example.com".to_string(),
            player_screen_name: "Hero".to_string(),
        }
    }
}

/// Configuration for the web API server.
#[derive(Debug, Clone)]
pub struct WebApiConfig {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub spool_dir: PathBuf,
    pub session_seed: StubSessionSeed,
    pub ws_poll_interval: Duration,
}

impl WebApiConfig {
    pub fn from_env() -> Result<Self> {
        let bind_addr = std::env::var("CHECK_MATE_WEB_API_BIND")
            .unwrap_or_else(|_| "127.0.0.1:3001".to_string())
            .parse()
            .context("failed to parse CHECK_MATE_WEB_API_BIND")?;

        Ok(Self {
            bind_addr,
            database_url: std::env::var("CHECK_MATE_DATABASE_URL")
                .context("CHECK_MATE_DATABASE_URL is required for tracker_web_api")?,
            spool_dir: std::env::var("CHECK_MATE_WEB_SPOOL_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(".local/upload_spool")),
            session_seed: StubSessionSeed {
                organization_name: std::env::var("CHECK_MATE_WEB_ORG_NAME")
                    .unwrap_or_else(|_| StubSessionSeed::default().organization_name),
                user_email: std::env::var("CHECK_MATE_WEB_USER_EMAIL")
                    .unwrap_or_else(|_| StubSessionSeed::default().user_email),
                player_screen_name: std::env::var("CHECK_MATE_WEB_PLAYER_NAME")
                    .unwrap_or_else(|_| StubSessionSeed::default().player_screen_name),
            },
            ws_poll_interval: Duration::from_millis(
                std::env::var("CHECK_MATE_WEB_WS_POLL_MS")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(250),
            ),
        })
    }
}

/// Shared application state threaded through axum extractors.
#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) config: Arc<WebApiConfig>,
}

/// Build the axum router with all routes wired.
pub fn build_app(config: WebApiConfig) -> Router {
    let state = AppState {
        config: Arc::new(config),
    };

    Router::new()
        .route("/api/session", get(routes::session::get_session))
        .route(
            "/api/ingest/bundles",
            post(routes::ingest::create_ingest_bundle),
        )
        .route("/api/ft/dashboard", get(routes::dashboard::get_ft_dashboard))
        .route(
            "/api/ingest/bundles/{bundle_id}",
            get(routes::ingest::get_bundle_snapshot_handler),
        )
        .route(
            "/api/ingest/bundles/{bundle_id}/ws",
            get(routes::ingest::bundle_events_ws),
        )
        .with_state(state)
}

/// Start serving on the given listener.
pub async fn serve(listener: TcpListener, config: WebApiConfig) -> Result<()> {
    axum::serve(listener, build_app(config))
        .await
        .context("tracker_web_api server failed")
}
