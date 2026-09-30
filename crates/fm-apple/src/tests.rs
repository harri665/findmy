use super::*;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use std::sync::{Arc, Mutex};

fn credentials() -> Credentials {
    Credentials {
        dsid: "12345".into(),
        search_party_token: "private-token".into(),
        uid: "user-id".into(),
        device_id: "device-id".into(),
    }
}

#[test]
fn imports_only_required_fields_and_rejects_incomplete_sessions() {
    let export = json!({"type":"account", "account":{"password":"must-not-be-retained"}, "ids":{"uid":"u","devid":"d"},
        "login":{"data":{"dsid":12345,"mobileme_data":{"tokens":{"searchPartyToken":"token"}}}}});
    let normalized = Credentials::from_export(&export).unwrap();
    assert_eq!(normalized.dsid, "12345");
    assert!(!serde_json::to_string(&normalized)
        .unwrap()
        .contains("must-not-be-retained"));
    assert!(Credentials::from_export(&json!({"dsid":"1"})).is_err());
    let mut c = credentials();
    c.dsid = "user:password".into();
    assert!(c.validate().is_err());
}

#[test]
fn parses_reports_and_rejects_service_errors() {
    let id = B64.encode([7; 32]);
    let response = json!({"acsnLocations":{"statusCode":"200", "locationPayload":[{"id":id, "locationInfo":["one","two"]}]}});
    let reports = parse_response(&serde_json::to_vec(&response).unwrap()).unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].hashed, id);
    for invalid in [
        json!({}),
        json!({"acsnLocations":{"statusCode":"401"}}),
        json!({"acsnLocations":{"statusCode":"200"}}),
    ] {
        assert!(parse_response(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    assert!(parse_response(b" ").is_err());
}

#[tokio::test]
async fn real_http_flow_separates_provider_and_apple_auth() {
    let seen = Arc::new(Mutex::new(None));
    async fn provider(headers: HeaderMap) -> Json<Value> {
        assert!(!headers.contains_key("authorization"));
        Json(json!({"X-Apple-I-MD":"otp", "X-Apple-I-MD-M":"machine", "Authorization":"evil"}))
    }
    async fn reports(
        State(seen): State<Arc<Mutex<Option<Value>>>>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(
            headers["authorization"],
            format!("Basic {}", B64.encode("12345:private-token"))
        );
        assert_eq!(headers["x-apple-i-md"], "otp");
        assert_eq!(headers["x-mme-device-id"], "DEVICE-ID");
        *seen.lock().unwrap() = Some(body);
        Json(json!({"acsnLocations":{"statusCode":"200", "locationPayload":[]}}))
    }
    let app = Router::new()
        .route("/provider", get(provider))
        .route("/fetch", post(reports))
        .route("/expired", post(|| async { StatusCode::UNAUTHORIZED }))
        .route(
            "/redirect",
            post(|| async { (StatusCode::FOUND, [("location", "/fetch")]) }),
        )
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let mut client = Client {
        http: fm_anisette::http_client().unwrap(),
        endpoint: format!("{origin}/fetch"),
    };
    let groups = vec![
        vec![B64.encode([1; 32]), B64.encode([2; 32])],
        vec![B64.encode([3; 32])],
    ];
    assert!(client
        .fetch(
            &credentials(),
            &format!("{origin}/provider"),
            &groups,
            1_790_000_000,
            "2026-09-21T00:00:00Z"
        )
        .await
        .unwrap()
        .is_empty());
    let body = seen.lock().unwrap().clone().unwrap();
    assert_eq!(body["fetch"].as_array().unwrap().len(), 2);
    assert_eq!(body["fetch"][0]["endDate"], 1_790_000_000_000i64);
    assert_eq!(
        body["fetch"][0]["startDate"],
        (1_790_000_000i64 - 7 * 86400) * 1000
    );
    assert_eq!(body["fetch"][0]["primaryIds"], json!(groups[0]));
    client.endpoint = format!("{origin}/expired");
    let error = client
        .fetch(
            &credentials(),
            &format!("{origin}/provider"),
            &groups,
            1_790_000_000,
            "time",
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("sign in again"));
    assert!(!error.contains("private-token"));
    client.endpoint = format!("{origin}/redirect");
    assert!(client
        .fetch(
            &credentials(),
            &format!("{origin}/provider"),
            &groups,
            1_790_000_000,
            "time"
        )
        .await
        .is_err());
    task.abort();
}
