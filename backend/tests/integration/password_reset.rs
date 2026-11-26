//! Password reset (P5 as amended by DECISIONS #13, P6):
//! `POST /api/v1/auth/password-reset/request` and
//! `POST /api/v1/auth/password-reset/confirm`.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration as StdDuration;

use axum::http::StatusCode;
use chrono::Duration;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use datenaow::modules::auth::application::ports::OutgoingEmail;

use crate::common::{TestApp, TestResponse};

const REQUEST: &str = "/api/v1/auth/password-reset/request";
const CONFIRM: &str = "/api/v1/auth/password-reset/confirm";
const EMAIL: &str = "an@example.com";
const OLD_PASSWORD: &str = "old-password-123";
const NEW_PASSWORD: &str = "new-password-456";

async fn request_reset(app: &TestApp, email: &str) -> TestResponse {
    app.post(REQUEST)
        .json(&json!({ "email": email }))
        .send()
        .await
}

async fn request_reset_from(app: &TestApp, peer: IpAddr, email: &str) -> TestResponse {
    app.post(REQUEST)
        .peer(SocketAddr::new(peer, 40000))
        .json(&json!({ "email": email }))
        .send()
        .await
}

async fn confirm(app: &TestApp, token: &str, new_password: &str) -> TestResponse {
    app.post(CONFIRM)
        .json(&json!({ "token": token, "new_password": new_password }))
        .send()
        .await
}

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

async fn refresh(app: &TestApp, cookie: &str) -> TestResponse {
    app.post("/api/v1/auth/refresh")
        .cookie(&format!("dn_refresh={cookie}"))
        .send()
        .await
}

/// A `TestApp` with one account (`EMAIL` / `OLD_PASSWORD`); also returns its
/// refresh cookie.
async fn registered(pool: PgPool) -> (TestApp, String) {
    let app = TestApp::new(pool);
    let res = app.register(EMAIL, OLD_PASSWORD).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.text);
    let cookie = res.cookie_value("dn_refresh").expect("refresh cookie");
    (app, cookie)
}

/// The token from the reset link in `email`.
fn token_in(email: &OutgoingEmail) -> String {
    let (_, rest) = email
        .text_body
        .split_once("#token=")
        .expect("email contains a reset link");
    rest.split_whitespace()
        .next()
        .expect("token after #token=")
        .to_owned()
}

/// Requests a reset for `EMAIL` and returns the token from the `n`-th email.
async fn reset_token(app: &TestApp, n: usize) -> String {
    let res = request_reset(app, EMAIL).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
    let sent = app.mailer.wait_for(n).await;
    assert_eq!(sent.len(), n, "expected {n} emails");
    token_in(&sent[n - 1])
}

/// Gives a background send time to happen before asserting that none did.
async fn settle() {
    tokio::time::sleep(StdDuration::from_millis(100)).await;
}

// ---- request ----

#[sqlx::test]
async fn request_for_known_email_sends_one_email_with_fragment_link(pool: PgPool) {
    let (app, _) = registered(pool).await;

    let res = request_reset(&app, " An@Example.com ").await;

    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
    assert_eq!(res.text, "");
    let sent = app.mailer.wait_for(1).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to.as_str(), "an@example.com");
    assert_eq!(sent[0].subject, "Đặt lại mật khẩu dateNaow");
    assert!(
        sent[0]
            .text_body
            .contains("http://localhost:5173/reset-password#token="),
        "{}",
        sent[0].text_body
    );
    let token = token_in(&sent[0]);
    assert_eq!(token.len(), 64);
    settle().await;
    assert_eq!(app.mailer.sent().len(), 1, "exactly one email");
}

#[sqlx::test]
async fn request_for_unknown_email_returns_204_and_sends_nothing(pool: PgPool) {
    let (app, _) = registered(pool).await;

    let res = request_reset(&app, "nobody@example.com").await;

    assert_eq!(res.status, StatusCode::NO_CONTENT);
    settle().await;
    assert!(app.mailer.sent().is_empty());
}

#[sqlx::test]
async fn unknown_email_response_is_identical_to_known(pool: PgPool) {
    let (app, _) = registered(pool).await;

    let known = request_reset(&app, EMAIL).await;
    let unknown = request_reset(&app, "nobody@example.com").await;

    assert_eq!(unknown.status, known.status);
    assert_eq!(unknown.text, known.text);
    let names = |res: &TestResponse| {
        let mut names: Vec<String> = res
            .headers
            .keys()
            .map(|name| name.as_str().to_owned())
            .collect();
        names.sort();
        names
    };
    assert_eq!(names(&unknown), names(&known));
    assert_eq!(unknown.headers["cache-control"], "no-store");
    assert_eq!(known.headers["cache-control"], "no-store");
}

#[sqlx::test]
async fn request_with_malformed_email_returns_422(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = request_reset(&app, "not-an-email").await;

    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "validation_error");
    assert_eq!(res.body["error"]["fields"]["email"][0], "invalid_email");
}

#[sqlx::test]
async fn request_rejects_unknown_fields(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app
        .post(REQUEST)
        .json(&json!({ "email": EMAIL, "user_id": "x" }))
        .send()
        .await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.body["error"]["code"], "invalid_json");
}

#[sqlx::test]
async fn fourth_request_from_one_ip_in_an_hour_returns_429_but_other_ips_still_work(pool: PgPool) {
    let (app, _) = registered(pool).await;

    for _ in 0..3 {
        let res = request_reset(&app, EMAIL).await;
        assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
    }
    let fourth = request_reset(&app, EMAIL).await;

    assert_eq!(
        fourth.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        fourth.text
    );
    assert_eq!(fourth.body["error"]["code"], "rate_limited");
    assert_eq!(fourth.headers["retry-after"], "3600");
    settle().await;
    assert_eq!(
        app.mailer.sent().len(),
        3,
        "the blocked request sends nothing"
    );

    // Another IP exhausting its pair limit cannot block the owner's recovery.
    let owner = request_reset_from(&app, client(1), EMAIL).await;
    assert_eq!(owner.status, StatusCode::NO_CONTENT, "{}", owner.text);
    assert_eq!(app.mailer.wait_for(4).await.len(), 4);
}

#[sqlx::test]
async fn hammering_reset_requests_from_one_ip_does_not_reach_the_global_limit(pool: PgPool) {
    let (app, _) = registered(pool).await;
    for i in 0..10 {
        let expected = if i < 3 {
            StatusCode::NO_CONTENT
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        let res = request_reset_from(&app, client(1), EMAIL).await;
        assert_eq!(res.status, expected, "request {i}: {}", res.text);
    }

    // Only the three requests that passed the pair limit count globally, so
    // seven more from other IPs still fit in the 10-per-email cap.
    for n in 2..=8 {
        let res = request_reset_from(&app, client(n), EMAIL).await;
        assert_eq!(res.status, StatusCode::NO_CONTENT, "ip {n}: {}", res.text);
    }
    assert_eq!(app.mailer.wait_for(10).await.len(), 10);
}

#[sqlx::test]
async fn eleventh_request_for_one_email_across_many_ips_returns_429(pool: PgPool) {
    let (app, _) = registered(pool).await;
    for n in 1..=10 {
        let res = request_reset_from(&app, client(n), EMAIL).await;
        assert_eq!(res.status, StatusCode::NO_CONTENT, "ip {n}: {}", res.text);
    }

    let eleventh = request_reset_from(&app, client(11), EMAIL).await;

    assert_eq!(
        eleventh.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        eleventh.text
    );
    assert_eq!(eleventh.headers["retry-after"], "3600");
    settle().await;
    assert_eq!(
        app.mailer.sent().len(),
        10,
        "the blocked request sends nothing"
    );

    // The cap is per email: another email from a fresh IP is unaffected.
    let other = request_reset_from(&app, client(12), "other@example.com").await;
    assert_eq!(other.status, StatusCode::NO_CONTENT);
}

#[sqlx::test]
async fn eleventh_request_from_one_ip_returns_429(pool: PgPool) {
    let app = TestApp::new(pool);
    let ip: IpAddr = "198.51.100.9".parse().unwrap();

    for i in 0..10 {
        let res = request_reset_from(&app, ip, &format!("user{i}@example.com")).await;
        assert_eq!(
            res.status,
            StatusCode::NO_CONTENT,
            "request {i}: {}",
            res.text
        );
    }
    let eleventh = request_reset_from(&app, ip, "user10@example.com").await;
    assert_eq!(eleventh.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(eleventh.headers["retry-after"], "3600");

    let other_ip: IpAddr = "198.51.100.10".parse().unwrap();
    let other = request_reset_from(&app, other_ip, "user10@example.com").await;
    assert_eq!(
        other.status,
        StatusCode::NO_CONTENT,
        "other IPs are unaffected"
    );
}

#[sqlx::test]
async fn request_still_returns_204_when_sending_fails(pool: PgPool) {
    let (app, _) = registered(pool).await;
    app.mailer.fail_sends(true);

    let res = request_reset(&app, EMAIL).await;

    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let attempts = app.mailer.wait_for_attempts(1).await;
    assert_eq!(attempts, 1, "the send was attempted");
    assert!(app.mailer.sent().is_empty());
}

#[sqlx::test]
async fn reset_tokens_are_stored_only_as_sha256_hashes(pool: PgPool) {
    let (app, _) = registered(pool.clone()).await;
    let token = reset_token(&app, 1).await;

    let hashes: Vec<Vec<u8>> = sqlx::query_scalar("SELECT token_hash FROM password_reset_tokens")
        .fetch_all(&pool)
        .await
        .expect("query");

    assert_eq!(hashes.len(), 1);
    let expected: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    assert_eq!(hashes[0], expected.to_vec());
}

// ---- confirm ----

#[sqlx::test]
async fn confirm_sets_new_password_and_revokes_sessions(pool: PgPool) {
    let (app, old_cookie) = registered(pool).await;
    let token = reset_token(&app, 1).await;

    let res = confirm(&app, &token, NEW_PASSWORD).await;

    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
    assert_eq!(res.headers["cache-control"], "no-store");
    assert_eq!(
        login(&app, EMAIL, OLD_PASSWORD).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, EMAIL, NEW_PASSWORD).await.status,
        StatusCode::OK
    );
    let old = refresh(&app, &old_cookie).await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED, "{}", old.text);
    assert_eq!(old.body["error"]["code"], "invalid_refresh_token");
}

#[sqlx::test]
async fn token_is_single_use(pool: PgPool) {
    let (app, _) = registered(pool).await;
    let token = reset_token(&app, 1).await;

    let first = confirm(&app, &token, NEW_PASSWORD).await;
    let second = confirm(&app, &token, "another-password-789").await;

    assert_eq!(first.status, StatusCode::NO_CONTENT, "{}", first.text);
    assert_eq!(second.status, StatusCode::BAD_REQUEST);
    assert_eq!(second.body["error"]["code"], "invalid_or_expired_token");
    assert_eq!(
        login(&app, EMAIL, NEW_PASSWORD).await.status,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn concurrent_confirms_consume_the_token_once(pool: PgPool) {
    let (app, _) = registered(pool).await;
    let token = reset_token(&app, 1).await;

    let (a, b) = tokio::join!(
        confirm(&app, &token, "first-password-111"),
        confirm(&app, &token, "second-password-222"),
    );

    let mut statuses = [a.status.as_u16(), b.status.as_u16()];
    statuses.sort_unstable();
    assert_eq!(statuses, [204, 400], "{} / {}", a.text, b.text);
    let winner = if a.status == StatusCode::NO_CONTENT {
        "first-password-111"
    } else {
        "second-password-222"
    };
    let loser = if winner == "first-password-111" {
        "second-password-222"
    } else {
        "first-password-111"
    };
    assert_eq!(login(&app, EMAIL, winner).await.status, StatusCode::OK);
    assert_eq!(
        login(&app, EMAIL, loser).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn token_expires_after_30_minutes(pool: PgPool) {
    let (app, _) = registered(pool).await;
    let token = reset_token(&app, 1).await;

    app.clock.advance(Duration::minutes(30));
    let res = confirm(&app, &token, NEW_PASSWORD).await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST, "{}", res.text);
    assert_eq!(res.body["error"]["code"], "invalid_or_expired_token");
    assert_eq!(
        login(&app, EMAIL, OLD_PASSWORD).await.status,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn token_still_works_just_before_30_minutes(pool: PgPool) {
    let (app, _) = registered(pool).await;
    let token = reset_token(&app, 1).await;

    app.clock
        .advance(Duration::minutes(30) - Duration::seconds(1));
    let res = confirm(&app, &token, NEW_PASSWORD).await;

    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
}

#[sqlx::test]
async fn new_request_invalidates_previous_token(pool: PgPool) {
    let (app, _) = registered(pool).await;
    let first = reset_token(&app, 1).await;
    let second = reset_token(&app, 2).await;
    assert_ne!(first, second);

    let old = confirm(&app, &first, NEW_PASSWORD).await;
    let new = confirm(&app, &second, NEW_PASSWORD).await;

    assert_eq!(old.status, StatusCode::BAD_REQUEST);
    assert_eq!(old.body["error"]["code"], "invalid_or_expired_token");
    assert_eq!(new.status, StatusCode::NO_CONTENT, "{}", new.text);
}

#[sqlx::test]
async fn short_new_password_returns_422_and_token_stays_usable(pool: PgPool) {
    let (app, _) = registered(pool).await;
    let token = reset_token(&app, 1).await;

    let short = confirm(&app, &token, "short").await;
    assert_eq!(
        short.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        short.text
    );
    assert_eq!(
        short.body["error"]["fields"]["new_password"][0],
        "too_short"
    );

    let retry = confirm(&app, &token, NEW_PASSWORD).await;
    assert_eq!(retry.status, StatusCode::NO_CONTENT, "{}", retry.text);
}

#[sqlx::test]
async fn common_new_password_returns_422_too_common_and_token_stays_usable(pool: PgPool) {
    let (app, _) = registered(pool).await;
    let token = reset_token(&app, 1).await;

    for common in ["password", "Iloveyou", "anhyeuem123"] {
        let res = confirm(&app, &token, common).await;
        assert_eq!(
            res.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{common}: {}",
            res.text
        );
        assert_eq!(res.body["error"]["code"], "validation_error");
        assert_eq!(
            res.body["error"]["fields"],
            json!({ "new_password": ["too_common"] }),
            "{common}"
        );
    }
    // The rejected password was not set.
    assert_eq!(
        login(&app, EMAIL, "password").await.status,
        StatusCode::UNAUTHORIZED
    );

    let retry = confirm(&app, &token, NEW_PASSWORD).await;
    assert_eq!(retry.status, StatusCode::NO_CONTENT, "{}", retry.text);
    assert_eq!(
        login(&app, EMAIL, NEW_PASSWORD).await.status,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn invalid_password_is_reported_before_a_bad_token(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = confirm(&app, "not-a-token", "short").await;

    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.body["error"]["fields"]["new_password"][0], "too_short");
}

#[sqlx::test]
async fn malformed_or_unknown_token_returns_400(pool: PgPool) {
    let app = TestApp::new(pool);
    let unknown = "0123456789abcdef".repeat(4);

    for token in ["not-a-token", "", unknown.as_str()] {
        let res = confirm(&app, token, NEW_PASSWORD).await;
        assert_eq!(
            res.status,
            StatusCode::BAD_REQUEST,
            "{token:?}: {}",
            res.text
        );
        assert_eq!(res.body["error"]["code"], "invalid_or_expired_token");
        assert_eq!(res.headers["cache-control"], "no-store");
    }
}

#[sqlx::test]
async fn confirm_rejects_unknown_fields(pool: PgPool) {
    let app = TestApp::new(pool);

    let res = app
        .post(CONFIRM)
        .json(&json!({ "token": "x", "new_password": NEW_PASSWORD, "email": EMAIL }))
        .send()
        .await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.body["error"]["code"], "invalid_json");
}

#[sqlx::test]
async fn old_cookie_after_reset_returns_401_without_revoking_new_session(pool: PgPool) {
    let (app, old_cookie) = registered(pool).await;
    let token = reset_token(&app, 1).await;
    assert_eq!(
        confirm(&app, &token, NEW_PASSWORD).await.status,
        StatusCode::NO_CONTENT
    );
    let signed_in = login(&app, EMAIL, NEW_PASSWORD).await;
    assert_eq!(signed_in.status, StatusCode::OK);
    let new_cookie = signed_in.cookie_value("dn_refresh").expect("new cookie");

    app.clock.advance(Duration::seconds(31));
    let old = refresh(&app, &old_cookie).await;
    let new = refresh(&app, &new_cookie).await;

    assert_eq!(old.status, StatusCode::UNAUTHORIZED, "{}", old.text);
    assert_eq!(new.status, StatusCode::OK, "{}", new.text);
}

#[sqlx::test]
async fn reset_clears_the_login_pair_counter_of_the_resetting_ip(pool: PgPool) {
    let (app, _) = registered(pool).await;
    for _ in 0..5 {
        assert_eq!(
            login(&app, EMAIL, "wrong-password").await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        login(&app, EMAIL, OLD_PASSWORD).await.status,
        StatusCode::TOO_MANY_REQUESTS,
        "the (email, IP) pair is limited before the reset"
    );

    // Requested and confirmed from the same (default) IP.
    let token = reset_token(&app, 1).await;
    assert_eq!(
        confirm(&app, &token, NEW_PASSWORD).await.status,
        StatusCode::NO_CONTENT
    );

    let res = login(&app, EMAIL, NEW_PASSWORD).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
}

#[sqlx::test]
async fn reset_clears_the_global_login_counter_but_not_other_ips_pairs(pool: PgPool) {
    let (app, _) = registered(pool).await;
    for n in 1..=10 {
        for _ in 0..5 {
            login_from(&app, client(n), EMAIL, "wrong-password").await;
        }
    }
    assert_eq!(
        login_from(&app, client(11), EMAIL, OLD_PASSWORD)
            .await
            .status,
        StatusCode::TOO_MANY_REQUESTS,
        "the email's global counter is full before the reset"
    );

    let token = reset_token(&app, 1).await;
    assert_eq!(
        confirm(&app, &token, NEW_PASSWORD).await.status,
        StatusCode::NO_CONTENT
    );

    let res = login_from(&app, client(12), EMAIL, NEW_PASSWORD).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.text);
    // An attacker's own pair counter is kept.
    assert_eq!(
        login_from(&app, client(1), EMAIL, NEW_PASSWORD)
            .await
            .status,
        StatusCode::TOO_MANY_REQUESTS
    );
}
