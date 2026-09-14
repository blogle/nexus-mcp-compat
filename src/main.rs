use std::{env, net::SocketAddr};

use anyhow::{Context, Result};
use axum::{Router, http::StatusCode, routing::get};
use nexus_mcp_compat::NexusCompat;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:8000";

const DEFAULT_ALLOWED_HOSTS: &str = concat!(
    "localhost,",
    "127.0.0.1,",
    "nexus-mcp-compat,",
    "nexus-mcp-compat.nexus.svc,",
    "nexus-mcp-compat.nexus.svc.cluster.local"
);

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let listen_addr = env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| DEFAULT_LISTEN_ADDR.to_owned())
        .parse::<SocketAddr>()
        .context("invalid LISTEN_ADDR")?;

    let nexus_mcp_url = env::var("NEXUS_MCP_URL").context("NEXUS_MCP_URL must be set")?;

    let allowed_hosts = env::var("MCP_ALLOWED_HOSTS")
        .unwrap_or_else(|_| DEFAULT_ALLOWED_HOSTS.to_owned())
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let cancellation = CancellationToken::new();

    let bridge = NexusCompat::new(nexus_mcp_url.clone());

    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_allowed_hosts(allowed_hosts)
        .with_cancellation_token(cancellation.child_token());

    let mcp_service = StreamableHttpService::new(
        move || Ok(bridge.clone()),
        LocalSessionManager::default().into(),
        config,
    );

    let app = Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(health))
        .nest_service("/mcp", mcp_service);

    let listener = TcpListener::bind(listen_addr)
        .await
        .with_context(|| format!("failed to bind {listen_addr}"))?;

    tracing::info!(
        %listen_addr,
        nexus_mcp_url = %nexus_mcp_url,
        "nexus-mcp-compat listening"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(cancellation.clone()))
        .await
        .context("HTTP server failed")?;

    Ok(())
}

async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .flatten_event(true)
        .init();
}

async fn shutdown_signal(cancellation: CancellationToken) {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }

    tracing::info!("shutdown requested");
    cancellation.cancel();
}
