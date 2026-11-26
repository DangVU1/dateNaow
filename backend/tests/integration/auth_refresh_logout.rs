//! Refresh tokens: the PostgreSQL repository, `SessionService::start`, and
//! the `POST /api/v1/auth/refresh` and `POST /api/v1/auth/logout` endpoints.

use std::sync::Arc;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use datenaow::modules::auth::application::AuthError;
use datenaow::modules::auth::application::services::{JwtService, SessionService, TokenError};
use datenaow::modules::auth::domain::{
    OpaqueToken, RefreshRepoError, RefreshTokenRecord, RefreshTokenRepository,
};
use datenaow::modules::auth::infrastructure::PgRefreshTokenRepository;
use datenaow::modules::users::domain::{Email, NewUser, User, UserRepository};
use datenaow::modules::users::infrastructure::PgUserRepository;
use datenaow::shared::actor::Role;
use datenaow::shared::clock::Clock;
use tokio::task::JoinSet;

use crate::common::{FakeClock, TestApp, TestResponse, default_start, test_config};

/// The stored password hash of every user created by `create_user`.
const PASSWORD_HASH: &str = "not-a-real-hash";

async fn create_user(pool: &PgPool, email: &str) -> User {
    PgUserRepository::new(pool.clone())
        .create(NewUser {
            email: Email::parse(email).expect("valid test email"),
            password_hash: PASSWORD_HASH.to_owned(),
        })
        .await
        .expect("create user")
}

fn hash_of(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn t0() -> DateTime<Utc> {
    default_start()
}

async fn find(repo: &PgRefreshTokenRepository, hash: [u8; 32]) -> RefreshTokenRecord {
    repo.find_by_hash(hash)
        .await
        .expect("query runs")
        .expect("token exists")
}

// ---- repository ----

#[sqlx::test]
async fn insert_then_find_by_hash_returns_active_record(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool);
    let expires = t0() + Duration::days(30);

    let inserted = repo
        .insert(user.id, PASSWORD_HASH, hash_of(1), expires)
        .await
        .expect("insert");
    let found = find(&repo, hash_of(1)).await;

    assert_eq!(found, inserted);
    assert_eq!(found.user_id, user.id);
    assert_eq!(found.expires_at, expires);
    assert_eq!(found.revoked_at, None);
    assert_eq!(found.replaced_at, None);
    assert!(
        repo.find_by_hash(hash_of(2))
            .await
            .expect("query")
            .is_none()
    );
}

#[sqlx::test]
async fn rotate_marks_old_revoked_and_inserts_new(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool);
    let old = repo
        .insert(
            user.id,
            PASSWORD_HASH,
            hash_of(1),
            t0() + Duration::days(30),
        )
        .await
        .expect("insert");
    let now = t0() + Duration::hours(1);
    let new_expires = now + Duration::days(30);

    let new = repo
        .rotate(old.id, user.id, hash_of(2), new_expires, now)
        .await
        .expect("rotate");

    let old_after = find(&repo, hash_of(1)).await;
    assert_eq!(old_after.revoked_at, Some(now));
    assert_eq!(old_after.replaced_at, Some(now));

    let new_found = find(&repo, hash_of(2)).await;
    assert_eq!(new_found, new);
    assert_ne!(new_found.id, old.id);
    assert_eq!(new_found.user_id, user.id);
    assert_eq!(new_found.expires_at, new_expires);
    assert_eq!(new_found.revoked_at, None);
    assert_eq!(new_found.replaced_at, None);
}

#[sqlx::test]
async fn rotate_already_revoked_returns_already_revoked(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool);
    let old = repo
        .insert(
            user.id,
            PASSWORD_HASH,
            hash_of(1),
            t0() + Duration::days(30),
        )
        .await
        .expect("insert");
    let expires = t0() + Duration::days(31);

    repo.rotate(old.id, user.id, hash_of(2), expires, t0())
        .await
        .expect("first rotate");
    let second = repo
        .rotate(
            old.id,
            user.id,
            hash_of(3),
            expires,
            t0() + Duration::seconds(1),
        )
        .await;

    assert!(matches!(second, Err(RefreshRepoError::AlreadyRevoked)));
    // The failed rotation rolled back: no token was inserted, and the first
    // rotation's timestamps were not overwritten.
    assert!(
        repo.find_by_hash(hash_of(3))
            .await
            .expect("query")
            .is_none()
    );
    let old_after = find(&repo, hash_of(1)).await;
    assert_eq!(old_after.revoked_at, Some(t0()));
    assert_eq!(old_after.replaced_at, Some(t0()));

    // A token revoked by logout cannot be rotated either.
    let logged_out = repo
        .insert(user.id, PASSWORD_HASH, hash_of(4), expires)
        .await
        .expect("insert");
    repo.revoke(logged_out.id, t0()).await.expect("revoke");
    let res = repo
        .rotate(logged_out.id, user.id, hash_of(5), expires, t0())
        .await;
    assert!(matches!(res, Err(RefreshRepoError::AlreadyRevoked)));
    assert!(
        repo.find_by_hash(hash_of(5))
            .await
            .expect("query")
            .is_none()
    );
}

#[sqlx::test]
async fn revoke_all_for_user_revokes_every_active_token_only_for_that_user(pool: PgPool) {
    let alice = create_user(&pool, "alice@example.com").await;
    let bob = create_user(&pool, "bob@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool);
    let expires = t0() + Duration::days(30);
    for hash in [hash_of(1), hash_of(2), hash_of(3)] {
        repo.insert(alice.id, PASSWORD_HASH, hash, expires)
            .await
            .expect("insert");
    }
    repo.insert(bob.id, PASSWORD_HASH, hash_of(9), expires)
        .await
        .expect("insert");
    let earlier = find(&repo, hash_of(3)).await;
    repo.revoke(earlier.id, t0()).await.expect("revoke");
    let now = t0() + Duration::hours(2);

    let revoked = repo
        .revoke_all_for_user(alice.id, now)
        .await
        .expect("revoke all");

    assert_eq!(revoked, 2, "only the two active tokens are revoked");
    for hash in [hash_of(1), hash_of(2)] {
        let record = find(&repo, hash).await;
        assert_eq!(record.revoked_at, Some(now));
        assert_eq!(record.replaced_at, None);
    }
    assert_eq!(
        find(&repo, hash_of(3)).await.revoked_at,
        Some(t0()),
        "an already revoked token keeps its first revocation time"
    );
    let bobs = find(&repo, hash_of(9)).await;
    assert_eq!(bobs.revoked_at, None);
    assert_eq!(bobs.user_id, bob.id);
}

#[sqlx::test]
async fn revoke_sets_revoked_at_but_leaves_replaced_at_null(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool);
    let record = repo
        .insert(
            user.id,
            PASSWORD_HASH,
            hash_of(1),
            t0() + Duration::days(30),
        )
        .await
        .expect("insert");
    let now = t0() + Duration::minutes(5);

    repo.revoke(record.id, now).await.expect("revoke");
    // Revoking again keeps the first revocation time.
    repo.revoke(record.id, now + Duration::minutes(1))
        .await
        .expect("revoke again");
    // Unknown ids are ignored.
    repo.revoke(Uuid::nil(), now).await.expect("revoke unknown");

    let after = find(&repo, hash_of(1)).await;
    assert_eq!(after.revoked_at, Some(now));
    assert_eq!(after.replaced_at, None);
}

#[sqlx::test]
async fn deleting_user_deletes_their_refresh_tokens(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool.clone());
    repo.insert(
        user.id,
        PASSWORD_HASH,
        hash_of(1),
        t0() + Duration::days(30),
    )
    .await
    .expect("insert");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("delete user");

    assert!(
        repo.find_by_hash(hash_of(1))
            .await
            .expect("query")
            .is_none()
    );
}

// ---- SessionService::start ----

struct Fixture {
    service: SessionService<PgUserRepository, PgRefreshTokenRepository>,
    jwt: Arc<JwtService>,
    clock: Arc<FakeClock>,
}

fn session_service(pool: &PgPool) -> Fixture {
    let config = test_config();
    let jwt = Arc::new(JwtService::new(
        config.jwt_secret.as_bytes(),
        Duration::seconds(config.access_token_ttl_secs),
    ));
    let clock = Arc::new(FakeClock::default());
    let service = SessionService::new(
        PgUserRepository::new(pool.clone()),
        PgRefreshTokenRepository::new(pool.clone()),
        jwt.clone(),
        clock.clone(),
        Duration::days(config.refresh_token_ttl_days),
    );
    Fixture {
        service,
        jwt,
        clock,
    }
}

async fn stored_hashes(pool: &PgPool, user_id: Uuid) -> Vec<Vec<u8>> {
    sqlx::query_scalar::<_, Vec<u8>>("SELECT token_hash FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .expect("read stored hashes")
}

#[sqlx::test]
async fn start_returns_jwt_valid_for_900s_and_stores_only_hash(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let f = session_service(&pool);
    let now = f.clock.now();

    let session = f
        .service
        .start(&user, PASSWORD_HASH)
        .await
        .expect("start session");

    assert_eq!(session.access_expires_in_secs, 900);
    let claims = f
        .jwt
        .verify(&session.access_token, now)
        .expect("access token is valid now");
    assert_eq!(claims.sub, user.id);
    assert_eq!(claims.role, user.role);
    assert_eq!(claims.iat, now.timestamp());
    assert_eq!(claims.exp - claims.iat, 900);
    assert!(
        f.jwt
            .verify(&session.access_token, now + Duration::seconds(899))
            .is_ok()
    );
    assert!(matches!(
        f.jwt
            .verify(&session.access_token, now + Duration::seconds(900)),
        Err(TokenError::Expired)
    ));

    let stored = stored_hashes(&pool, user.id).await;
    assert_eq!(stored.len(), 1);
    assert_ne!(stored[0], session.refresh_token.as_str().as_bytes());
    assert_eq!(stored[0], session.refresh_token.hash().to_vec());
    // The token handed to the client parses back and is found by its hash.
    let presented = OpaqueToken::from_client(session.refresh_token.as_str()).expect("valid");
    let record = PgRefreshTokenRepository::new(pool.clone())
        .find_by_hash(presented.hash())
        .await
        .expect("query")
        .expect("token stored");
    assert_eq!(record.user_id, user.id);
    assert_eq!(record.revoked_at, None);
}

#[sqlx::test]
async fn start_sets_refresh_expiry_30_days_from_now(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let f = session_service(&pool);
    f.clock.advance(Duration::minutes(7));
    let now = f.clock.now();

    let session = f
        .service
        .start(&user, PASSWORD_HASH)
        .await
        .expect("start session");

    assert_eq!(session.refresh_expires_at, now + Duration::days(30));
    let record = PgRefreshTokenRepository::new(pool.clone())
        .find_by_hash(session.refresh_token.hash())
        .await
        .expect("query")
        .expect("token stored");
    assert_eq!(record.expires_at, now + Duration::days(30));
}

#[sqlx::test]
async fn start_after_password_change_is_invalid_credentials_and_stores_nothing(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let f = session_service(&pool);

    // The caller verified a hash that a password reset has replaced since.
    let res = f.service.start(&user, "hash-before-reset").await;

    assert!(matches!(res, Err(AuthError::InvalidCredentials)), "{res:?}");
    assert!(stored_hashes(&pool, user.id).await.is_empty());
}

#[sqlx::test]
async fn each_start_issues_a_distinct_refresh_token(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let f = session_service(&pool);

    let first = f
        .service
        .start(&user, PASSWORD_HASH)
        .await
        .expect("first session");
    let second = f
        .service
        .start(&user, PASSWORD_HASH)
        .await
        .expect("second session");

    assert_ne!(first.refresh_token, second.refresh_token);
    assert_eq!(stored_hashes(&pool, user.id).await.len(), 2);
}

// ---- POST /api/v1/auth/refresh and /api/v1/auth/logout ----

const EMAIL: &str = "an@example.com";
const PASSWORD: &str = "correct horse battery";
const REFRESH: &str = "/api/v1/auth/refresh";
const LOGOUT: &str = "/api/v1/auth/logout";

/// `"dn_refresh=<value>"` taken from a response's `Set-Cookie`.
fn cookie_pair(res: &TestResponse) -> String {
    let value = res
        .cookie_value("dn_refresh")
        .expect("response sets dn_refresh");
    format!("dn_refresh={value}")
}

/// A registered user and the refresh cookie pair from sign-up.
async fn signed_up(pool: PgPool) -> (TestApp, String) {
    let app = TestApp::new(pool);
    let res = app.register(EMAIL, PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
    let cookie = cookie_pair(&res);
    (app, cookie)
}

async fn sign_in(app: &TestApp) -> String {
    let res = app
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({ "email": EMAIL, "password": PASSWORD }))
        .send()
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
    cookie_pair(&res)
}

async fn refresh(app: &TestApp, cookie: &str) -> TestResponse {
    app.post(REFRESH).cookie(cookie).send().await
}

fn assert_invalid_and_cleared(res: &TestResponse) {
    assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "invalid_refresh_token");
    let c = res
        .set_cookie("dn_refresh")
        .expect("401 clears the refresh cookie");
    assert!(
        c.starts_with("dn_refresh=;") && c.contains("Max-Age=0") && c.contains("Path=/api/v1/auth"),
        "{c}"
    );
}

#[sqlx::test]
async fn refresh_with_valid_cookie_returns_new_token_and_rotates_cookie(pool: PgPool) {
    let (app, old_cookie) = signed_up(pool).await;

    let res = refresh(&app, &old_cookie).await;

    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
    assert!(res.body["access_token"].is_string());
    assert_eq!(res.body["token_type"], "Bearer");
    assert_eq!(res.body["expires_in"], 900);
    assert_eq!(res.body["user"]["email"], EMAIL);
    assert_eq!(res.headers["cache-control"], "no-store");
    let set = res.set_cookie("dn_refresh").expect("new cookie");
    assert!(
        set.contains("HttpOnly")
            && set.contains("SameSite=Strict")
            && set.contains("Path=/api/v1/auth")
            && set.contains("Max-Age=2592000"),
        "{set}"
    );
    let new_cookie = cookie_pair(&res);
    assert_ne!(new_cookie, old_cookie);
    // The new token works in turn.
    assert_eq!(refresh(&app, &new_cookie).await.status, StatusCode::OK);
}

#[sqlx::test]
async fn refreshed_token_expires_30_days_after_the_refresh(pool: PgPool) {
    let (app, first) = signed_up(pool).await;
    app.clock.advance(Duration::days(20));
    let second = cookie_pair(&refresh(&app, &first).await);

    // 40 days after sign-up, but only 20 days after the refresh.
    app.clock.advance(Duration::days(20));
    assert_eq!(refresh(&app, &second).await.status, StatusCode::OK);
}

#[sqlx::test]
async fn refresh_with_old_cookie_within_30s_returns_409_refresh_conflict_and_keeps_session(
    pool: PgPool,
) {
    let (app, old_cookie) = signed_up(pool).await;
    let first = refresh(&app, &old_cookie).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text);
    let new_cookie = cookie_pair(&first);
    // Exactly 30 seconds still counts as "within".
    app.clock.advance(Duration::seconds(30));

    let old = refresh(&app, &old_cookie).await;

    assert_eq!(old.status, StatusCode::CONFLICT, "{}", old.text);
    assert_eq!(old.body["error"]["code"], "refresh_conflict");
    assert!(
        old.set_cookie("dn_refresh").is_none(),
        "409 keeps the cookie"
    );
    assert_eq!(refresh(&app, &new_cookie).await.status, StatusCode::OK);
}

#[sqlx::test]
async fn refresh_with_old_cookie_after_30s_revokes_all_sessions(pool: PgPool) {
    let (app, old_cookie) = signed_up(pool).await;
    let other_session = sign_in(&app).await;
    let new_cookie = cookie_pair(&refresh(&app, &old_cookie).await);
    app.clock.advance(Duration::seconds(31));

    assert_invalid_and_cleared(&refresh(&app, &old_cookie).await);
    assert_invalid_and_cleared(&refresh(&app, &new_cookie).await);
    assert_invalid_and_cleared(&refresh(&app, &other_session).await);
    // A fresh sign-in still works.
    let again = sign_in(&app).await;
    assert_eq!(refresh(&app, &again).await.status, StatusCode::OK);
}

#[sqlx::test]
async fn refresh_401_clears_the_cookie(pool: PgPool) {
    let (app, _) = signed_up(pool).await;
    let unknown = format!("dn_refresh={}", "ab".repeat(32));

    let res = refresh(&app, &unknown).await;

    assert_invalid_and_cleared(&res);
    assert!(
        res.set_cookie("dn_refresh")
            .expect("cleared")
            .contains("Max-Age=0")
    );
    assert_eq!(res.headers["cache-control"], "no-store");
}

#[sqlx::test]
async fn refresh_with_malformed_cookie_returns_401_and_clears_it(pool: PgPool) {
    let (app, _) = signed_up(pool).await;

    for bad in ["dn_refresh=", "dn_refresh=not-a-token", "dn_refresh=ABCD"] {
        assert_invalid_and_cleared(&refresh(&app, bad).await);
    }
}

#[sqlx::test]
async fn token_revoked_by_logout_does_not_revoke_other_sessions(pool: PgPool) {
    let (app, a) = signed_up(pool).await;
    let b = sign_in(&app).await;
    let res = app.post(LOGOUT).cookie(&a).send().await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
    app.clock.advance(Duration::seconds(31));

    assert_invalid_and_cleared(&refresh(&app, &a).await);
    assert_eq!(refresh(&app, &b).await.status, StatusCode::OK);
}

#[sqlx::test]
async fn refresh_after_30_days_returns_401(pool: PgPool) {
    let (app, cookie) = signed_up(pool).await;
    app.clock.advance(Duration::days(30));

    assert_invalid_and_cleared(&refresh(&app, &cookie).await);
}

#[sqlx::test]
async fn refresh_one_second_before_30_days_succeeds(pool: PgPool) {
    let (app, cookie) = signed_up(pool).await;
    app.clock.advance(Duration::days(30) - Duration::seconds(1));

    assert_eq!(refresh(&app, &cookie).await.status, StatusCode::OK);
}

#[sqlx::test]
async fn refresh_without_cookie_returns_401_invalid_refresh_token(pool: PgPool) {
    let (app, _) = signed_up(pool).await;

    let res = app.post(REFRESH).send().await;

    assert_invalid_and_cleared(&res);
}

#[sqlx::test]
async fn refresh_returns_current_role_from_database(pool: PgPool) {
    let (app, cookie) = signed_up(pool.clone()).await;
    let promoted = PgUserRepository::new(pool)
        .set_role(&Email::parse(EMAIL).expect("valid"), Role::Admin)
        .await
        .expect("set role");
    assert!(promoted);

    let res = refresh(&app, &cookie).await;

    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
    assert_eq!(res.body["user"]["role"], "admin");
    let token = res.body["access_token"].as_str().expect("access token");
    let config = test_config();
    let jwt = JwtService::new(
        config.jwt_secret.as_bytes(),
        Duration::seconds(config.access_token_ttl_secs),
    );
    let claims = jwt.verify(token, app.clock.now()).expect("valid jwt");
    assert_eq!(claims.role, Role::Admin);
}

#[sqlx::test]
async fn refresh_for_deleted_user_returns_401(pool: PgPool) {
    let (app, cookie) = signed_up(pool.clone()).await;
    sqlx::query("DELETE FROM users WHERE email = $1")
        .bind(EMAIL)
        .execute(&pool)
        .await
        .expect("delete user");

    assert_invalid_and_cleared(&refresh(&app, &cookie).await);
}

#[sqlx::test]
async fn concurrent_refreshes_with_same_cookie_give_one_200_and_one_409(pool: PgPool) {
    let (app, cookie) = signed_up(pool).await;

    let mut set = JoinSet::new();
    for _ in 0..2 {
        let request = app.post(REFRESH).cookie(&cookie);
        set.spawn(async move { request.send().await });
    }
    let results: Vec<TestResponse> = set.join_all().await;

    let mut statuses: Vec<StatusCode> = results.iter().map(|r| r.status).collect();
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::OK, StatusCode::CONFLICT],
        "{results:?}"
    );
    let winner = results
        .iter()
        .find(|r| r.status == StatusCode::OK)
        .expect("one refresh succeeded");
    assert_eq!(
        refresh(&app, &cookie_pair(winner)).await.status,
        StatusCode::OK,
        "the session survives the race"
    );
}

#[sqlx::test]
async fn logout_revokes_token_and_clears_cookie(pool: PgPool) {
    let (app, cookie) = signed_up(pool).await;

    let res = app.post(LOGOUT).cookie(&cookie).send().await;

    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
    assert_eq!(res.headers["cache-control"], "no-store");
    let c = res.set_cookie("dn_refresh").expect("logout clears cookie");
    assert!(
        c.contains("Max-Age=0") && c.contains("Path=/api/v1/auth"),
        "{c}"
    );
    assert_invalid_and_cleared(&refresh(&app, &cookie).await);
}

#[sqlx::test]
async fn logout_without_cookie_returns_204(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app.post(LOGOUT).send().await;

    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
    let c = res.set_cookie("dn_refresh").expect("logout clears cookie");
    assert!(c.contains("Max-Age=0"), "{c}");
}

#[sqlx::test]
async fn logout_with_unknown_or_malformed_cookie_returns_204(pool: PgPool) {
    let (app, cookie) = signed_up(pool).await;
    let unknown = format!("dn_refresh={}", "cd".repeat(32));

    for bad in [unknown.as_str(), "dn_refresh=garbage"] {
        let res = app.post(LOGOUT).cookie(bad).send().await;
        assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
        assert!(
            res.set_cookie("dn_refresh")
                .expect("cleared")
                .contains("Max-Age=0")
        );
    }
    // Logging out twice with the same token is fine too.
    for _ in 0..2 {
        let res = app.post(LOGOUT).cookie(&cookie).send().await;
        assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
    }
    assert_invalid_and_cleared(&refresh(&app, &cookie).await);
}

#[sqlx::test]
async fn logout_storage_error_returns_500_and_still_clears_cookie(pool: PgPool) {
    // #17: a revocation that fails must not be reported as a sign-out. The
    // failure is simulated with a trigger that rejects every UPDATE of
    // `refresh_tokens`, so the lookup works and the revocation fails.
    let (app, cookie) = signed_up(pool.clone()).await;
    sqlx::raw_sql(
        r#"CREATE FUNCTION reject_update() RETURNS trigger LANGUAGE plpgsql AS
           $$ BEGIN RAISE EXCEPTION 'simulated storage failure'; END $$;
           CREATE TRIGGER reject_update BEFORE UPDATE ON refresh_tokens
           FOR EACH ROW EXECUTE FUNCTION reject_update();"#,
    )
    .execute(&pool)
    .await
    .expect("install failing trigger");

    let res = app.post(LOGOUT).cookie(&cookie).send().await;

    assert_eq!(
        res.status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "{}",
        res.text
    );
    assert_eq!(res.body["error"]["code"], "internal_error");
    assert!(res.body["error"]["message"].is_string());
    assert!(!res.text.contains("simulated"), "{}", res.text);
    assert_eq!(res.headers["cache-control"], "no-store");
    let c = res
        .set_cookie("dn_refresh")
        .expect("500 still clears cookie");
    assert!(
        c.starts_with("dn_refresh=;") && c.contains("Max-Age=0") && c.contains("Path=/api/v1/auth"),
        "{c}"
    );

    // The 500 was truthful: the token was not revoked.
    sqlx::raw_sql("DROP TRIGGER reject_update ON refresh_tokens")
        .execute(&pool)
        .await
        .expect("drop trigger");
    assert_eq!(refresh(&app, &cookie).await.status, StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn openapi_documents_the_logout_500(pool: PgPool) {
    let res = TestApp::new(pool).get("/api/v1/openapi.json").send().await;

    let responses = &res.body["paths"][LOGOUT]["post"]["responses"];
    assert!(responses["204"].is_object(), "{}", res.text);
    assert!(responses["500"].is_object(), "{}", res.text);
}

#[sqlx::test(migrations = false)]
async fn openapi_lists_refresh_and_logout(pool: PgPool) {
    let res = TestApp::new(pool).get("/api/v1/openapi.json").send().await;

    assert!(
        res.body["paths"][REFRESH]["post"].is_object(),
        "{}",
        res.text
    );
    assert!(
        res.body["paths"][LOGOUT]["post"].is_object(),
        "{}",
        res.text
    );
}
