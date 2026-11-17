//! The RackDesk server as a library: the router and its state.
//!
//! `main.rs` handles the command line and the listener. Everything else is
//! here, so the integration tests in `tests/` can call the router without a
//! network socket.

pub mod auth;
pub mod csrf;
pub mod error;
pub mod files;
pub mod files_write;
pub mod state;
pub mod tls;
mod ui;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue};
use axum::routing::{get, patch, post, put};
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
        .route("/fs/list", get(files::list))
        .route("/fs/entry", get(files::entry))
        .route("/fs/places", get(files::places))
        .route("/fs/search", get(files::search))
        .route("/fs/download", get(files::download))
        .route("/fs/mkdir", post(files_write::mkdir))
        .route("/fs/rename", post(files_write::rename))
        .route("/fs/copy", post(files_write::copy))
        .route("/fs/delete", post(files_write::delete))
        .route(
            "/fs/trash",
            get(files_write::trash_list).post(files_write::trash),
        )
        .route("/fs/trash/restore", post(files_write::trash_restore))
        .route("/fs/trash/delete", post(files_write::trash_delete))
        .route("/fs/trash/empty", post(files_write::trash_empty))
        .route("/fs/uploads", post(files_write::upload_begin))
        // These 2 routes take file content, so they have larger body limits
        // than the rest of the API.
        .route(
            "/fs/file",
            put(files_write::save).layer(DefaultBodyLimit::max(files_write::MAX_SAVE_BYTES)),
        )
        .route(
            "/fs/uploads/{id}",
            patch(files_write::upload_chunk)
                .layer(DefaultBodyLimit::max(files_write::MAX_CHUNK_BYTES))
                .get(files_write::upload_get)
                .delete(files_write::upload_cancel),
        )
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
        // A file download sets a stricter policy of its own (files.rs).
        .layer(SetResponseHeaderLayer::if_not_present(
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
