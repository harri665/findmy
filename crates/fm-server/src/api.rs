//! HTTP API under `/api`.
//!
//! Public: `GET /api/health`, `POST /api/login`.
//! Beacon agents: `GET /api/agent/blob` with `Authorization: Device <token>`.
//! Everything else needs `Authorization: Bearer <session token>` from login.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use fm_core::derive::derive_keypair;
use fm_core::keys::KeyPair;
use fm_core::schedule::KeyMode;
use fm_core::{blob, report, KEY_LEN};
use rand_core::{OsRng, RngCore};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::crypto::{self, Sealer};
use crate::db::{self, Location, NewDevice, NewKey};

/// Sessions last 30 days.
const SESSION_SECS: i64 = 30 * 24 * 3600;
const PLATFORMS: &[&str] = &["esp32", "nrf52", "linux", "windows", "other"];

pub struct AppState {
    pub db: Mutex<Connection>,
    pub sealer: Sealer,
    pub admin_password_hash: [u8; 32],
}

pub type Shared = Arc<AppState>;

pub fn router(state: Shared) -> Router {
    let protected = Router::new()
        .route("/api/devices", get(list_devices).post(create_device))
        .route("/api/devices/import", post(import_device))
        .route("/api/devices/{id}", get(get_device).patch(update_device).delete(delete_device))
        .route("/api/devices/{id}/blob", get(device_blob))
        .route("/api/devices/{id}/locations", get(device_locations))
        .route("/api/devices/{id}/agent-token", post(rotate_agent_token))
        .route("/api/reports", post(ingest_reports))
        .route("/api/keys/hashed", get(hashed_keys))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/logout", post(logout))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_session));

    Router::new()
        .route("/api/health", get(health))
        .route("/api/login", post(login))
        .route("/api/agent/blob", get(agent_blob))
        .merge(protected)
        .with_state(state)
}

// --- Errors -----------------------------------------------------------------

pub struct ApiError(StatusCode, String);

impl ApiError {
    fn bad_request(msg: impl Into<String>) -> Self {
        ApiError(StatusCode::BAD_REQUEST, msg.into())
    }
    fn not_found() -> Self {
        ApiError(StatusCode::NOT_FOUND, "not found".into())
    }
    fn unauthorized() -> Self {
        ApiError(StatusCode::UNAUTHORIZED, "unauthorized".into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!("internal error: {e:#}");
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
    }
}

type ApiResult<T> = Result<T, ApiError>;

fn lock(state: &AppState) -> std::sync::MutexGuard<'_, Connection> {
    // A panic while holding the lock leaves SQLite consistent (transactions
    // roll back), so a poisoned mutex is safe to keep using.
    state.db.lock().unwrap_or_else(|p| p.into_inner())
}

// --- Health and auth --------------------------------------------------------

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "timestamp": now_rfc3339() }))
}

#[derive(Deserialize)]
struct LoginBody {
    password: String,
}

async fn login(State(state): State<Shared>, Json(body): Json<LoginBody>) -> ApiResult<Json<serde_json::Value>> {
    if !crypto::ct_eq(&crypto::sha256(body.password.as_bytes()), &state.admin_password_hash) {
        // Slow down guessing.
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        return Err(ApiError::unauthorized());
    }
    let token = crypto::random_token();
    let expires = db::now() + SESSION_SECS;
    db::insert_session(&lock(&state), &crypto::sha256_hex(token.as_bytes()), expires)?;
    Ok(Json(json!({ "token": token, "expires": expires })))
}

async fn logout(State(state): State<Shared>, headers: HeaderMap) -> ApiResult<StatusCode> {
    if let Some(token) = auth_token(&headers, "Bearer") {
        db::delete_session(&lock(&state), &crypto::sha256_hex(token.as_bytes()))?;
    }
    Ok(StatusCode::NO_CONTENT)
}

fn auth_token<'h>(headers: &'h HeaderMap, scheme: &str) -> Option<&'h str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (s, token) = value.split_once(' ')?;
    (s.eq_ignore_ascii_case(scheme) && !token.is_empty()).then_some(token.trim())
}

async fn require_session(State(state): State<Shared>, req: Request, next: Next) -> Response {
    let valid = match auth_token(req.headers(), "Bearer") {
        Some(token) => db::session_valid(&lock(&state), &crypto::sha256_hex(token.as_bytes())).unwrap_or(false),
        None => false,
    };
    if !valid {
        return ApiError::unauthorized().into_response();
    }
    next.run(req).await
}

// --- Devices ----------------------------------------------------------------

async fn list_devices(State(state): State<Shared>) -> ApiResult<Json<Vec<db::Device>>> {
    Ok(Json(db::list_devices(&lock(&state))?))
}

async fn get_device(State(state): State<Shared>, Path(id): Path<i64>) -> ApiResult<Json<db::Device>> {
    db::get_device(&lock(&state), id)?.map(Json).ok_or_else(ApiError::not_found)
}

#[derive(Deserialize)]
struct CreateDevice {
    name: String,
    #[serde(default = "default_icon")]
    icon: String,
    platform: String,
    /// "static", "pool" or "derived".
    key_mode: String,
    /// Keys in the pool (ignored for static).
    #[serde(default = "default_count")]
    count: u16,
    #[serde(default = "default_interval")]
    interval_s: u32,
    #[serde(default)]
    status: u8,
}

fn default_icon() -> String {
    "tag".into()
}
fn default_count() -> u16 {
    96
}
fn default_interval() -> u32 {
    fm_core::schedule::DEFAULT_INTERVAL_S
}

#[derive(Serialize)]
struct Created {
    device: db::Device,
    /// Shown once: lets a Linux/Windows agent download this device's keys.
    agent_token: String,
}

async fn create_device(State(state): State<Shared>, Json(body): Json<CreateDevice>) -> ApiResult<Json<Created>> {
    let mode = parse_mode(&body.key_mode)?;
    let count = if mode == KeyMode::Static { 1 } else { body.count };
    validate_common(&body.name, &body.platform, count, mode, body.interval_s)?;

    let (seed_enc, keys): (Option<Vec<u8>>, Vec<NewKey>) = if mode == KeyMode::Derived {
        let mut seed = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut seed);
        let keys = (0..count as u32).map(|i| new_key(&state.sealer, &derive_keypair(&seed, i))).collect();
        (Some(state.sealer.seal(&seed)), keys)
    } else {
        (None, (0..count).map(|_| new_key(&state.sealer, &KeyPair::generate(&mut OsRng))).collect())
    };
    let agent_token = crypto::random_token();
    let new = NewDevice {
        name: body.name.trim(),
        icon: &body.icon,
        platform: &body.platform,
        key_mode: &body.key_mode,
        interval_s: body.interval_s,
        status: body.status,
        agent_token_hash: &crypto::sha256_hex(agent_token.as_bytes()),
        seed_enc: seed_enc.as_deref(),
    };
    let mut conn = lock(&state);
    let id = db::insert_device(&mut conn, &new, &keys)?;
    let device = db::get_device(&conn, id)?.ok_or_else(ApiError::not_found)?;
    Ok(Json(Created { device, agent_token }))
}

/// Same shape as `fm keygen`'s key file, so CLI-made keys can move in.
#[derive(Deserialize)]
struct ImportDevice {
    name: String,
    #[serde(default = "default_icon")]
    icon: String,
    #[serde(default = "default_platform")]
    platform: String,
    mode: String,
    interval_s: u32,
    #[serde(default)]
    status: u8,
    /// Derived mode: the seed (hex) the keys come from.
    #[serde(default)]
    seed: Option<String>,
    keys: Vec<ImportKey>,
}

#[derive(Deserialize)]
struct ImportKey {
    private: String,
}

fn default_platform() -> String {
    "esp32".into()
}

async fn import_device(State(state): State<Shared>, Json(body): Json<ImportDevice>) -> ApiResult<Json<Created>> {
    let mode = parse_mode(&body.mode)?;
    let count = u16::try_from(body.keys.len()).map_err(|_| ApiError::bad_request("too many keys"))?;
    validate_common(&body.name, &body.platform, count, mode, body.interval_s)?;

    let mut keys = Vec::with_capacity(body.keys.len());
    for (i, k) in body.keys.iter().enumerate() {
        let bytes: [u8; KEY_LEN] = unhex(&k.private)
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| ApiError::bad_request(format!("key {i}: private key must be {KEY_LEN} hex bytes")))?;
        let kp = KeyPair::from_private_bytes(&bytes).map_err(|e| ApiError::bad_request(format!("key {i}: {e}")))?;
        keys.push(new_key(&state.sealer, &kp));
    }
    let seed_enc = if mode == KeyMode::Derived {
        let seed: [u8; KEY_LEN] = body
            .seed
            .as_deref()
            .and_then(unhex)
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| ApiError::bad_request(format!("derived mode needs a {KEY_LEN}-byte hex seed")))?;
        for (i, k) in keys.iter().enumerate() {
            if derive_keypair(&seed, i as u32).adv_key() != k.adv_key {
                return Err(ApiError::bad_request(format!("key {i} does not derive from the seed")));
            }
        }
        Some(state.sealer.seal(&seed))
    } else {
        None
    };

    let agent_token = crypto::random_token();
    let new = NewDevice {
        name: body.name.trim(),
        icon: &body.icon,
        platform: &body.platform,
        key_mode: &body.mode,
        interval_s: body.interval_s,
        status: body.status,
        agent_token_hash: &crypto::sha256_hex(agent_token.as_bytes()),
        seed_enc: seed_enc.as_deref(),
    };
    let mut conn = lock(&state);
    let id = db::insert_device(&mut conn, &new, &keys).map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            ApiError(StatusCode::CONFLICT, "some of these keys already belong to a device".into())
        } else {
            e.into()
        }
    })?;
    let device = db::get_device(&conn, id)?.ok_or_else(ApiError::not_found)?;
    Ok(Json(Created { device, agent_token }))
}

#[derive(Deserialize)]
struct UpdateDevice {
    name: Option<String>,
    icon: Option<String>,
}

async fn update_device(
    State(state): State<Shared>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateDevice>,
) -> ApiResult<Json<db::Device>> {
    if let Some(n) = &body.name {
        validate_name(n)?;
    }
    let conn = lock(&state);
    if !db::update_device(&conn, id, body.name.as_deref().map(str::trim), body.icon.as_deref())? {
        return Err(ApiError::not_found());
    }
    db::get_device(&conn, id)?.map(Json).ok_or_else(ApiError::not_found)
}

async fn delete_device(State(state): State<Shared>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    if db::delete_device(&lock(&state), id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

async fn rotate_agent_token(State(state): State<Shared>, Path(id): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    let token = crypto::random_token();
    if !db::set_agent_token(&lock(&state), id, &crypto::sha256_hex(token.as_bytes()))? {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({ "agent_token": token })))
}

async fn device_blob(State(state): State<Shared>, Path(id): Path<i64>) -> ApiResult<Response> {
    let bytes = build_blob(&state.sealer, &lock(&state), id)?;
    let name = format!("device-{id}.fmkt");
    Ok(blob_response(bytes, &name))
}

/// For beacon agents (Linux daemon, Windows agent): their own device's blob.
async fn agent_blob(State(state): State<Shared>, headers: HeaderMap) -> ApiResult<Response> {
    let token = auth_token(&headers, "Device").ok_or_else(ApiError::unauthorized)?;
    let conn = lock(&state);
    let id = db::device_by_agent_token(&conn, &crypto::sha256_hex(token.as_bytes()))?.ok_or_else(ApiError::unauthorized)?;
    Ok(blob_response(build_blob(&state.sealer, &conn, id)?, "keys.fmkt"))
}

fn build_blob(sealer: &Sealer, conn: &Connection, id: i64) -> ApiResult<Vec<u8>> {
    let (mode, interval_s, status) = db::blob_params(conn, id)?.ok_or_else(ApiError::not_found)?;
    let keys = db::adv_keys(conn, id)?;
    let built = match parse_mode(&mode)? {
        KeyMode::Derived => {
            let seed = db::seed_enc(conn, id)?
                .and_then(|enc| sealer.open(&enc))
                .and_then(|b| <[u8; KEY_LEN]>::try_from(b).ok())
                .ok_or_else(|| anyhow::anyhow!("device {id} has no readable seed"))?;
            blob::build_derived(interval_s, status, &seed, keys.len() as u16)
        }
        mode => blob::build(mode, interval_s, status, &keys),
    };
    built.map_err(|e| anyhow::anyhow!("building blob: {e}").into())
}

fn blob_response(bytes: Vec<u8>, filename: &str) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\""))
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .expect("valid response")
}

#[derive(Deserialize)]
struct LocationsQuery {
    /// Unix seconds; default: 7 days ago.
    since: Option<i64>,
    limit: Option<u32>,
}

async fn device_locations(
    State(state): State<Shared>,
    Path(id): Path<i64>,
    Query(q): Query<LocationsQuery>,
) -> ApiResult<Json<Vec<Location>>> {
    let conn = lock(&state);
    if db::get_device(&conn, id)?.is_none() {
        return Err(ApiError::not_found());
    }
    let since = q.since.unwrap_or_else(|| db::now() - 7 * 24 * 3600);
    let limit = q.limit.unwrap_or(2000).min(10_000);
    Ok(Json(db::locations(&conn, id, since, limit)?))
}

// --- Reports ----------------------------------------------------------------

/// Raw reports as fetched from Apple by an external fetcher (the server does
/// not fetch itself). The server decrypts and stores them.
#[derive(Deserialize)]
struct IngestBody {
    reports: Vec<IngestReport>,
}

#[derive(Deserialize)]
struct IngestReport {
    /// Base64 SHA256(adv key) the report was fetched under.
    hashed: String,
    /// Base64 report payload (88 or 89 bytes).
    payload: String,
}

#[derive(Serialize, Default, Debug, PartialEq)]
pub struct IngestResult {
    pub stored: u32,
    pub duplicate: u32,
    pub unknown_key: u32,
    pub invalid: u32,
}

async fn ingest_reports(State(state): State<Shared>, Json(body): Json<IngestBody>) -> ApiResult<Json<IngestResult>> {
    let mut result = IngestResult::default();
    let conn = lock(&state);
    for r in &body.reports {
        let Some((device_id, enc)) = db::key_by_hashed(&conn, &r.hashed)? else {
            result.unknown_key += 1;
            continue;
        };
        let decrypted = B64
            .decode(r.payload.trim())
            .ok()
            .zip(state.sealer.open(&enc))
            .and_then(|(payload, priv_bytes)| {
                let priv_bytes: [u8; KEY_LEN] = priv_bytes.try_into().ok()?;
                let kp = KeyPair::from_private_bytes(&priv_bytes).ok()?;
                report::decrypt(&kp, &payload).ok()
            });
        let Some(loc) = decrypted else {
            result.invalid += 1;
            continue;
        };
        let loc = Location {
            ts: loc.timestamp,
            lat: loc.latitude,
            lon: loc.longitude,
            accuracy: loc.accuracy,
            confidence: loc.confidence,
            status: loc.status,
        };
        if db::insert_report(&conn, device_id, &r.hashed, &loc)? {
            result.stored += 1;
        } else {
            result.duplicate += 1;
        }
    }
    Ok(Json(result))
}

/// Every hashed key the server knows, for a fetcher to ask Apple about.
async fn hashed_keys(State(state): State<Shared>) -> ApiResult<Json<Vec<String>>> {
    Ok(Json(db::all_hashed_keys(&lock(&state))?.into_iter().map(|(_, h)| h).collect()))
}

// --- Settings ---------------------------------------------------------------

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Settings {
    /// Stored for an external fetcher to read; the server itself does not use it.
    /// Chosen and run by whoever deploys findmy; see crates/fm-anisette/README.md.
    pub anisette_url: Option<String>,
    /// Minutes between report fetches, for the external fetcher.
    pub fetch_interval_min: Option<u32>,
}

async fn get_settings(State(state): State<Shared>) -> ApiResult<Json<Settings>> {
    let conn = lock(&state);
    Ok(Json(Settings {
        anisette_url: db::get_setting(&conn, "anisette_url")?,
        fetch_interval_min: db::get_setting(&conn, "fetch_interval_min")?.and_then(|v| v.parse().ok()),
    }))
}

async fn put_settings(State(state): State<Shared>, Json(body): Json<Settings>) -> ApiResult<Json<Settings>> {
    let url = body.anisette_url.as_deref().map(str::trim).filter(|u| !u.is_empty());
    if let Some(u) = url {
        if !(u.starts_with("http://") || u.starts_with("https://")) {
            return Err(ApiError::bad_request("anisette_url must start with http:// or https://"));
        }
    }
    if let Some(m) = body.fetch_interval_min {
        if !(5..=1440).contains(&m) {
            return Err(ApiError::bad_request("fetch_interval_min must be 5..=1440"));
        }
    }
    let conn = lock(&state);
    db::set_setting(&conn, "anisette_url", url)?;
    db::set_setting(&conn, "fetch_interval_min", body.fetch_interval_min.map(|m| m.to_string()).as_deref())?;
    Ok(Json(Settings { anisette_url: url.map(String::from), fetch_interval_min: body.fetch_interval_min }))
}

// --- Helpers ----------------------------------------------------------------

fn parse_mode(s: &str) -> ApiResult<KeyMode> {
    match s {
        "static" => Ok(KeyMode::Static),
        "pool" => Ok(KeyMode::Pool),
        "derived" => Ok(KeyMode::Derived),
        _ => Err(ApiError::bad_request("key_mode must be \"static\", \"pool\" or \"derived\"")),
    }
}

fn validate_name(name: &str) -> ApiResult<()> {
    let n = name.trim();
    if n.is_empty() || n.len() > 64 {
        return Err(ApiError::bad_request("name must be 1-64 characters"));
    }
    Ok(())
}

fn validate_common(name: &str, platform: &str, count: u16, mode: KeyMode, interval_s: u32) -> ApiResult<()> {
    validate_name(name)?;
    if !PLATFORMS.contains(&platform) {
        return Err(ApiError::bad_request(format!("platform must be one of {}", PLATFORMS.join(", "))));
    }
    if count == 0 || count > blob::MAX_KEYS {
        return Err(ApiError::bad_request(format!("key count must be 1..={}", blob::MAX_KEYS)));
    }
    if mode == KeyMode::Derived && platform == "esp32" {
        return Err(ApiError::bad_request("the esp32 beacon cannot derive keys on-device; use pool mode"));
    }
    if mode != KeyMode::Static && interval_s < 60 {
        return Err(ApiError::bad_request("interval_s must be at least 60 for pool and derived mode"));
    }
    Ok(())
}

fn new_key(sealer: &Sealer, kp: &KeyPair) -> NewKey {
    NewKey {
        privkey_enc: sealer.seal(&kp.private_bytes()),
        adv_key: kp.adv_key(),
        hashed: B64.encode(kp.hashed_adv_key()),
    }
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn now_rfc3339() -> String {
    let secs = db::now();
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
