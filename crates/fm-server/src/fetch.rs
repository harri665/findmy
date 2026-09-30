//! Scheduled and manual report retrieval share a lock and ingestion path.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use fm_apple::Credentials;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::api::{lock, AppState, IngestResult, Shared};
use crate::{api, db};

#[derive(Serialize)]
pub struct Status {
    pub connected: bool,
    pub configured: bool,
    pub running: bool,
    pub last_fetch: Option<i64>,
    pub last_success: Option<i64>,
    pub next_fetch: Option<i64>,
    pub error: Option<String>,
    pub result: Option<IngestResult>,
}

pub enum FetchError {
    Busy,
    Unconfigured,
    Failed(String),
}

fn credentials(state: &AppState, sealed: &str) -> Result<Credentials> {
    let sealed = B64.decode(sealed).context("invalid stored Apple session")?;
    let clear = state
        .sealer
        .open(&sealed)
        .context("cannot decrypt Apple session; check the master key")?;
    serde_json::from_slice(&clear).context("invalid stored Apple session")
}

fn interval(conn: &rusqlite::Connection) -> Result<i64> {
    Ok(db::get_setting(conn, "fetch_interval_min")?
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(15)
        .clamp(5, 1440)
        * 60)
}

pub fn status(state: &AppState) -> Result<Status> {
    let conn = lock(state);
    let connected = db::get_setting(&conn, "apple_session")?.is_some();
    let configured = connected && db::get_setting(&conn, "anisette_url")?.is_some();
    let last_fetch = db::get_setting(&conn, "last_fetch")?.and_then(|v| v.parse::<i64>().ok());
    let next_fetch = if configured {
        let delay = interval(&conn)?;
        Some(last_fetch.map(|t| t + delay).unwrap_or_else(db::now))
    } else {
        None
    };
    Ok(Status {
        connected,
        configured,
        running: state.fetch_lock.try_lock().is_err(),
        last_fetch,
        last_success: db::get_setting(&conn, "last_success")?.and_then(|v| v.parse().ok()),
        next_fetch,
        error: db::get_setting(&conn, "fetch_error")?,
        result: db::get_setting(&conn, "last_fetch_result")?
            .and_then(|v| serde_json::from_str(&v).ok()),
    })
}

pub async fn run(state: &AppState) -> std::result::Result<IngestResult, FetchError> {
    let _guard = state.fetch_lock.try_lock().map_err(|_| FetchError::Busy)?;
    let snapshot = (|| -> Result<_> {
        let conn = lock(state);
        Ok((
            db::get_setting(&conn, "apple_session")?,
            db::get_setting(&conn, "anisette_url")?,
            db::all_hashed_keys(&conn)?,
        ))
    })()
    .map_err(|e| FetchError::Failed(e.to_string()))?;
    let (Some(sealed), Some(provider), keys) = snapshot else {
        return Err(FetchError::Unconfigured);
    };
    let started = db::now();
    let mut total = IngestResult::default();
    let outcome = async {
        db::set_setting(&lock(state), "last_fetch", Some(&started.to_string()))?;
        let credentials = credentials(state, &sealed)?;
        // Batch by key count while preserving device boundaries within each request.
        for chunk in keys.chunks(fm_apple::BATCH_SIZE) {
            let mut groups = BTreeMap::<i64, Vec<String>>::new();
            for (id, key) in chunk {
                groups.entry(*id).or_default().push(key.clone());
            }
            let groups: Vec<_> = groups.into_values().collect();
            let reports = state
                .apple
                .fetch(
                    &credentials,
                    &provider,
                    &groups,
                    started,
                    &api::now_rfc3339(),
                )
                .await?;
            let result = api::store_reports(state, &reports)?;
            total.stored += result.stored;
            total.duplicate += result.duplicate;
            total.unknown_key += result.unknown_key;
            total.invalid += result.invalid;
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let saved = (|| -> Result<_> {
        let mut conn = lock(state);
        let tx = conn.transaction()?;
        db::set_setting(
            &tx,
            "last_fetch_result",
            Some(&serde_json::to_string(&total)?),
        )?;
        let error = outcome.as_ref().err().map(|e| e.to_string());
        db::set_setting(&tx, "fetch_error", error.as_deref())?;
        if outcome.is_ok() {
            db::set_setting(&tx, "last_success", Some(&db::now().to_string()))?;
        }
        tx.commit()?;
        Ok(())
    })();
    saved.map_err(|e| FetchError::Failed(e.to_string()))?;
    outcome.map_err(|e| FetchError::Failed(e.to_string()))?;
    Ok(total)
}

pub async fn worker(state: Shared) {
    loop {
        match status(&state) {
            Ok(s) if s.configured && !s.running && s.next_fetch.is_some_and(|t| t <= db::now()) => {
                if let Err(FetchError::Failed(error)) = run(&state).await {
                    tracing::warn!("report fetch failed: {error}");
                }
            }
            Err(error) => tracing::error!("reading report fetch configuration: {error}"),
            _ => {}
        }
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {},
            _ = state.fetch_notify.notified() => {},
        }
    }
}
