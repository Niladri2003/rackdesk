//! Serves the React build (`web/dist`) from inside the binary.
//!
//! Release builds embed the files at compile time, so the server is a single
//! file. Debug builds read them from disk, so `npm run build` shows up without
//! recompiling Rust. If `web/dist` doesn't exist yet, a placeholder page is served.

use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct Assets;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "no such API route").into_response();
    }
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // Vite puts content hashes in asset names, so those can be cached forever.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return (
            [
                (header::CONTENT_TYPE, mime.as_ref().to_owned()),
                (header::CACHE_CONTROL, cache.to_owned()),
            ],
            file.data,
        )
            .into_response();
    }
    // Client-side routes fall back to the app shell.
    match Assets::get("index.html") {
        Some(index) => Html(index.data).into_response(),
        None => Html(PLACEHOLDER).into_response(),
    }
}

const PLACEHOLDER: &str = "<!doctype html><meta charset=utf-8><title>RackDesk</title>\
<p style=\"font:16px system-ui;margin:3rem\">rackdeskd is running. Build the UI with \
<code>cd web &amp;&amp; npm run build</code>.</p>";
