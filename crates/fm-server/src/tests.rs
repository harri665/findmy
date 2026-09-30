//! API tests through the real router with an in-memory database.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use fm_core::blob::Blob;
use fm_core::report::{seal, Location};
use http_body_util::BodyExt;
use rand_core::OsRng;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::api::{self, AppState};
use crate::{crypto, db};

const PASSWORD: &str = "correct horse battery";

fn test_state() -> Arc<AppState> {
    Arc::new(AppState {
        db: Mutex::new(db::open_in_memory().unwrap()),
        sealer: crypto::Sealer::new(&[42; 32]),
        admin_password_hash: crypto::sha256(PASSWORD.as_bytes()),
        apple: fm_apple::Client::new().unwrap(),
        fetch_lock: tokio::sync::Mutex::new(()),
        fetch_notify: tokio::sync::Notify::new(),
    })
}

fn app() -> Router {
    api::router(test_state())
}

async fn call(app: &Router, method: Method, uri: &str, auth: Option<&str>, body: Option<Value>) -> (StatusCode, Vec<u8>) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(a) = auth {
        req = req.header(header::AUTHORIZATION, a);
    }
    let req = match body {
        Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    (status, resp.into_body().collect().await.unwrap().to_bytes().to_vec())
}

fn json_of(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

async fn login(app: &Router) -> String {
    let (s, b) = call(app, Method::POST, "/api/login", None, Some(json!({ "password": PASSWORD }))).await;
    assert_eq!(s, StatusCode::OK);
    format!("Bearer {}", json_of(&b)["token"].as_str().unwrap())
}

#[tokio::test]
async fn health_is_public() {
    let (s, b) = call(&app(), Method::GET, "/api/health", None, None).await;
    assert_eq!(s, StatusCode::OK);
    let v = json_of(&b);
    assert_eq!(v["status"], "ok");
    assert!(v["timestamp"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn auth_required_and_enforced() {
    let app = app();
    assert_eq!(call(&app, Method::GET, "/api/devices", None, None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, Method::GET, "/api/devices", Some("Bearer nope"), None).await.0, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&app, Method::POST, "/api/login", None, Some(json!({ "password": "wrong" }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let token = login(&app).await;
    assert_eq!(call(&app, Method::GET, "/api/devices", Some(&token), None).await.0, StatusCode::OK);
    assert_eq!(call(&app, Method::POST, "/api/logout", Some(&token), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, Method::GET, "/api/devices", Some(&token), None).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_device_blob_and_agent() {
    let app = app();
    let auth = login(&app).await;
    let (s, b) = call(
        &app,
        Method::POST,
        "/api/devices",
        Some(&auth),
        Some(json!({ "name": "Bike", "platform": "esp32", "key_mode": "pool", "count": 5, "interval_s": 900 })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let created = json_of(&b);
    let id = created["device"]["id"].as_i64().unwrap();
    assert_eq!(created["device"]["key_count"], 5);
    let agent = created["agent_token"].as_str().unwrap().to_string();

    // Blob via dashboard and via agent token are the same, valid FMKT blob.
    let (s, blob) = call(&app, Method::GET, &format!("/api/devices/{id}/blob"), Some(&auth), None).await;
    assert_eq!(s, StatusCode::OK);
    let parsed = Blob::parse(&blob).unwrap();
    assert_eq!(parsed.key_count(), 5);
    assert_eq!(parsed.interval_s, 900);

    let (s, agent_blob) = call(&app, Method::GET, "/api/agent/blob", Some(&format!("Device {agent}")), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(agent_blob, blob);
    assert_eq!(call(&app, Method::GET, "/api/agent/blob", Some("Device wrong"), None).await.0, StatusCode::UNAUTHORIZED);
    // A session token is not an agent token.
    assert_eq!(call(&app, Method::GET, "/api/agent/blob", Some(&auth), None).await.0, StatusCode::UNAUTHORIZED);

    // Rotating the agent token invalidates the old one.
    let (_, b) = call(&app, Method::POST, &format!("/api/devices/{id}/agent-token"), Some(&auth), None).await;
    let new_agent = json_of(&b)["agent_token"].as_str().unwrap().to_string();
    assert_eq!(call(&app, Method::GET, "/api/agent/blob", Some(&format!("Device {agent}")), None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, Method::GET, "/api/agent/blob", Some(&format!("Device {new_agent}")), None).await.0, StatusCode::OK);

    // Rename, then delete.
    let (s, b) = call(&app, Method::PATCH, &format!("/api/devices/{id}"), Some(&auth), Some(json!({ "name": "Red bike" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json_of(&b)["name"], "Red bike");
    assert_eq!(call(&app, Method::DELETE, &format!("/api/devices/{id}"), Some(&auth), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, Method::GET, &format!("/api/devices/{id}"), Some(&auth), None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rejects_bad_device_requests() {
    let app = app();
    let auth = login(&app).await;
    for body in [
        json!({ "name": "", "platform": "esp32", "key_mode": "pool" }),
        json!({ "name": "x", "platform": "toaster", "key_mode": "pool" }),
        json!({ "name": "x", "platform": "esp32", "key_mode": "derived" }),
        json!({ "name": "x", "platform": "linux", "key_mode": "bogus" }),
        json!({ "name": "x", "platform": "esp32", "key_mode": "pool", "count": 0 }),
        json!({ "name": "x", "platform": "esp32", "key_mode": "pool", "count": 5000 }),
        json!({ "name": "x", "platform": "esp32", "key_mode": "pool", "interval_s": 10 }),
    ] {
        let (s, _) = call(&app, Method::POST, "/api/devices", Some(&auth), Some(body.clone())).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
async fn import_ingest_and_locations() {
    let app = app();
    let auth = login(&app).await;

    // Import the fm-core test vector key (docs/protocol.md section 6).
    let import = json!({
        "name": "Vector", "mode": "static", "interval_s": 0, "status": 64,
        "keys": [{ "private": "6bea390c3ab9532df5ace692b52d00a82b55145742c45b4e372abf3f" }]
    });
    let (s, b) = call(&app, Method::POST, "/api/devices/import", Some(&auth), Some(import.clone())).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let id = json_of(&b)["device"]["id"].as_i64().unwrap();
    // Importing the same key twice is a conflict.
    assert_eq!(call(&app, Method::POST, "/api/devices/import", Some(&auth), Some(import)).await.0, StatusCode::CONFLICT);

    let (_, b) = call(&app, Method::GET, "/api/keys/hashed", Some(&auth), None).await;
    let hashed = json_of(&b)[0].as_str().unwrap().to_string();
    assert_eq!(hashed, "mznwvrP22ZlLBNL2odcKnnAkJF2Z4g+BB8fVdyxGH9Y=");

    // The independent vector report, plus one we seal now like a finder would.
    let vector = "306173000204394a7c1c259b57d1d63fbe71ef7937a9e15ac2b06f38fe79f7d71fb1aa5c488768cf511b199b80a53d4ba60c3dc71b8a6bfae0e6511441ee46207fb94b5aeae6c1ef5fd80eec651d009e64d54d6158ec02cd";
    let vector: Vec<u8> = (0..vector.len()).step_by(2).map(|i| u8::from_str_radix(&vector[i..i + 2], 16).unwrap()).collect();
    let adv: [u8; 28] = B64.decode("iUDAoeAmskpaUsbou1w5gSMd1v3nEGd88tD/1A==").unwrap().try_into().unwrap();
    let later = Location { timestamp: 1_790_003_600, confidence: 3, latitude: 40.02, longitude: -105.25, accuracy: 20, status: 0x40 };
    let sealed = seal(&adv, &p224::SecretKey::random(&mut OsRng), &later).unwrap();

    let body = json!({ "reports": [
        { "hashed": hashed, "payload": B64.encode(&vector) },
        { "hashed": hashed, "payload": B64.encode(&sealed) },
        { "hashed": hashed, "payload": B64.encode(&vector) },
        { "hashed": "bm90LW91cnM=", "payload": B64.encode(&vector) },
        { "hashed": hashed, "payload": "not base64!" },
    ]});
    let (s, b) = call(&app, Method::POST, "/api/reports", Some(&auth), Some(body)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json_of(&b), json!({ "stored": 2, "duplicate": 1, "unknown_key": 1, "invalid": 1 }));

    let (_, b) = call(&app, Method::GET, &format!("/api/devices/{id}/locations?since=0"), Some(&auth), None).await;
    let locs = json_of(&b);
    assert_eq!(locs.as_array().unwrap().len(), 2);
    assert_eq!(locs[0]["ts"], 1_790_000_000);
    assert_eq!(locs[0]["lat"], 40.0149856);
    assert_eq!(locs[1]["ts"], 1_790_003_600);

    let (_, b) = call(&app, Method::GET, &format!("/api/devices/{id}"), Some(&auth), None).await;
    let dev = json_of(&b);
    assert_eq!(dev["report_count"], 2);
    assert_eq!(dev["last"]["ts"], 1_790_003_600);
}

#[tokio::test]
async fn private_keys_are_encrypted_at_rest() {
    let conn = db::open_in_memory().unwrap();
    let state = Arc::new(AppState {
        db: Mutex::new(conn),
        sealer: crypto::Sealer::new(&[42; 32]),
        admin_password_hash: crypto::sha256(PASSWORD.as_bytes()),
        apple: fm_apple::Client::new().unwrap(),
        fetch_lock: tokio::sync::Mutex::new(()),
        fetch_notify: tokio::sync::Notify::new(),
    });
    let app = api::router(state.clone());
    let auth = login(&app).await;
    let import = json!({
        "name": "Vector", "mode": "static", "interval_s": 0,
        "keys": [{ "private": "6bea390c3ab9532df5ace692b52d00a82b55145742c45b4e372abf3f" }]
    });
    call(&app, Method::POST, "/api/devices/import", Some(&auth), Some(import)).await;

    let conn = state.db.lock().unwrap();
    let stored: Vec<u8> = conn.query_row("SELECT privkey_enc FROM keys", [], |r| r.get(0)).unwrap();
    let raw = [0x6b, 0xea, 0x39, 0x0c, 0x3a, 0xb9];
    assert!(!stored.windows(raw.len()).any(|w| w == raw), "private key stored in the clear");
    assert_eq!(state.sealer.open(&stored).unwrap()[..6], raw);
}

#[tokio::test]
async fn settings_round_trip_and_validation() {
    let app = app();
    let auth = login(&app).await;
    let (_, b) = call(&app, Method::GET, "/api/settings", Some(&auth), None).await;
    assert_eq!(json_of(&b), json!({ "anisette_url": null, "fetch_interval_min": null }));

    let body = json!({ "anisette_url": "http://anisette:6969", "fetch_interval_min": 15 });
    let (s, _) = call(&app, Method::PUT, "/api/settings", Some(&auth), Some(body.clone())).await;
    assert_eq!(s, StatusCode::OK);
    let (_, b) = call(&app, Method::GET, "/api/settings", Some(&auth), None).await;
    assert_eq!(json_of(&b), body);

    for bad in [json!({ "anisette_url": "ftp://x" }), json!({ "fetch_interval_min": 1 })] {
        assert_eq!(call(&app, Method::PUT, "/api/settings", Some(&auth), Some(bad)).await.0, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn derived_device_blob_carries_seed_not_keys() {
    let app = app();
    let auth = login(&app).await;
    let (s, b) = call(
        &app,
        Method::POST,
        "/api/devices",
        Some(&auth),
        Some(json!({ "name": "Tag", "platform": "linux", "key_mode": "derived", "count": 12, "interval_s": 900 })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
    let id = json_of(&b)["device"]["id"].as_i64().unwrap();

    let (_, bytes) = call(&app, Method::GET, &format!("/api/devices/{id}/blob"), Some(&auth), None).await;
    let blob = Blob::parse(&bytes).unwrap();
    assert_eq!(blob.mode, fm_core::schedule::KeyMode::Derived);
    assert_eq!(blob.key_count(), 12);
    assert_eq!(bytes.len(), 14 + 28 + 4);

    // Every key the beacon derives is one the server holds a private key for.
    let (_, h) = call(&app, Method::GET, "/api/keys/hashed", Some(&auth), None).await;
    let known: Vec<String> = serde_json::from_slice(&h).unwrap();
    for i in 0..12 {
        let hashed = B64.encode(fm_core::keys::hashed_adv_key(&blob.adv_key(i)));
        assert!(known.contains(&hashed), "derived key {i} unknown to server");
    }

    // Import: a seed that doesn't produce the keys is refused.
    let seed = blob.seed().unwrap();
    let key = fm_core::derive::derive_keypair(&seed, 0);
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let import = |seed: &[u8]| {
        json!({ "name": "Imp", "mode": "derived", "interval_s": 900, "seed": hex(seed),
                "keys": [{ "private": hex(&key.private_bytes()) }] })
    };
    let (s, _) = call(&app, Method::POST, "/api/devices/import", Some(&auth), Some(import(&[1u8; 28]))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

fn apple_session() -> Value {
    json!({"dsid":"12345", "searchPartyToken":"private-report-token", "uid":"user-id", "device_id":"device-id"})
}

#[tokio::test]
async fn apple_session_is_encrypted_and_never_returned() {
    let state = test_state();
    let app = api::router(state.clone());
    for uri in ["/api/apple", "/api/reports/fetch"] {
        assert_eq!(call(&app, Method::POST, uri, None, None).await.0, StatusCode::UNAUTHORIZED);
    }
    let auth = login(&app).await;
    assert_eq!(call(&app, Method::POST, "/api/reports/fetch", Some(&auth), None).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call(&app, Method::PUT, "/api/apple", Some(&auth), Some(json!({"dsid":"x"}))).await.0, StatusCode::BAD_REQUEST);
    let (s, b) = call(&app, Method::PUT, "/api/apple", Some(&auth), Some(apple_session())).await;
    assert_eq!(s, StatusCode::OK);
    let status = json_of(&b);
    assert_eq!(status["connected"], true);
    assert_eq!(status["configured"], false);
    assert_eq!(status["running"], false);
    assert!(!String::from_utf8_lossy(&b).contains("private-report-token"));
    {
        let conn = state.db.lock().unwrap();
        let stored = db::get_setting(&conn, "apple_session").unwrap().unwrap();
        assert!(!stored.contains("private-report-token"));
        let clear = state.sealer.open(&B64.decode(stored).unwrap()).unwrap();
        assert_eq!(json_of(&clear), apple_session());
    }
    let (s, _) = call(&app, Method::DELETE, "/api/apple", Some(&auth), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(db::get_setting(&state.db.lock().unwrap(), "apple_session").unwrap().is_none());
}

#[tokio::test]
async fn manual_fetch_records_status_and_rejects_concurrent_changes() {
    let state = test_state();
    let app = api::router(state.clone());
    let auth = login(&app).await;
    call(&app, Method::PUT, "/api/settings", Some(&auth), Some(json!({"anisette_url":"http://unused-provider:6969", "fetch_interval_min":5}))).await;
    call(&app, Method::PUT, "/api/apple", Some(&auth), Some(apple_session())).await;
    // No devices: there are no keys to request, and no external calls occur.
    let (s, b) = call(&app, Method::POST, "/api/reports/fetch", Some(&auth), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json_of(&b)["stored"], 0);
    let status = crate::fetch::status(&state).unwrap();
    assert!(status.last_success.is_some());
    assert_eq!(status.next_fetch.unwrap() - status.last_fetch.unwrap(), 300);
    let guard = state.fetch_lock.lock().await;
    for (method, uri, body) in [(Method::POST, "/api/reports/fetch", None), (Method::PUT, "/api/apple", Some(apple_session())), (Method::DELETE, "/api/apple", None)] {
        assert_eq!(call(&app, method, uri, Some(&auth), body).await.0, StatusCode::CONFLICT);
    }
    drop(guard);
    call(&app, Method::PUT, "/api/apple", Some(&auth), Some(apple_session())).await;
    let status = crate::fetch::status(&state).unwrap();
    assert!(status.last_success.is_none());
    assert!(status.result.is_none());
}

#[tokio::test]
async fn provider_failure_is_visible_and_does_not_change_history() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/", axum::routing::get(|| async { axum::Json(json!({})) }))).await.unwrap();
    });
    let state = test_state();
    let app = api::router(state.clone());
    let auth = login(&app).await;
    call(&app, Method::POST, "/api/devices", Some(&auth), Some(json!({"name":"Tag", "platform":"linux", "key_mode":"static"}))).await;
    call(&app, Method::PUT, "/api/settings", Some(&auth), Some(json!({"anisette_url":provider}))).await;
    call(&app, Method::PUT, "/api/apple", Some(&auth), Some(apple_session())).await;
    let (s, b) = call(&app, Method::POST, "/api/reports/fetch", Some(&auth), None).await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert!(json_of(&b)["error"].as_str().unwrap().contains("missing X-Apple-I-MD"));
    let status = crate::fetch::status(&state).unwrap();
    assert!(status.error.unwrap().contains("missing X-Apple-I-MD"));
    assert!(status.last_success.is_none());
    assert!(!status.running);
    let count: i64 = state.db.lock().unwrap().query_row("SELECT COUNT(*) FROM reports", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
    task.abort();
}

#[tokio::test]
async fn scheduler_wakes_after_session_import_and_respects_interval() {
    let state = test_state();
    let app = api::router(state.clone());
    let auth = login(&app).await;
    let worker = tokio::spawn(crate::fetch::worker(state.clone()));
    call(&app, Method::PUT, "/api/settings", Some(&auth), Some(json!({"anisette_url":"http://unused:6969"}))).await;
    call(&app, Method::PUT, "/api/apple", Some(&auth), Some(apple_session())).await;
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while crate::fetch::status(&state).unwrap().last_success.is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    let first = crate::fetch::status(&state).unwrap().last_fetch;
    state.fetch_notify.notify_one();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(crate::fetch::status(&state).unwrap().last_fetch, first);
    worker.abort();
}

#[tokio::test]
async fn unreadable_session_records_failure_and_delays_retry() {
    let state = test_state();
    {
        let conn = state.db.lock().unwrap();
        db::set_setting(&conn, "anisette_url", Some("http://unused:6969")).unwrap();
        db::set_setting(&conn, "apple_session", Some(&B64.encode(b"corrupt session"))).unwrap();
    }
    assert!(matches!(crate::fetch::run(&state).await, Err(crate::fetch::FetchError::Failed(_))));
    let status = crate::fetch::status(&state).unwrap();
    assert!(status.error.unwrap().contains("check the master key"));
    assert!(status.last_success.is_none());
    assert!(status.next_fetch.unwrap() > db::now());
}
