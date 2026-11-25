use axum::http::StatusCode;
use sqlx::PgPool;

use crate::common::TestApp;

#[sqlx::test(migrations = false)]
async fn openapi_json_is_served(pool: PgPool) {
    let res = TestApp::new(pool).get("/api/v1/openapi.json").send().await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body["openapi"].is_string(), "body: {}", res.text);
    assert!(res.body["paths"]["/api/v1/health"].is_object());
    assert!(res.body["paths"]["/api/v1/health"]["get"].is_object());
}
