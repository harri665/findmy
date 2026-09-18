//! fm-server: stores devices and their keys, decrypts location reports and
//! serves the dashboard API on `/api` (port 3001, behind the website's nginx).

mod api;
mod config;
mod crypto;
mod db;

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::api::AppState;
use crate::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let config = Config::from_env()?;
    let conn = db::open(&config.db_path()).with_context(|| format!("opening {}", config.db_path().display()))?;
    let state = Arc::new(AppState {
        db: Mutex::new(conn),
        sealer: crypto::Sealer::new(&config.master_key),
        admin_password_hash: crypto::sha256(config.admin_password.as_bytes()),
    });

    let app = api::router(state).layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(&config.bind).await.with_context(|| format!("binding {}", config.bind))?;
    tracing::info!("fm-server listening on {}", config.bind);
    axum::serve(listener, app).with_graceful_shutdown(shutdown()).await?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests;
