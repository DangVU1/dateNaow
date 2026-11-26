//! `POST /api/v1/auth/register` and `POST /api/v1/auth/login`, including the
//! failed-attempt limits (P5 as amended by DECISIONS #13: 5 per (email, IP)
//! pair, 20 per IP and 50 per email in 15 minutes), the sign-up limit (#17:
//! 10 per IP in an hour) and NFC passwords (#17).

use std::net::{IpAddr, SocketAddr};

use axum::http::StatusCode;
use chrono::Duration;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::task::JoinSet;

use crate::common::{TestApp, TestResponse};

const EMAIL: &str = "an@example.com";
const PASSWORD: &str = "correct horse battery";
const WRONG: &str = "wrong horse battery";

async fn login(app: &TestApp, email: &str, password: &str) -> TestResponse {
    app.post("/api/v1/auth/login")
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
}

async fn login_from(app: &TestApp, peer: IpAddr, email: &str, password: &str) -> TestResponse {
    app.post("/api/v1/auth/login")
        .peer(SocketAddr::new(peer, 40000))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
}

/// The `n`-th test client address, `198.51.100.n`, distinct from the default peer.
fn client(n: u8) -> IpAddr {
    IpAddr::from([198, 51, 100, n])
}

/// Rows stored under the plain-text limiter `label` (hashed like the limiter).
async fn rows_for(pool: &PgPool, label: &str) -> i64 {
    let hash: [u8; 32] = Sha256::digest(label.as_bytes()).into();
    sqlx::query_scalar("SELECT count(*) FROM auth_attempts WHERE key_hash = $1")
        .bind(&hash[..])
        .fetch_one(pool)
        .await
        .expect("count auth_attempts")
}

async fn registered(pool: PgPool) -> TestApp {
    let app = TestApp::new(pool);
    let res = app.register(EMAIL, PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
    app
}

// ---- register ----

async fn register_from(app: &TestApp, peer: IpAddr, email: &str, password: &str) -> TestResponse {
    app.post("/api/v1/auth/register")
        .peer(SocketAddr::new(peer, 40000))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
}

async fn user_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(pool)
        .await
        .expect("count users")
}

#[sqlx::test]
async fn eleventh_sign_up_from_one_ip_in_an_hour_returns_429(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    for n in 0..10 {
        let res = register_from(&app, client(1), &format!("u{n}@example.com"), PASSWORD).await;
        assert_eq!(res.status, StatusCode::CREATED, "#{n}: {}", res.text);
    }

    let res = register_from(&app, client(1), "u10@example.com", PASSWORD).await;

    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "rate_limited");
    assert_eq!(res.headers["retry-after"], "3600");
    assert!(res.set_cookie("dn_refresh").is_none());
    assert_eq!(user_count(&pool).await, 10, "no account past the limit");
    // Successful sign-ups are not forgiven: every attempt is still counted,
    // under a hashed key.
    assert_eq!(rows_for(&pool, "register:ip:198.51.100.1").await, 11);

    // Another address is not affected.
    let res = register_from(&app, client(2), "u10@example.com", PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
}

#[sqlx::test]
async fn sign_up_limit_lifts_after_an_hour(pool: PgPool) {
    let app = TestApp::new(pool);
    for n in 0..10 {
        let res = register_from(&app, client(1), &format!("u{n}@example.com"), PASSWORD).await;
        assert_eq!(res.status, StatusCode::CREATED, "#{n}: {}", res.text);
    }
    let blocked = register_from(&app, client(1), "late@example.com", PASSWORD).await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);

    app.clock
        .advance(Duration::minutes(60) + Duration::seconds(1));
    let res = register_from(&app, client(1), "late@example.com", PASSWORD).await;

    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
}

#[sqlx::test]
async fn sign_up_limit_is_per_ipv6_64(pool: PgPool) {
    let app = TestApp::new(pool);
    for n in 0..10u16 {
        let peer = IpAddr::from([0x2001, 0xdb8, 1, 2, 0, 0, 0, n + 1]);
        let res = register_from(&app, peer, &format!("u{n}@example.com"), PASSWORD).await;
        assert_eq!(res.status, StatusCode::CREATED, "#{n}: {}", res.text);
    }
    let same_64 = IpAddr::from([0x2001, 0xdb8, 1, 2, 0xffff, 0, 0, 1]);
    let res = register_from(&app, same_64, "u10@example.com", PASSWORD).await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS, "{}", res.text);

    let other_64 = IpAddr::from([0x2001, 0xdb8, 1, 3, 0, 0, 0, 1]);
    let res = register_from(&app, other_64, "u10@example.com", PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
}

#[sqlx::test]
async fn invalid_sign_ups_are_not_counted(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    for _ in 0..15 {
        let res = register_from(&app, client(1), "not-an-email", "short").await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.text);
        let res = register_from(&app, client(1), "an@example.com", "password").await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.text);
    }
    assert_eq!(rows_for(&pool, "register:ip:198.51.100.1").await, 0);

    let res = register_from(&app, client(1), EMAIL, PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
}

#[sqlx::test]
async fn duplicate_email_sign_ups_are_counted_and_limited_before_email_taken(pool: PgPool) {
    let app = TestApp::new(pool);
    let res = register_from(&app, client(1), EMAIL, PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
    for _ in 0..9 {
        let res = register_from(&app, client(1), EMAIL, PASSWORD).await;
        assert_eq!(res.status, StatusCode::CONFLICT, "{}", res.text);
    }

    // Past the limit the answer no longer says whether the email is taken.
    let res = register_from(&app, client(1), EMAIL, PASSWORD).await;

    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "rate_limited");
}

#[sqlx::test]
async fn sign_up_limit_is_separate_from_login_limits(pool: PgPool) {
    let app = registered(pool).await;
    for _ in 0..20 {
        login(&app, "nobody@example.com", WRONG).await;
    }
    // The IP is now over the login limit but can still sign up.
    let res = app.register("bo@example.com", PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
}

// ---- NFC passwords (#17) ----

/// "mật khẩu đẹp lắm" with precomposed letters and with combining marks.
const VI_NFC: &str = "m\u{1EAD}t kh\u{1EA9}u \u{0111}\u{1EB9}p l\u{1EAF}m";
const VI_NFD: &str =
    "ma\u{0323}\u{0302}t kha\u{0302}\u{0309}u \u{0111}e\u{0323}p la\u{0306}\u{0301}m";

#[sqlx::test]
async fn password_set_in_one_unicode_form_signs_in_with_the_other(pool: PgPool) {
    assert_ne!(VI_NFC, VI_NFD);
    let app = TestApp::new(pool);
    for (email, set, typed) in [
        ("nfc@example.com", VI_NFC, VI_NFD),
        ("nfd@example.com", VI_NFD, VI_NFC),
    ] {
        let res = app.register(email, set).await;
        assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);

        let res = login(&app, email, typed).await;
        assert_eq!(res.status, StatusCode::OK, "{email}: {}", res.text);
        let res = login(&app, email, set).await;
        assert_eq!(res.status, StatusCode::OK, "{email}: {}", res.text);
    }
}

#[sqlx::test]
async fn password_length_counts_nfc_characters(pool: PgPool) {
    let app = TestApp::new(pool);
    // 128 characters in NFC, 384 in NFD: accepted.
    let long = "a\u{0323}\u{0302}".repeat(128);
    let res = app.register(EMAIL, &long).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
    let res = login(&app, EMAIL, &"\u{1EAD}".repeat(128)).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
}

#[sqlx::test]
async fn register_returns_201_with_token_and_refresh_cookie(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app.register(EMAIL, PASSWORD).await;

    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
    assert!(res.body["access_token"].is_string());
    assert_eq!(res.body["token_type"], "Bearer");
    assert!(res.body["user"]["id"].is_string());
    assert_eq!(res.body["user"]["email"], "an@example.com");
    assert_eq!(res.body["user"]["role"], "user");
    assert!(res.body["user"]["gender"].is_null());
    assert_eq!(res.body["expires_in"], 900);
    let c = res.set_cookie("dn_refresh").expect("refresh cookie is set");
    assert!(
        c.contains("HttpOnly")
            && c.contains("SameSite=Strict")
            && c.contains("Path=/api/v1/auth")
            && c.contains("Max-Age=2592000"),
        "{c}"
    );
    let value = res.cookie_value("dn_refresh").expect("cookie value");
    assert_eq!(value.len(), 64);
}

#[sqlx::test]
async fn register_normalises_email(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app.register("  An@Example.COM ", PASSWORD).await;

    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
    assert_eq!(res.body["user"]["email"], "an@example.com");
}

#[sqlx::test]
async fn register_same_email_different_case_returns_409_email_taken(pool: PgPool) {
    let app = registered(pool).await;

    let res = app.register(" AN@example.com", PASSWORD).await;

    assert_eq!(res.status, StatusCode::CONFLICT);
    assert_eq!(res.body["error"]["code"], "email_taken");
}

#[sqlx::test]
async fn register_short_password_returns_422_with_password_field(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app.register(EMAIL, "short").await;

    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.body["error"]["code"], "validation_error");
    assert_eq!(res.body["error"]["fields"]["password"][0], "too_short");
}

#[sqlx::test]
async fn register_password_of_129_chars_returns_422_too_long(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app.register(EMAIL, &"a".repeat(129)).await;

    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.body["error"]["fields"]["password"][0], "too_long");
}

#[sqlx::test]
async fn register_common_password_returns_422_too_common_and_creates_no_user(pool: PgPool) {
    let app = TestApp::new(pool.clone());

    // In the SecLists list, in a different case, and in the Vietnamese list.
    for common in ["password", "PassWord", "12345678", "matkhau123"] {
        let res = app.register(EMAIL, common).await;

        assert_eq!(
            res.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{common}: {}",
            res.text
        );
        assert_eq!(res.body["error"]["code"], "validation_error");
        assert_eq!(
            res.body["error"]["fields"],
            json!({ "password": ["too_common"] }),
            "{common}"
        );
        assert!(res.set_cookie("dn_refresh").is_none());
    }
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .expect("count users");
    assert_eq!(users, 0);
}

#[sqlx::test]
async fn register_short_common_password_still_reports_too_short(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app.register(EMAIL, "123456").await;

    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.body["error"]["fields"]["password"][0], "too_short");
}

#[sqlx::test]
async fn register_reports_email_and_password_errors_together(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app.register("not-an-email", "short").await;

    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.body["error"]["fields"]["email"][0], "invalid_email");
    assert_eq!(res.body["error"]["fields"]["password"][0], "too_short");
}

#[sqlx::test]
async fn register_rejects_unknown_fields(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app
        .post("/api/v1/auth/register")
        .json(&json!({ "email": EMAIL, "password": PASSWORD, "role": "admin" }))
        .send()
        .await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.body["error"]["code"], "invalid_json");
}

#[sqlx::test]
async fn auth_responses_have_cache_control_no_store(pool: PgPool) {
    let app = TestApp::new(pool);

    let created = app.register(EMAIL, PASSWORD).await;
    let failed = login(&app, EMAIL, WRONG).await;
    let invalid = app.register(EMAIL, "short").await;

    for res in [created, failed, invalid] {
        assert_eq!(res.headers["cache-control"], "no-store", "{}", res.text);
    }
}

#[sqlx::test]
async fn register_response_never_contains_password_hash(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app.register(EMAIL, PASSWORD).await;

    assert_eq!(res.status, StatusCode::CREATED);
    assert!(!res.text.contains("$argon2"));
    assert!(!res.text.contains("password"));
}

// ---- login ----

#[sqlx::test]
async fn login_with_correct_password_returns_200_and_cookie(pool: PgPool) {
    let app = registered(pool).await;

    let res = login(&app, " An@Example.com ", PASSWORD).await;

    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
    assert!(res.body["access_token"].is_string());
    assert_eq!(res.body["token_type"], "Bearer");
    assert_eq!(res.body["expires_in"], 900);
    assert_eq!(res.body["user"]["email"], "an@example.com");
    assert_eq!(res.body["user"]["role"], "user");
    let c = res.set_cookie("dn_refresh").expect("refresh cookie is set");
    assert!(
        c.contains("HttpOnly") && c.contains("Max-Age=2592000"),
        "{c}"
    );
    assert!(!res.text.contains("$argon2"));
}

/// Gives `EMAIL`'s account the password `raw` directly in the database, the
/// way an account created before DECISIONS #16 could have it. Hashed with
/// the argon2 crate itself: `PasswordHashService::hash` only takes a
/// `NewPassword`, which a common password can never become (#17). `raw` must
/// already be NFC.
async fn set_password_directly(pool: &PgPool, raw: &str) {
    use argon2::password_hash::PasswordHasher;

    let params = crate::common::test_config()
        .argon2_params()
        .expect("valid test params");
    let hash = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password(raw.as_bytes())
        .expect("hash password")
        .to_string();
    let updated = sqlx::query("UPDATE users SET password_hash = $1 WHERE email = $2")
        .bind(hash)
        .bind(EMAIL)
        .execute(pool)
        .await
        .expect("update password hash");
    assert_eq!(updated.rows_affected(), 1);
}

#[sqlx::test]
async fn login_still_works_for_an_account_with_a_common_password(pool: PgPool) {
    let app = registered(pool.clone()).await;
    set_password_directly(&pool, "password1").await;

    let res = login(&app, EMAIL, "password1").await;

    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
}

#[sqlx::test]
async fn wrong_common_password_guesses_are_counted_by_the_limiter(pool: PgPool) {
    // Regression guard (#16): sign-in must keep `PlainPassword::parse`; if it
    // used the common-password list, these guesses would skip the limiter.
    let app = registered(pool).await;
    for _ in 0..5 {
        let res = login(&app, EMAIL, "password").await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{}", res.text);
        assert_eq!(res.body["error"]["code"], "invalid_credentials");
    }

    let res = login(&app, EMAIL, PASSWORD).await;

    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "rate_limited");
}

#[sqlx::test]
async fn login_with_wrong_password_returns_401_invalid_credentials(pool: PgPool) {
    let app = registered(pool).await;

    let res = login(&app, EMAIL, WRONG).await;

    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.body["error"]["code"], "invalid_credentials");
    assert!(res.set_cookie("dn_refresh").is_none());
}

#[sqlx::test]
async fn login_unknown_email_returns_same_body_as_wrong_password(pool: PgPool) {
    let app = registered(pool).await;

    let wrong = login(&app, EMAIL, WRONG).await;
    let unknown = login(&app, "nobody@example.com", WRONG).await;

    assert_eq!(unknown.status, wrong.status);
    assert_eq!(unknown.body, wrong.body);
}

#[sqlx::test]
async fn login_with_malformed_input_returns_invalid_credentials(pool: PgPool) {
    let app = registered(pool).await;

    for (email, password) in [("not-an-email", PASSWORD), (EMAIL, "short")] {
        let res = login(&app, email, password).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
        assert_eq!(res.body["error"]["code"], "invalid_credentials");
    }
}

#[sqlx::test]
async fn sixth_login_from_one_ip_after_five_failures_returns_429_even_with_correct_password(
    pool: PgPool,
) {
    let app = registered(pool).await;
    for _ in 0..5 {
        assert_eq!(
            login(&app, EMAIL, WRONG).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    let res = login(&app, EMAIL, PASSWORD).await;

    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.headers["retry-after"], "900");
    assert_eq!(res.body["error"]["code"], "rate_limited");
    assert!(res.set_cookie("dn_refresh").is_none());

    // The pair (email, that IP) is limited, not the email: another IP still
    // gets its password checked.
    let elsewhere = login_from(&app, client(1), EMAIL, WRONG).await;
    assert_eq!(
        elsewhere.status,
        StatusCode::UNAUTHORIZED,
        "{}",
        elsewhere.text
    );
    let owner = login_from(&app, client(2), EMAIL, PASSWORD).await;
    assert_eq!(owner.status, StatusCode::OK, "{}", owner.text);
}

#[sqlx::test]
async fn slow_wrong_guesses_from_one_ip_never_lock_the_owner_out(pool: PgPool) {
    // Review finding F2: one wrong password every 2.5 minutes from IP A for
    // 45 minutes used to keep the email locked for good.
    let app = registered(pool.clone()).await;
    let attacker = client(1);
    let mut limited = 0;
    for _ in 0..18 {
        let res = login_from(&app, attacker, EMAIL, WRONG).await;
        match res.status {
            StatusCode::UNAUTHORIZED => {}
            StatusCode::TOO_MANY_REQUESTS => limited += 1,
            other => panic!("unexpected {other}: {}", res.text),
        }
        app.clock
            .advance(Duration::minutes(2) + Duration::seconds(30));
    }
    assert!(limited > 0, "the attacker's own pair is limited");
    // Only the attempts that passed the pair limit reach the email's counter.
    assert_eq!(rows_for(&pool, &format!("login:email:{EMAIL}")).await, 5);

    let res = login_from(&app, client(2), EMAIL, PASSWORD).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
}

#[sqlx::test]
async fn hammering_from_one_ip_does_not_reach_the_global_email_limit(pool: PgPool) {
    let app = registered(pool.clone()).await;
    let attacker = client(1);
    for i in 0..100 {
        let res = login_from(&app, attacker, EMAIL, WRONG).await;
        let expected = if i < 5 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        assert_eq!(res.status, expected, "attempt {i}: {}", res.text);
    }
    // Attempts blocked by the pair limit never reach the email's counter.
    assert_eq!(rows_for(&pool, &format!("login:email:{EMAIL}")).await, 5);
    assert_eq!(
        rows_for(&pool, &format!("login:pair:{EMAIL}|{attacker}")).await,
        100
    );

    let res = login_from(&app, client(2), EMAIL, PASSWORD).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
}

#[sqlx::test]
async fn fifty_first_attempt_across_many_ips_returns_429_even_with_correct_password(pool: PgPool) {
    let app = registered(pool).await;
    for n in 1..=10 {
        for i in 0..5 {
            let res = login_from(&app, client(n), EMAIL, WRONG).await;
            assert_eq!(res.status, StatusCode::UNAUTHORIZED, "ip {n}, attempt {i}");
        }
    }

    let res = login_from(&app, client(11), EMAIL, PASSWORD).await;

    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS, "{}", res.text);
    assert_eq!(res.headers["retry-after"], "900");
    assert_eq!(res.body["error"]["code"], "rate_limited");
    assert!(res.set_cookie("dn_refresh").is_none());

    // The global cap is per email: other emails from a fresh IP are unaffected.
    let other = login_from(&app, client(12), "other@example.com", WRONG).await;
    assert_eq!(other.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn after_20_failures_from_one_ip_any_new_email_gets_429(pool: PgPool) {
    let app = registered(pool).await;
    for i in 0..20 {
        let res = login(&app, &format!("guess{i}@example.com"), WRONG).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "attempt {i}");
    }

    let res = login(&app, EMAIL, PASSWORD).await;

    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.headers["retry-after"], "900");
}

#[sqlx::test]
async fn failure_window_expires_after_15_minutes(pool: PgPool) {
    let app = registered(pool).await;
    for _ in 0..5 {
        login(&app, EMAIL, WRONG).await;
    }
    assert_eq!(
        login(&app, EMAIL, PASSWORD).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );

    app.clock
        .advance(Duration::minutes(15) + Duration::seconds(1));

    let res = login(&app, EMAIL, PASSWORD).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
}

#[sqlx::test]
async fn successful_login_clears_the_pair_failures(pool: PgPool) {
    let app = registered(pool).await;
    for _ in 0..4 {
        login(&app, EMAIL, WRONG).await;
    }
    assert_eq!(login(&app, EMAIL, PASSWORD).await.status, StatusCode::OK);

    // The pair starts from zero: five more failures, then the limit.
    for i in 0..5 {
        let res = login(&app, EMAIL, WRONG).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "attempt {i}");
    }
    assert_eq!(
        login(&app, EMAIL, WRONG).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test]
async fn successful_login_clears_the_global_counter_but_not_other_ips_pairs(pool: PgPool) {
    let app = registered(pool.clone()).await;
    for n in 1..=9 {
        for _ in 0..5 {
            login_from(&app, client(n), EMAIL, WRONG).await;
        }
    }
    let owner = login_from(&app, client(10), EMAIL, PASSWORD).await;
    assert_eq!(owner.status, StatusCode::OK, "{}", owner.text);
    assert_eq!(rows_for(&pool, &format!("login:email:{EMAIL}")).await, 0);

    // 45 earlier failures are gone from the global counter: ten more fit.
    for n in 11..=12 {
        for i in 0..5 {
            let res = login_from(&app, client(n), EMAIL, WRONG).await;
            assert_eq!(res.status, StatusCode::UNAUTHORIZED, "ip {n}, attempt {i}");
        }
    }
    // An attacker's own pair counter is kept.
    let attacker = login_from(&app, client(1), EMAIL, WRONG).await;
    assert_eq!(attacker.status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn attempts_table_stores_only_32_byte_hashes(pool: PgPool) {
    let app = registered(pool.clone()).await;
    login(&app, EMAIL, WRONG).await;
    login(&app, "other@example.com", WRONG).await;

    let lengths: Vec<i32> = sqlx::query_scalar("SELECT length(key_hash) FROM auth_attempts")
        .fetch_all(&pool)
        .await
        .expect("query auth_attempts");

    // The sign-up (once per IP, #17) and two failed logins, each counted
    // once per pair, per IP and per email.
    assert_eq!(lengths.len(), 7);
    assert!(lengths.iter().all(|len| *len == 32), "{lengths:?}");
    assert_eq!(rows_for(&pool, "register:ip:203.0.113.7").await, 1);
}

#[sqlx::test]
async fn forwarded_for_is_ignored_when_trust_proxy_is_false(pool: PgPool) {
    let app = registered(pool).await;
    for i in 0..20 {
        let res = app
            .post("/api/v1/auth/login")
            .header("x-forwarded-for", &format!("198.51.100.{i}"))
            .json(&json!({ "email": format!("guess{i}@example.com"), "password": WRONG }))
            .send()
            .await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "attempt {i}");
    }

    let res = app
        .post("/api/v1/auth/login")
        .header("x-forwarded-for", "198.51.100.200")
        .json(&json!({ "email": "fresh@example.com", "password": WRONG }))
        .send()
        .await;

    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn concurrent_wrong_logins_for_one_email_allow_at_most_five_password_checks(pool: PgPool) {
    let app = registered(pool).await;

    let mut set = JoinSet::new();
    for _ in 0..20 {
        let request = app
            .post("/api/v1/auth/login")
            .json(&json!({ "email": EMAIL, "password": WRONG }));
        set.spawn(async move { request.send().await.status });
    }
    let statuses: Vec<StatusCode> = set.join_all().await;

    assert_eq!(statuses.len(), 20);
    let unauthorized = statuses
        .iter()
        .filter(|s| **s == StatusCode::UNAUTHORIZED)
        .count();
    let limited = statuses
        .iter()
        .filter(|s| **s == StatusCode::TOO_MANY_REQUESTS)
        .count();
    assert!(unauthorized <= 5, "{statuses:?}");
    assert!(limited >= 15, "{statuses:?}");
}

#[sqlx::test]
async fn concurrent_wrong_logins_from_many_ips_allow_at_most_fifty_password_checks(pool: PgPool) {
    let app = registered(pool).await;

    let mut set = JoinSet::new();
    for n in 1..=12 {
        for _ in 0..5 {
            let request = app
                .post("/api/v1/auth/login")
                .peer(SocketAddr::new(client(n), 40000))
                .json(&json!({ "email": EMAIL, "password": WRONG }));
            set.spawn(async move { request.send().await.status });
        }
    }
    let statuses: Vec<StatusCode> = set.join_all().await;

    assert_eq!(statuses.len(), 60);
    let unauthorized = statuses
        .iter()
        .filter(|s| **s == StatusCode::UNAUTHORIZED)
        .count();
    let limited = statuses
        .iter()
        .filter(|s| **s == StatusCode::TOO_MANY_REQUESTS)
        .count();
    assert!(unauthorized <= 50, "{statuses:?}");
    assert_eq!(unauthorized + limited, 60, "{statuses:?}");
}

#[sqlx::test]
async fn ipv6_addresses_in_same_64_share_a_limit(pool: PgPool) {
    let app = registered(pool).await;
    let first: IpAddr = "2001:db8::1".parse().expect("valid IPv6");
    let same_64: IpAddr = "2001:db8::2".parse().expect("valid IPv6");
    let other_64: IpAddr = "2001:db8:0:1::1".parse().expect("valid IPv6");
    for i in 0..20 {
        let res = login_from(&app, first, &format!("guess{i}@example.com"), WRONG).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "attempt {i}");
    }

    let blocked = login_from(&app, same_64, "fresh@example.com", WRONG).await;
    let elsewhere = login_from(&app, other_64, "fresh2@example.com", WRONG).await;

    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(elsewhere.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn successful_login_is_not_counted_against_the_ip_limit(pool: PgPool) {
    let app = registered(pool).await;
    for i in 0..19 {
        let res = login(&app, &format!("guess{i}@example.com"), WRONG).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "attempt {i}");
    }
    assert_eq!(login(&app, EMAIL, PASSWORD).await.status, StatusCode::OK);

    // 19 failures + a forgiven success: the 20th failure is still allowed.
    let res = login(&app, "guess19@example.com", WRONG).await;

    assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{}", res.text);
}

#[sqlx::test]
async fn forwarded_for_is_used_when_trust_proxy_is_true(pool: PgPool) {
    let config = datenaow::config::Config {
        trust_proxy: true,
        ..crate::common::test_config()
    };
    let app = TestApp::with_config(pool, config);
    let attempt = |xff: &'static str, email: String| {
        app.post("/api/v1/auth/login")
            .header("x-forwarded-for", xff)
            .json(&json!({ "email": email, "password": WRONG }))
            .send()
    };
    for i in 0..20 {
        let res = attempt("198.51.100.7", format!("guess{i}@example.com")).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "attempt {i}");
    }

    let other_client = attempt("198.51.100.8", "fresh@example.com".into()).await;
    let same_client = attempt("198.51.100.7", "fresh2@example.com".into()).await;

    assert_eq!(other_client.status, StatusCode::UNAUTHORIZED);
    assert_eq!(same_client.status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn forged_entries_left_of_an_invalid_rightmost_are_never_used(pool: PgPool) {
    // #17: with an unusable rightmost entry the socket address counts, so
    // rotating the entries to its left does not spread the attempts out.
    let config = datenaow::config::Config {
        trust_proxy: true,
        ..crate::common::test_config()
    };
    let app = TestApp::with_config(pool, config);
    for i in 0..20 {
        let res = app
            .post("/api/v1/auth/login")
            .header("x-forwarded-for", &format!("198.51.100.{i}, garbage"))
            .json(&json!({ "email": format!("guess{i}@example.com"), "password": WRONG }))
            .send()
            .await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "attempt {i}");
    }

    // The 21st attempt from the same socket address is over its IP limit.
    let res = app
        .post("/api/v1/auth/login")
        .header("x-forwarded-for", "198.51.100.99, ")
        .json(&json!({ "email": "fresh@example.com", "password": WRONG }))
        .send()
        .await;

    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS, "{}", res.text);
}
