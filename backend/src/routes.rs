//! Top-level router: every API route lives under `/api/v1`, wrapped in the
//! shared middleware stack.

use axum::body::Body;
use axum::error_handling::HandleErrorLayer;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Request, header};
use axum::response::Response;
use axum::routing::get;
use axum::{BoxError, Router, middleware};
use tower::ServiceBuilder;
use tower::timeout::TimeoutLayer;
use tower::timeout::error::Elapsed;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{Level, Span};

use crate::modules::auth;
use crate::shared::error::ApiError;
use crate::shared::{health, openapi};
use crate::state::AppState;

/// Maximum request body size (P9). Routes that accept uploads raise it locally.
const MAX_BODY_BYTES: usize = 65_536;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health::health))
        .route("/openapi.json", get(openapi::openapi_json))
        .nest(
            "/auth",
            auth::presentation::routes().layer(middleware::map_response(no_store)),
        )
        // Unknown paths under /api/v1 answer with the JSON envelope.
        .fallback(|| async { ApiError::NotFound });

    with_middleware(Router::new().nest("/api/v1", api), state)
}

/// Wraps `routes` in the shared middleware stack and attaches the state.
/// Public so integration tests can mount probe routes on the real stack.
pub fn with_middleware(routes: Router<AppState>, state: AppState) -> Router {
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
        .layer(TimeoutLayer::new(state.config.request_timeout));

    routes
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

/// Sets `Cache-Control: no-store` (P2), replacing any existing value, on every
/// response of the routes it wraps (tokens and account data must not be cached).
/// Errors from extractors inside those routes get it too.
pub async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Converts errors from fallible middleware (the timeout) into the JSON envelope.
async fn handle_middleware_error(error: BoxError) -> ApiError {
    if error.is::<Elapsed>() {
        ApiError::Timeout
    } else {
        ApiError::Internal(anyhow::anyhow!(error).context("unhandled middleware error"))
    }
}
