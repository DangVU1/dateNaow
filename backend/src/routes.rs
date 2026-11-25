//! Top-level router: every API route lives under `/api/v1`, wrapped in the
//! shared middleware stack.

use std::time::Duration;

use axum::body::Body;
use axum::error_handling::HandleErrorLayer;
use axum::extract::DefaultBodyLimit;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::{BoxError, Router};
use tower::ServiceBuilder;
use tower::timeout::TimeoutLayer;
use tower::timeout::error::Elapsed;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{Level, Span};

use crate::shared::health;
use crate::state::AppState;

/// Maximum request body size (P9). Routes that accept uploads raise it locally.
const MAX_BODY_BYTES: usize = 65_536;
/// Maximum time a request may take before it is answered with 408 (P9).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub fn build_router(state: AppState) -> Router {
    let api = Router::new().route("/health", get(health::health));

    // One ServiceBuilder so the layers run in the order written, outermost first.
    let middleware = ServiceBuilder::new()
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(request_span)
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(HandleErrorLayer::new(handle_middleware_error))
        .layer(TimeoutLayer::new(REQUEST_TIMEOUT));

    Router::new()
        .nest("/api/v1", api)
        // Read by axum's body extractors; returns a rejection instead of a
        // non-JSON 413 like tower-http's RequestBodyLimitLayer would.
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware)
        .with_state(state)
}

/// Span for each request, carrying the request id set by `SetRequestIdLayer`.
fn request_span(request: &Request<Body>) -> Span {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-");
    tracing::info_span!(
        "request",
        request_id = %request_id,
        method = %request.method(),
        uri = %request.uri(),
    )
}

/// Converts errors from fallible middleware (the timeout) into responses.
/// Task 2 replaces the bare status codes with the JSON error envelope.
async fn handle_middleware_error(error: BoxError) -> StatusCode {
    if error.is::<Elapsed>() {
        StatusCode::REQUEST_TIMEOUT
    } else {
        tracing::error!(%error, "unhandled middleware error");
        StatusCode::INTERNAL_SERVER_ERROR
    }
}
