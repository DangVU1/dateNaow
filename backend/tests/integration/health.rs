use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::common::TestApp;

#[sqlx::test(migrations = false)]
async fn health_returns_ok_when_database_reachable(pool: PgPool) {
    let app = TestApp::new(pool);
    let res = app.get("/api/v1/health").send().await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body, json!({"status": "ok", "database": "ok"}));
}

#[sqlx::test(migrations = false)]
async fn every_response_has_request_id_header(pool: PgPool) {
    let res = TestApp::new(pool).get("/api/v1/health").send().await;
    assert!(res.headers.contains_key("x-request-id"));
}

#[sqlx::test(migrations = false)]
async fn health_returns_503_when_database_unavailable(pool: PgPool) {
    pool.close().await;
    let res = TestApp::new(pool).get("/api/v1/health").send().await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        res.body,
        json!({"status": "degraded", "database": "unavailable"})
    );
}
