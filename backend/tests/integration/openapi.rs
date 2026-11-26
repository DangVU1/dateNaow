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

#[sqlx::test(migrations = false)]
async fn openapi_documents_password_reset(pool: PgPool) {
    let res = TestApp::new(pool).get("/api/v1/openapi.json").send().await;
    for path in [
        "/api/v1/auth/password-reset/request",
        "/api/v1/auth/password-reset/confirm",
    ] {
        assert!(
            res.body["paths"][path]["post"].is_object(),
            "{path} missing"
        );
    }
    for schema in ["PasswordResetRequest", "PasswordResetConfirmRequest"] {
        assert!(
            res.body["components"]["schemas"][schema].is_object(),
            "{schema} missing"
        );
    }
}

#[sqlx::test(migrations = false)]
async fn openapi_lists_all_plan_1_paths(pool: PgPool) {
    let res = TestApp::new(pool).get("/api/v1/openapi.json").send().await;
    assert_eq!(res.status, StatusCode::OK);
    for (path, method) in [
        ("/api/v1/health", "get"),
        ("/api/v1/auth/register", "post"),
        ("/api/v1/auth/login", "post"),
        ("/api/v1/auth/refresh", "post"),
        ("/api/v1/auth/logout", "post"),
        ("/api/v1/auth/password-reset/request", "post"),
        ("/api/v1/auth/password-reset/confirm", "post"),
        ("/api/v1/me", "get"),
        ("/api/v1/me", "patch"),
    ] {
        assert!(
            res.body["paths"][path][method].is_object(),
            "missing {method} {path}"
        );
    }
}
