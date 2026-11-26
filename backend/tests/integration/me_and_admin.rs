//! `GET`/`PATCH /api/v1/me` (current user and gender) and the `AdminActor`
//! guard (P7: the role is re-read from the database on every admin request).

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use chrono::Duration;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use datenaow::AppState;
use datenaow::modules::admin::presentation::extractors::AdminActor;
use datenaow::modules::auth::application::services::JwtService;
use datenaow::modules::auth::presentation as auth;
use datenaow::modules::users::domain::{Email, UserRepository};
use datenaow::modules::users::infrastructure::PgUserRepository;
use datenaow::shared::actor::Role;

use crate::common::{TestApp, TestResponse, default_start, test_config};

const EMAIL: &str = "an@example.com";
const PASSWORD: &str = "correct horse battery";

/// Registers `EMAIL` and returns its access token.
async fn token_for(app: &TestApp, email: &str) -> String {
    let res = app.register(email, PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
    res.body["access_token"]
        .as_str()
        .expect("access_token is a string")
        .to_owned()
}

async fn login_token(app: &TestApp, email: &str) -> String {
    let res = app
        .post("/api/v1/auth/login")
        .json(&json!({ "email": email, "password": PASSWORD }))
        .send()
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
    res.body["access_token"]
        .as_str()
        .expect("access_token is a string")
        .to_owned()
}

async fn set_role(pool: &PgPool, email: &str, role: Role) {
    let changed = PgUserRepository::new(pool.clone())
        .set_role(&Email::parse(email).expect("valid email"), role)
        .await
        .expect("set_role runs");
    assert!(changed, "user {email} exists");
}

/// A correctly signed token for a user id that has no account.
fn token_for_missing_user(role: Role) -> String {
    JwtService::new(test_config().jwt_secret.as_bytes(), Duration::seconds(900))
        .issue(Uuid::from_u128(42), role, default_start())
        .expect("issue token")
}

async fn get_me(app: &TestApp, token: &str) -> TestResponse {
    app.get("/api/v1/me").bearer(token).send().await
}

async fn patch_me(app: &TestApp, token: &str, body: serde_json::Value) -> TestResponse {
    app.patch("/api/v1/me")
        .bearer(token)
        .json(&body)
        .send()
        .await
}

fn assert_unauthorized(res: &TestResponse) {
    assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "unauthorized", "{}", res.text);
    assert!(res.body["error"]["message"].is_string());
}

// ---- GET /me ----

#[sqlx::test]
async fn me_without_token_returns_401(pool: PgPool) {
    let res = TestApp::new(pool).get("/api/v1/me").send().await;
    assert_unauthorized(&res);
}

#[sqlx::test]
async fn me_with_garbage_bearer_returns_401(pool: PgPool) {
    let app = TestApp::new(pool);

    assert_unauthorized(&get_me(&app, "not-a-jwt").await);
    // Wrong scheme and a bare header are rejected the same way.
    for value in ["Basic abc", "Bearer", "Bearer ", "bearer-token"] {
        let res = app
            .get("/api/v1/me")
            .header("authorization", value)
            .send()
            .await;
        assert_unauthorized(&res);
    }
}

#[sqlx::test]
async fn me_with_expired_token_returns_401(pool: PgPool) {
    let app = TestApp::new(pool);
    let token = token_for(&app, EMAIL).await;

    app.clock.advance(Duration::minutes(15));

    assert_unauthorized(&get_me(&app, &token).await);
}

#[sqlx::test]
async fn me_with_token_signed_by_another_secret_returns_401(pool: PgPool) {
    let app = TestApp::new(pool);
    let token = JwtService::new(
        b"another-secret-that-is-also-32-bytes-long",
        Duration::seconds(900),
    )
    .issue(Uuid::from_u128(1), Role::Admin, default_start())
    .expect("issue token");

    assert_unauthorized(&get_me(&app, &token).await);
}

#[sqlx::test]
async fn me_for_deleted_account_returns_401(pool: PgPool) {
    let app = TestApp::new(pool);
    assert_unauthorized(&get_me(&app, &token_for_missing_user(Role::User)).await);
}

#[sqlx::test]
async fn me_returns_current_user_without_hash(pool: PgPool) {
    let app = TestApp::new(pool);
    let token = token_for(&app, EMAIL).await;

    let res = get_me(&app, &token).await;

    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
    assert_eq!(res.body["email"], "an@example.com");
    assert_eq!(res.body["role"], "user");
    assert!(res.body["gender"].is_null());
    assert!(res.body["id"].is_string());
    assert!(!res.text.contains("password"), "{}", res.text);
    assert!(!res.text.contains("argon2"), "{}", res.text);
}

#[sqlx::test]
async fn me_responses_have_cache_control_no_store(pool: PgPool) {
    let app = TestApp::new(pool);
    let token = token_for(&app, EMAIL).await;

    let ok = get_me(&app, &token).await;
    let patched = patch_me(&app, &token, json!({ "gender": "male" })).await;
    let unauthorized = app.get("/api/v1/me").send().await;
    let invalid = patch_me(&app, &token, json!({ "gender": "robot" })).await;
    let missing = patch_me(&app, &token, json!({})).await;
    let wrong_type = patch_me(&app, &token, json!({ "gender": 7 })).await;
    assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(wrong_type.status, StatusCode::BAD_REQUEST);

    for res in [ok, patched, unauthorized, invalid, missing, wrong_type] {
        assert_eq!(res.headers["cache-control"], "no-store", "{}", res.text);
    }
}

// ---- PATCH /me ----

#[sqlx::test]
async fn patch_me_sets_female_then_clears_with_null(pool: PgPool) {
    let app = TestApp::new(pool);
    let token = token_for(&app, EMAIL).await;

    let a = patch_me(&app, &token, json!({ "gender": "female" })).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.text);
    assert_eq!(a.body["gender"], "female");
    assert_eq!(get_me(&app, &token).await.body["gender"], "female");

    let b = patch_me(&app, &token, json!({ "gender": null })).await;
    assert_eq!(b.status, StatusCode::OK, "{}", b.text);
    assert!(b.body["gender"].is_null());
    assert!(get_me(&app, &token).await.body["gender"].is_null());
}

#[sqlx::test]
async fn patch_me_accepts_every_gender_choice(pool: PgPool) {
    let app = TestApp::new(pool);
    let token = token_for(&app, EMAIL).await;

    for choice in ["male", "female", "other", "prefer_not_to_say"] {
        let res = patch_me(&app, &token, json!({ "gender": choice })).await;
        assert_eq!(res.status, StatusCode::OK, "{choice}: {}", res.text);
        assert_eq!(res.body["gender"], choice);
    }
}

#[sqlx::test]
async fn patch_me_rejects_unknown_gender_with_422(pool: PgPool) {
    let app = TestApp::new(pool);
    let token = token_for(&app, EMAIL).await;
    patch_me(&app, &token, json!({ "gender": "female" })).await;

    for value in ["robot", "Female", ""] {
        let res = patch_me(&app, &token, json!({ "gender": value })).await;
        assert_eq!(
            res.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{value}: {}",
            res.text
        );
        assert_eq!(res.body["error"]["code"], "validation_error");
        assert_eq!(res.body["error"]["fields"]["gender"][0], "invalid_choice");
    }
    assert_eq!(get_me(&app, &token).await.body["gender"], "female");
}

#[sqlx::test]
async fn patch_me_with_empty_body_returns_422_required_and_keeps_gender(pool: PgPool) {
    let app = TestApp::new(pool);
    let token = token_for(&app, EMAIL).await;
    patch_me(&app, &token, json!({ "gender": "other" })).await;

    let res = patch_me(&app, &token, json!({})).await;

    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.text);
    assert_eq!(res.body["error"]["fields"]["gender"][0], "required");
    assert_eq!(get_me(&app, &token).await.body["gender"], "other");
}

#[sqlx::test]
async fn patch_me_with_wrong_type_returns_400(pool: PgPool) {
    let app = TestApp::new(pool);
    let token = token_for(&app, EMAIL).await;

    let res = patch_me(&app, &token, json!({ "gender": 3 })).await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "invalid_json");
}

#[sqlx::test]
async fn patch_me_with_role_field_is_rejected_and_role_unchanged(pool: PgPool) {
    let app = TestApp::new(pool);
    let token = token_for(&app, EMAIL).await;

    let res = patch_me(&app, &token, json!({ "gender": null, "role": "admin" })).await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "invalid_json");
    assert_eq!(get_me(&app, &token).await.body["role"], "user");
}

#[sqlx::test]
async fn patch_me_without_token_returns_401(pool: PgPool) {
    let res = TestApp::new(pool)
        .patch("/api/v1/me")
        .json(&json!({ "gender": "male" }))
        .send()
        .await;
    assert_unauthorized(&res);
}

// ---- AdminActor ----

async fn probe_admin(AdminActor(actor): AdminActor) -> String {
    format!("{} {}", actor.user_id, actor.role)
}

/// The auth routes (to obtain tokens) plus a test-only `GET /probe-admin`.
/// Production has no route that only exists to check permissions.
fn admin_probe_app(pool: PgPool) -> TestApp {
    let routes: Router<AppState> = Router::new()
        .nest("/api/v1/auth", auth::routes())
        .route("/probe-admin", get(probe_admin));
    TestApp::with_routes(pool, test_config(), routes)
}

async fn probe(app: &TestApp, token: Option<&str>) -> TestResponse {
    let request = app.get("/probe-admin");
    match token {
        Some(token) => request.bearer(token).send().await,
        None => request.send().await,
    }
}

#[sqlx::test]
async fn admin_guard_allows_admin(pool: PgPool) {
    let app = admin_probe_app(pool.clone());
    token_for(&app, EMAIL).await;
    set_role(&pool, EMAIL, Role::Admin).await;
    let token = login_token(&app, EMAIL).await;

    let res = probe(&app, Some(&token)).await;

    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
    assert!(res.text.ends_with(" admin"), "{}", res.text);
}

#[sqlx::test]
async fn admin_guard_uses_db_role_not_token_role(pool: PgPool) {
    let app = admin_probe_app(pool.clone());
    // The token was issued while the account was a plain user.
    let token = token_for(&app, EMAIL).await;
    set_role(&pool, EMAIL, Role::Admin).await;

    let res = probe(&app, Some(&token)).await;

    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
}

#[sqlx::test]
async fn admin_guard_rejects_user_with_403(pool: PgPool) {
    let app = admin_probe_app(pool);
    let token = token_for(&app, EMAIL).await;

    let res = probe(&app, Some(&token)).await;

    assert_eq!(res.status, StatusCode::FORBIDDEN, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "forbidden");
}

#[sqlx::test]
async fn admin_guard_rejects_anonymous_with_401(pool: PgPool) {
    let app = admin_probe_app(pool);

    assert_unauthorized(&probe(&app, None).await);
    assert_unauthorized(&probe(&app, Some("garbage")).await);
}

#[sqlx::test]
async fn admin_guard_rejects_missing_account_with_401(pool: PgPool) {
    let app = admin_probe_app(pool);

    let res = probe(&app, Some(&token_for_missing_user(Role::Admin))).await;

    assert_unauthorized(&res);
}

#[sqlx::test]
async fn admin_guard_rejects_demoted_admin_even_with_old_token(pool: PgPool) {
    let app = admin_probe_app(pool.clone());
    token_for(&app, EMAIL).await;
    set_role(&pool, EMAIL, Role::Admin).await;
    let admin_token = login_token(&app, EMAIL).await;
    assert_eq!(probe(&app, Some(&admin_token)).await.status, StatusCode::OK);

    set_role(&pool, EMAIL, Role::User).await;

    let res = probe(&app, Some(&admin_token)).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "forbidden");
}

// ---- OpenAPI ----

#[sqlx::test(migrations = false)]
async fn openapi_documents_me_with_bearer_security(pool: PgPool) {
    let res = TestApp::new(pool).get("/api/v1/openapi.json").send().await;

    let me = &res.body["paths"]["/api/v1/me"];
    assert!(me["get"].is_object(), "{}", res.text);
    assert!(me["patch"].is_object(), "{}", res.text);
    assert_eq!(
        res.body["components"]["securitySchemes"]["bearer_auth"]["scheme"],
        "bearer"
    );
    assert!(me["get"]["security"].is_array(), "{}", res.text);
}
