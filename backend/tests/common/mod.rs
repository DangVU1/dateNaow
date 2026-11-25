//! Shared helpers for integration tests: an in-process `TestApp`, a fake clock
//! and the test configuration.
//!
//! Not every helper is used by every test module yet, so dead-code warnings
//! are silenced for this file only.
#![allow(dead_code)]

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use chrono::{DateTime, Utc};
use http_body_util::BodyExt;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Notify;
use tower::ServiceExt;

use datenaow::config::Config;
use datenaow::modules::auth::application::ports::{EmailSender, OutgoingEmail, SendEmailError};
use datenaow::routes::with_middleware;
use datenaow::shared::clock::Clock;
use datenaow::{AppState, build_router};

/// Peer address that every test request appears to come from.
pub const TEST_PEER_ADDR: ([u8; 4], u16) = ([203, 0, 113, 7], 40000);

/// Configuration used by every `TestApp`. The pool comes from `#[sqlx::test]`,
/// so `database_url` is never used to connect.
pub fn test_config() -> Config {
    Config {
        database_url: "postgres://unused-in-tests".to_owned(),
        bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
        app_base_url: "http://localhost:5173".to_owned(),
        request_timeout: Duration::from_secs(15),
        jwt_secret: "test-jwt-secret-that-is-at-least-32-bytes".to_owned(),
        access_token_ttl_secs: 900,
        refresh_token_ttl_days: 30,
        // Small Argon2 parameters keep password hashing fast in tests.
        argon2_memory_kib: 1024,
        argon2_iterations: 1,
        argon2_parallelism: 1,
        cookie_secure: false,
        trust_proxy: false,
        smtp_host: "unused-in-tests".to_owned(),
        smtp_port: 1025,
        smtp_from: "dateNaow <no-reply@datenaow.local>".to_owned(),
        reset_token_ttl_mins: 30,
    }
}

/// Longest time `FakeMailer::wait_for` waits for background sends.
const MAIL_WAIT: Duration = Duration::from_secs(2);

/// An `EmailSender` that keeps every email in memory. Emails are sent from a
/// background task, so tests wait for them with `wait_for`.
#[derive(Debug, Default)]
pub struct FakeMailer {
    sent: Mutex<Vec<OutgoingEmail>>,
    attempts: AtomicUsize,
    fail: AtomicBool,
    notify: Notify,
}

impl FakeMailer {
    /// Every email sent so far.
    pub fn sent(&self) -> Vec<OutgoingEmail> {
        self.sent
            .lock()
            .expect("fake mailer mutex poisoned")
            .clone()
    }

    /// Waits up to 2 seconds until at least `n` emails were sent, then returns
    /// all of them (possibly fewer than `n` on timeout).
    pub async fn wait_for(&self, n: usize) -> Vec<OutgoingEmail> {
        self.wait_until(|| self.sent().len() >= n).await;
        self.sent()
    }

    /// Waits up to 2 seconds until at least `n` sends were attempted
    /// (successful or not), then returns the number of attempts.
    pub async fn wait_for_attempts(&self, n: usize) -> usize {
        self.wait_until(|| self.attempts.load(Ordering::SeqCst) >= n)
            .await;
        self.attempts.load(Ordering::SeqCst)
    }

    /// Makes every later send fail.
    pub fn fail_sends(&self, fail: bool) {
        self.fail.store(fail, Ordering::SeqCst);
    }

    async fn wait_until(&self, done: impl Fn() -> bool) {
        let deadline = tokio::time::Instant::now() + MAIL_WAIT;
        loop {
            // Created before the check, and `notify_waiters` wakes every
            // future created earlier, so a send in between is not missed.
            let notified = self.notify.notified();
            if done() {
                return;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return;
            }
        }
    }
}

impl EmailSender for FakeMailer {
    fn send(
        &self,
        email: OutgoingEmail,
    ) -> Pin<Box<dyn Future<Output = Result<(), SendEmailError>> + Send + '_>> {
        Box::pin(async move {
            let result = if self.fail.load(Ordering::SeqCst) {
                Err(SendEmailError::Delivery("fake failure"))
            } else {
                self.sent
                    .lock()
                    .expect("fake mailer mutex poisoned")
                    .push(email);
                Ok(())
            };
            self.attempts.fetch_add(1, Ordering::SeqCst);
            self.notify.notify_waiters();
            result
        })
    }
}

/// A pool that never connects unless a query runs. For tests that exercise
/// the HTTP stack only and therefore use `#[tokio::test]`, not `#[sqlx::test]`.
pub fn lazy_pool() -> PgPool {
    PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .expect("valid lazy pool URL")
}

/// The default start time of `FakeClock`: 2026-11-25T09:00:00Z.
pub fn default_start() -> DateTime<Utc> {
    "2026-11-25T09:00:00Z"
        .parse()
        .expect("valid RFC 3339 timestamp")
}

/// A clock that only moves when the test calls `advance`.
#[derive(Debug)]
pub struct FakeClock {
    now: Mutex<DateTime<Utc>>,
}

impl FakeClock {
    pub fn new(start: DateTime<Utc>) -> Self {
        Self {
            now: Mutex::new(start),
        }
    }

    pub fn advance(&self, by: chrono::Duration) {
        let mut now = self.now.lock().expect("fake clock mutex poisoned");
        *now += by;
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new(default_start())
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().expect("fake clock mutex poisoned")
    }
}

/// The real `AppState` with a fake clock and a fake mailer.
fn test_state(pool: PgPool, config: Config) -> (AppState, Arc<FakeClock>, Arc<FakeMailer>) {
    let clock = Arc::new(FakeClock::default());
    let mailer = Arc::new(FakeMailer::default());
    let state =
        AppState::new(config, pool, clock.clone(), mailer.clone()).expect("build app state");
    (state, clock, mailer)
}

/// The application router wired to a test database, called in-process.
pub struct TestApp {
    router: Router,
    pub clock: Arc<FakeClock>,
    pub mailer: Arc<FakeMailer>,
}

impl TestApp {
    pub fn new(pool: PgPool) -> Self {
        Self::with_config(pool, test_config())
    }

    pub fn with_config(pool: PgPool, config: Config) -> Self {
        let (state, clock, mailer) = test_state(pool, config);
        let router = build_router(state).layer(MockConnectInfo(SocketAddr::from(TEST_PEER_ADDR)));
        Self {
            router,
            clock,
            mailer,
        }
    }

    /// Mounts test-only `routes` (at the root, not under `/api/v1`) on the
    /// real middleware stack, e.g. to probe extractors or the timeout.
    pub fn with_routes(pool: PgPool, config: Config, routes: Router<AppState>) -> Self {
        let (state, clock, mailer) = test_state(pool, config);
        let router =
            with_middleware(routes, state).layer(MockConnectInfo(SocketAddr::from(TEST_PEER_ADDR)));
        Self {
            router,
            clock,
            mailer,
        }
    }

    pub fn get(&self, path: &str) -> TestRequest {
        self.request(Method::GET, path)
    }

    pub fn post(&self, path: &str) -> TestRequest {
        self.request(Method::POST, path)
    }

    pub fn patch(&self, path: &str) -> TestRequest {
        self.request(Method::PATCH, path)
    }

    /// `POST /api/v1/auth/register` with the given email and password.
    pub async fn register(&self, email: &str, password: &str) -> TestResponse {
        self.post("/api/v1/auth/register")
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await
    }

    fn request(&self, method: Method, path: &str) -> TestRequest {
        TestRequest {
            router: self.router.clone(),
            method,
            path: path.to_owned(),
            headers: Vec::new(),
            cookies: Vec::new(),
            body: None,
            peer: None,
        }
    }
}

/// A request being built; call `send` to run it through the router.
pub struct TestRequest {
    router: Router,
    method: Method,
    path: String,
    headers: Vec<(String, String)>,
    cookies: Vec<String>,
    body: Option<Vec<u8>>,
    peer: Option<SocketAddr>,
}

impl TestRequest {
    /// Sets a JSON body and `Content-Type: application/json`.
    pub fn json(mut self, value: &serde_json::Value) -> Self {
        let bytes = serde_json::to_vec(value).expect("serialize JSON body");
        self.headers.push((
            header::CONTENT_TYPE.as_str().to_owned(),
            "application/json".to_owned(),
        ));
        self.body = Some(bytes);
        self
    }

    /// Sets a raw body without touching any header, for malformed or
    /// oversized payloads.
    pub fn body(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.body = Some(bytes.into());
        self
    }

    /// Sets `Authorization: Bearer <token>`.
    pub fn bearer(self, token: &str) -> Self {
        self.header(header::AUTHORIZATION.as_str(), &format!("Bearer {token}"))
    }

    /// Adds a cookie pair such as `"dn_refresh=abc"`; several calls are joined
    /// into one `Cookie` header.
    pub fn cookie(mut self, pair: &str) -> Self {
        self.cookies.push(pair.to_owned());
        self
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Makes this request come from `addr` instead of `TEST_PEER_ADDR`. A real
    /// `ConnectInfo` extension takes precedence over `MockConnectInfo`.
    pub fn peer(mut self, addr: SocketAddr) -> Self {
        self.peer = Some(addr);
        self
    }

    pub async fn send(self) -> TestResponse {
        let mut builder = Request::builder().method(self.method).uri(&self.path);
        for (name, value) in &self.headers {
            builder = builder.header(name, value);
        }
        if !self.cookies.is_empty() {
            builder = builder.header(header::COOKIE, self.cookies.join("; "));
        }
        let body = self.body.map(Body::from).unwrap_or_else(Body::empty);
        let mut request = builder.body(body).expect("build test request");
        if let Some(addr) = self.peer {
            request.extensions_mut().insert(ConnectInfo(addr));
        }

        let response = self
            .router
            .oneshot(request)
            .await
            .expect("router is infallible");

        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("read response body")
            .to_bytes();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);

        TestResponse {
            status,
            headers,
            body,
            text,
        }
    }
}

/// A finished response. `body` is `Value::Null` when the body is not JSON.
#[derive(Debug)]
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: serde_json::Value,
    pub text: String,
}

impl TestResponse {
    /// The whole `Set-Cookie` header value for the named cookie, attributes included.
    pub fn set_cookie(&self, name: &str) -> Option<String> {
        let prefix = format!("{name}=");
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with(&prefix))
            .map(str::to_owned)
    }

    /// Only the value of the named cookie from its `Set-Cookie` header.
    pub fn cookie_value(&self, name: &str) -> Option<String> {
        let header = self.set_cookie(name)?;
        let pair = header.split(';').next()?;
        pair.split_once('=').map(|(_, value)| value.to_owned())
    }
}
