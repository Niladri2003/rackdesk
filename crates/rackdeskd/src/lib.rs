//! The RackDesk server as a library: the router and its state.
//!
//! `main.rs` handles the command line and the listener. Everything else is
//! here, so the integration tests in `tests/` can call the router without a
//! network socket.

pub mod auth;
pub mod csrf;
pub mod error;
pub mod state;
pub mod tls;
mod ui;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue};
use axum::routing::{get, post};
use axum::{middleware, Json, Router};
use rd_api::ServerInfo;
use tower_http::set_header::SetResponseHeaderLayer;

pub use state::{AppState, Config};

/// Largest request body the API accepts. Every current body is a small JSON
/// object.
const MAX_BODY_BYTES: usize = 8 * 1024;

pub fn router(state: AppState) -> Router {
    // Routes that work without a session, or while the password must be changed.
    let session = Router::new()
        .route(
            "/session",
            get(auth::who_am_i)
                .post(auth::sign_in)
                .delete(auth::sign_out),
        )
        .route("/session/password", post(auth::change_password));

    // Every other API route needs a session whose password has been changed.
    let protected = Router::new()
        .route("/server", get(server_info))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_active_session,
        ));

    let api = session
        .merge(protected)
        .fallback(error::no_such_route)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        // API responses can hold account data. Browsers and proxies must not
        // store them.
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ));

    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api/v1", api)
        .fallback(ui::serve)
        .layer(middleware::from_fn(csrf::reject_cross_site))
        // Security headers on every response (docs/security/SECURITY.md, H1).
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; \
                 connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
            ),
        ))
        .with_state(state)
}

async fn server_info() -> Json<ServerInfo> {
    Json(ServerInfo {
        hostname: hostname(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

/// The name of this machine, or "server" if it can't be found.
pub fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_owned())
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "server".into())
}
