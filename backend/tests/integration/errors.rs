//! The JSON error envelope as seen over HTTP: `ValidatedJson` rejections,
//! the body limit, the request timeout and unknown routes.

use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::{get, post};
use datenaow::AppState;
use datenaow::shared::extract::ValidatedJson;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use validator::Validate;

use crate::common::{TestApp, lazy_pool, test_config};

#[derive(Debug, Deserialize, Validate)]
struct Probe {
    #[validate(length(min = 3))]
    name: String,
}

async fn probe(ValidatedJson(probe): ValidatedJson<Probe>) -> String {
    probe.name
}

async fn slow() -> StatusCode {
    tokio::time::sleep(Duration::from_secs(5)).await;
    StatusCode::OK
}

fn probe_routes() -> Router<AppState> {
    Router::new()
        .route("/probe", post(probe))
        .route("/slow", get(slow))
}

fn probe_app() -> TestApp {
    TestApp::with_routes(lazy_pool(), test_config(), probe_routes())
}

fn assert_error_envelope(body: &serde_json::Value, code: &str) {
    assert_eq!(body["error"]["code"], code, "body: {body}");
    assert!(body["error"]["message"].is_string(), "body: {body}");
}

#[tokio::test]
async fn valid_body_reaches_the_handler() {
    let res = probe_app()
        .post("/probe")
        .json(&json!({"name": "Mai"}))
        .send()
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.text, "Mai");
}

#[tokio::test]
async fn malformed_json_returns_400_invalid_json() {
    let res = probe_app()
        .post("/probe")
        .header("content-type", "application/json")
        .body("{")
        .send()
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_error_envelope(&res.body, "invalid_json");
}

#[tokio::test]
async fn wrong_field_type_returns_400_invalid_json() {
    let res = probe_app()
        .post("/probe")
        .json(&json!({"name": 5}))
        .send()
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_error_envelope(&res.body, "invalid_json");
}

#[tokio::test]
async fn missing_required_field_returns_400_invalid_json() {
    let res = probe_app().post("/probe").json(&json!({})).send().await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_error_envelope(&res.body, "invalid_json");
}

#[tokio::test]
async fn missing_content_type_returns_400_invalid_json() {
    let res = probe_app()
        .post("/probe")
        .body(r#"{"name":"Mai"}"#)
        .send()
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_error_envelope(&res.body, "invalid_json");
}

#[tokio::test]
async fn failed_validation_returns_422_with_field_codes() {
    let res = probe_app()
        .post("/probe")
        .json(&json!({"name": "ab"}))
        .send()
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_error_envelope(&res.body, "validation_error");
    assert_eq!(res.body["error"]["fields"]["name"][0], "length");
}

#[tokio::test]
async fn body_over_64_kib_with_content_length_returns_413_json() {
    let body = format!(r#"{{"name":"{}"}}"#, "a".repeat(70_000));
    let res = probe_app()
        .post("/probe")
        .header("content-type", "application/json")
        // Browsers always send Content-Length; the limit must still answer in JSON.
        .header("content-length", &body.len().to_string())
        .body(body)
        .send()
        .await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_error_envelope(&res.body, "payload_too_large");
}

#[tokio::test]
async fn slow_handler_returns_408_json() {
    let mut config = test_config();
    config.request_timeout = Duration::from_millis(50);
    let app = TestApp::with_routes(lazy_pool(), config, probe_routes());

    let res = app.get("/slow").send().await;
    assert_eq!(res.status, StatusCode::REQUEST_TIMEOUT);
    assert_error_envelope(&res.body, "request_timeout");
    // Errors produced by middleware still carry the request id.
    assert!(res.headers.contains_key("x-request-id"));
}

#[sqlx::test(migrations = false)]
async fn unknown_api_route_returns_404_json(pool: PgPool) {
    let res = TestApp::new(pool).get("/api/v1/no-such-route").send().await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_error_envelope(&res.body, "not_found");
}

#[sqlx::test(migrations = false)]
async fn wrong_method_returns_405(pool: PgPool) {
    let res = TestApp::new(pool).post("/api/v1/health").send().await;
    assert_eq!(res.status, StatusCode::METHOD_NOT_ALLOWED);
}
