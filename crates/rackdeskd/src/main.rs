//! `rackdeskd`: one binary that serves the API and the web UI.
//!
//! This file handles the command line, the first start and the listener. The
//! routes are in `lib.rs`.

use anyhow::Context;
use clap::{Parser, Subcommand};
use rackdeskd::tls::{self, TlsMode};
use rackdeskd::{AppState, Config};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Address to listen on. Localhost by default: exposing it is a deliberate choice.
    #[arg(long, env = "RACKDESK_LISTEN", default_value = "127.0.0.1:7171")]
    listen: SocketAddr,
    /// Where credentials and settings live. Production: /var/lib/rackdesk.
    #[arg(
        long,
        env = "RACKDESK_DATA_DIR",
        default_value = "./dev-data",
        global = true
    )]
    data_dir: PathBuf,
    /// The folder the file browser shows as "/". No route uses it yet.
    #[arg(long, env = "RACKDESK_FILES_ROOT")]
    files_root: Option<PathBuf>,
    /// When to use HTTPS. "auto" uses HTTPS unless the listen address is loopback.
    #[arg(long, env = "RACKDESK_TLS", value_enum, default_value = "auto")]
    tls: TlsMode,
    /// Your own certificate chain (PEM). Without it, a self-signed one is made.
    #[arg(long, env = "RACKDESK_TLS_CERT", requires = "tls_key")]
    tls_cert: Option<PathBuf>,
    /// The private key for --tls-cert (PEM).
    #[arg(long, env = "RACKDESK_TLS_KEY", requires = "tls_cert")]
    tls_key: Option<PathBuf>,
    /// Set this when a reverse proxy with HTTPS forwards requests to RackDesk.
    /// The client address is then read from X-Forwarded-For, and the session
    /// cookie is marked Secure.
    #[arg(long, env = "RACKDESK_BEHIND_PROXY")]
    behind_proxy: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Replace the admin password with a new random one and print it.
    /// Run it on the server, as root or as the user that runs rackdeskd.
    ResetPassword,
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

    if let Some(Command::ResetPassword) = args.command {
        let (_, password) = store.reset().context("resetting the admin password")?;
        print_password("RackDesk admin password reset", &password);
        println!("  A running server signs out every session at its next request.");
        println!();
        return Ok(());
    }

    let (creds, initial) = store
        .load_or_init()
        .context("loading or creating the admin account")?;
    if let Some(password) = initial {
        print_password("RackDesk admin account created", &password);
    }

    if let Some(root) = &args.files_root {
        rd_fs::Root::open(root)
            .with_context(|| format!("opening files root {}", root.display()))?;
    }

    let loopback = args.listen.ip().is_loopback();
    let use_tls = match args.tls {
        TlsMode::On => true,
        TlsMode::Off => false,
        TlsMode::Auto => !loopback || args.tls_cert.is_some(),
    };
    if !use_tls && !loopback && !args.behind_proxy {
        tracing::warn!(
            addr = %args.listen,
            "plain HTTP on a non-loopback address: passwords and sessions can be read on the network"
        );
    }

    let state = AppState::new(
        Config {
            data_dir: args.data_dir.clone(),
            https: use_tls || args.behind_proxy,
            behind_proxy: args.behind_proxy,
        },
        creds,
    )
    .await
    .context("preparing the server state")?;
    let app = rackdeskd::router(state).into_make_service_with_connect_info::<SocketAddr>();

    if use_tls {
        let files = match (&args.tls_cert, &args.tls_key) {
            (Some(cert), Some(key)) => tls::TlsFiles {
                cert: cert.clone(),
                key: key.clone(),
            },
            _ => {
                let files = tls::self_signed(&args.data_dir, &certificate_names(&args.listen))?;
                tracing::info!(
                    sha256 = %tls::fingerprint(&files.cert)?,
                    "using a self-signed certificate; the browser will warn. Compare this fingerprint with the one the browser shows"
                );
                files
            }
        };
        let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(&files.cert, &files.key)
            .await
            .with_context(|| format!("loading the certificate {}", files.cert.display()))?;
        let handle = axum_server::Handle::new();
        let on_signal = handle.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            on_signal.graceful_shutdown(Some(Duration::from_secs(10)));
        });
        tracing::info!("rackdeskd listening on https://{}", args.listen);
        axum_server::bind_rustls(args.listen, config)
            .handle(handle)
            .serve(app)
            .await
            .with_context(|| format!("serving on {}", args.listen))?;
    } else {
        let listener = tokio::net::TcpListener::bind(args.listen)
            .await
            .with_context(|| format!("binding {}", args.listen))?;
        tracing::info!("rackdeskd listening on http://{}", args.listen);
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
    }
    Ok(())
}

/// Printed once, to the terminal only. Never logged through tracing, so the
/// password doesn't end up in log files or journald.
fn print_password(title: &str, password: &rd_auth::InitialPassword) {
    println!();
    println!("  {title}");
    println!("  username: {}", rd_auth::ADMIN_USER);
    println!("  password: {}", password.0);
    println!("  You'll be asked to change it at the next sign-in.");
    println!();
}

/// The DNS names and IP addresses a self-signed certificate is made for.
fn certificate_names(listen: &SocketAddr) -> Vec<String> {
    let mut names = vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "::1".to_owned(),
    ];
    if !listen.ip().is_unspecified() {
        names.push(listen.ip().to_string());
    }
    let host = rackdeskd::hostname();
    let valid_dns_name = !host.is_empty()
        && host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.');
    if valid_dns_name {
        names.push(host);
    }
    names.sort();
    names.dedup();
    names
}
