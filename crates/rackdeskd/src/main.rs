//! `rackdeskd`: one binary that serves the API and the web UI.
//!
//! M0 scope: start-up (data dir, random initial admin password), `/healthz`,
//! `/api/v1/server`, and the embedded UI. Sign-in, sessions and the files API
//! land in M1 and M3 (docs/ROADMAP.md). The files API stays unrouted until
//! sessions exist, so nothing here can read files without signing in.

mod ui;

use anyhow::Context;
use axum::http::{header, HeaderValue};
use axum::{routing::get, Json, Router};
use clap::Parser;
use rd_api::ServerInfo;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::set_header::SetResponseHeaderLayer;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Address to listen on. Localhost by default: exposing it is a deliberate choice.
    #[arg(long, env = "RACKDESK_LISTEN", default_value = "127.0.0.1:7171")]
    listen: SocketAddr,
    /// Where credentials and settings live. Production: /var/lib/rackdesk.
    #[arg(long, env = "RACKDESK_DATA_DIR", default_value = "./dev-data")]
    data_dir: PathBuf,
    /// The folder the file browser shows as "/". Used from M3.
    #[arg(long, env = "RACKDESK_FILES_ROOT")]
    files_root: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rackdeskd=info".into()),
        )
        .init();
    let args = Args::parse();

    let store = rd_auth::CredentialStore::new(&args.data_dir);
    let (_creds, initial) = store
        .load_or_init()
        .context("loading or creating the admin account")?;
    if let Some(rd_auth::InitialPassword(pw)) = initial {
        // Printed once, to the terminal only. Never logged through tracing,
        // so it doesn't end up in log files or journald.
        println!();
        println!("  RackDesk admin account created");
        println!("  username: {}", rd_auth::ADMIN_USER);
        println!("  password: {pw}");
        println!("  You'll be asked to change it on first sign-in.");
        println!();
    }

    if let Some(root) = &args.files_root {
        rd_fs::Root::open(root)
            .with_context(|| format!("opening files root {}", root.display()))?;
    }

    let app = router();
    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("binding {}", args.listen))?;
    if !args.listen.ip().is_loopback() {
        tracing::warn!(addr = %args.listen, "listening on a non-loopback address; HTTPS arrives in M1, so use a TLS proxy until then");
    }
    tracing::info!(addr = %args.listen, "rackdeskd listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn router() -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/v1/server", get(server_info))
        .fallback(ui::serve)
        // Baseline security headers on every response. The CSP is tightened
        // further once the real UI exists (docs/security/SECURITY.md, H1).
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
}

async fn server_info() -> Json<ServerInfo> {
    Json(ServerInfo {
        hostname: hostname(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_owned())
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "server".into())
}
