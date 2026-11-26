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
use datenaow::modules::auth::domain::PasswordResetRepository;
use datenaow::modules::auth::infrastructure::PgPasswordResetRepository;
use datenaow::shared::clock::Clock;

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
async fn concurrent_requests_leave_exactly_one_live_token(pool: PgPool) {
    // #17: without serialising on the user row, each request's UPDATE missed
    // the others' uncommitted inserts and several links stayed live.
    const REQUESTS: u8 = 8;
    let (app, _) = registered(pool.clone()).await;

    // From different IPs, so the (email, IP) pair limit stays out of the way;
    // 8 stays under the per-email cap of 10.
    let mut set = tokio::task::JoinSet::new();
    for n in 1..=REQUESTS {
        let request = app
            .post(REQUEST)
            .peer(SocketAddr::new(client(n), 40000))
            .json(&json!({ "email": EMAIL }));
        set.spawn(async move { request.send().await });
    }
    let results = set.join_all().await;
    for res in &results {
        assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
    }

    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM password_reset_tokens WHERE used_at IS NULL AND expires_at > $1",
    )
    .bind(app.clock.now())
    .fetch_one(&pool)
    .await
    .expect("count live tokens");
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM password_reset_tokens")
        .fetch_one(&pool)
        .await
        .expect("count tokens");
    assert_eq!(live, 1, "exactly one live reset token");
    assert_eq!(total, i64::from(REQUESTS), "every request stored a token");

    // And exactly one of the emailed links works.
    let sent = app.mailer.wait_for(usize::from(REQUESTS)).await;
    assert_eq!(sent.len(), usize::from(REQUESTS));
    let mut accepted = 0;
    for email in &sent {
        if confirm(&app, &token_in(email), NEW_PASSWORD).await.status == StatusCode::NO_CONTENT {
            accepted += 1;
        }
    }
    assert_eq!(accepted, 1);
}

#[sqlx::test]
async fn concurrent_replace_for_user_leaves_exactly_one_unused_token(pool: PgPool) {
    // The repository alone, without the limiter in front, so the calls
    // overlap as much as the pool allows.
    const CALLS: u8 = 32;
    let (app, _) = registered(pool.clone()).await;
    let user_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(EMAIL)
        .fetch_one(&pool)
        .await
        .expect("user id");
    let repo = PgPasswordResetRepository::new(pool.clone());
    let now = app.clock.now();

    for round in 0..3u8 {
        let mut set = tokio::task::JoinSet::new();
        for n in 0..CALLS {
            let repo = repo.clone();
            let hash: [u8; 32] = Sha256::digest([round, n]).into();
            set.spawn(async move {
                repo.replace_for_user(user_id, hash, now + Duration::minutes(30), now)
                    .await
            });
        }
        for result in set.join_all().await {
            result.expect("replace_for_user");
        }

        let unused: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM password_reset_tokens WHERE user_id = $1 AND used_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("count unused tokens");
        assert_eq!(unused, 1, "round {round}");
    }
}

#[sqlx::test]
async fn new_password_set_decomposed_signs_in_composed(pool: PgPool) {
    // #17: "mật khẩu đẹp lắm" set with combining marks, typed precomposed.
    let nfd = "ma\u{0323}\u{0302}t kha\u{0302}\u{0309}u \u{0111}e\u{0323}p la\u{0306}\u{0301}m";
    let nfc = "m\u{1EAD}t kh\u{1EA9}u \u{0111}\u{1EB9}p l\u{1EAF}m";
    let (app, _) = registered(pool).await;
    let token = reset_token(&app, 1).await;

    let res = confirm(&app, &token, nfd).await;

    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.text);
    assert_eq!(login(&app, EMAIL, nfc).await.status, StatusCode::OK);
}

/// The id of `user_id`'s one unused reset token.
async fn unused_token_id(pool: &PgPool, user_id: uuid::Uuid) -> uuid::Uuid {
    sqlx::query_scalar(
        "SELECT id FROM password_reset_tokens WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("one unused token")
}

#[sqlx::test]
async fn concurrent_consume_and_replace_never_deadlock(pool: PgPool) {
    // Review of #17: `replace_for_user` locks users -> reset tokens; the
    // confirm transaction must use the same order, or a confirmation racing
    // a new request for the same user ends in `deadlock detected`.
    let (app, _) = registered(pool.clone()).await;
    let user_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(EMAIL)
        .fetch_one(&pool)
        .await
        .expect("user id");
    let repo = PgPasswordResetRepository::new(pool.clone());
    let now = app.clock.now();
    let expires = now + Duration::minutes(30);

    for i in 0..40u8 {
        repo.replace_for_user(user_id, Sha256::digest([0, i]).into(), expires, now)
            .await
            .expect("seed token");
        let token_id = unused_token_id(&pool, user_id).await;

        let consume = {
            let repo = repo.clone();
            tokio::spawn(async move {
                repo.consume_and_set_password(token_id, user_id, "$argon2id$fake", now)
                    .await
            })
        };
        let replace = {
            let repo = repo.clone();
            tokio::spawn(async move {
                repo.replace_for_user(user_id, Sha256::digest([1, i]).into(), expires, now)
                    .await
            })
        };
        let consumed = consume
            .await
            .expect("join")
            .expect("consume: no storage error");
        replace
            .await
            .expect("join")
            .expect("replace: no storage error");

        // Whichever ran first, the newest request's token is the one live link.
        let unused: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM password_reset_tokens WHERE user_id = $1 AND used_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("count unused");
        assert_eq!(unused, 1, "iteration {i}");
        if consumed {
            let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .expect("stored hash");
            assert_eq!(hash, "$argon2id$fake", "iteration {i}");
        }
    }
}

#[sqlx::test]
async fn confirm_racing_a_new_request_never_fails_with_500(pool: PgPool) {
    let (app, _) = registered(pool.clone()).await;
    for i in 0..10u8 {
        // A fresh hour each round, so the request limits never interfere.
        app.clock.advance(Duration::minutes(61));
        let token = reset_token(&app, usize::from(i) * 2 + 1).await;
        let password = format!("racing-password-{i:03}");

        let (confirmed, requested) = tokio::join!(
            confirm(&app, &token, &password),
            request_reset_from(&app, client(i + 1), EMAIL),
        );

        assert_eq!(
            requested.status,
            StatusCode::NO_CONTENT,
            "{}",
            requested.text
        );
        match confirmed.status {
            // Confirmed first: the new password works.
            StatusCode::NO_CONTENT => {
                assert_eq!(login(&app, EMAIL, &password).await.status, StatusCode::OK);
            }
            // The new request invalidated the link first.
            StatusCode::BAD_REQUEST => {
                assert_eq!(confirmed.body["error"]["code"], "invalid_or_expired_token");
            }
            other => panic!("round {i}: confirm answered {other}: {}", confirmed.text),
        }
        // Wait for the second email so the next round's numbering holds.
        assert_eq!(
            app.mailer.wait_for(usize::from(i) * 2 + 2).await.len(),
            usize::from(i) * 2 + 2
        );
    }
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
