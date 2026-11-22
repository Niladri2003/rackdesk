//! `rackdeskd`: one binary that serves the API and the web UI.
//!
//! This file handles the command line, the first start and the listener. The
//! routes are in `lib.rs`.

use anyhow::Context;
use clap::{Parser, Subcommand};
use rackdeskd::term::TermMode;
use rackdeskd::tls::{self, TlsMode};
use rackdeskd::{datadir, demo, AppState, Config};
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
    /// The folder the file browser shows as "/". Nothing above it can be read.
    #[arg(long, env = "RACKDESK_FILES_ROOT", default_value = "/")]
    files_root: PathBuf,
    /// Who starts the shell of the terminal app. "off": no terminal.
    /// "as-user": rackdesk-root starts it, as the user it was configured
    /// with. "direct": this program starts it, as its own user.
    #[arg(long, env = "RACKDESK_TERMINAL", value_enum, default_value = "off")]
    terminal: TermMode,
    /// The socket of rackdesk-root, for actions that need root. Without
    /// it, no action is done as root.
    #[arg(long, env = "RACKDESK_ROOT_SOCKET")]
    root_socket: Option<PathBuf>,
    /// Largest file that can be uploaded, in bytes. The default is 20 GiB.
    #[arg(long, env = "RACKDESK_MAX_UPLOAD_BYTES", default_value_t = 20 * 1024 * 1024 * 1024)]
    max_upload_bytes: u64,
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
    /// Try RackDesk with sample files. The file browser sees a temporary
    /// folder only, there is no terminal, and everything is deleted when the
    /// server stops.
    #[arg(long)]
    demo: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Replace the admin password with a new random one and print it.
    /// Run it on the server, as the user that runs rackdeskd.
    ResetPassword,
    /// Create the data folder and the admin account, and print the password.
    /// It does nothing if the account exists. An installer runs it, so the
    /// password is shown to the person who installs and not written to the
    /// log of the service.
    Init,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rackdeskd=info".into()),
        )
        .init();
    let mut args = Args::parse();

    let demo = if args.demo {
        if args.command.is_some() {
            anyhow::bail!("--demo starts the server. It can't be used with a command");
        }
        demo::check(&demo::Request {
            listen: &args.listen,
            terminal: args.terminal,
            root_socket: args.root_socket.is_some(),
        })
        .map_err(anyhow::Error::msg)?;
        let demo = demo::Demo::create().context("creating the demo folders")?;
        args.data_dir = demo.data_dir();
        args.files_root = demo.files_root();
        args.max_upload_bytes = args.max_upload_bytes.min(demo::MAX_UPLOAD_BYTES);
        Some(demo)
    } else {
        None
    };

    datadir::refuse_root_on_a_folder_of_another_user(&args.data_dir)?;
    datadir::prepare(&args.data_dir)?;
    let store = rd_auth::CredentialStore::new(&args.data_dir);

    match args.command {
        Some(Command::ResetPassword) => {
            let (_, password) = store.reset().context("resetting the admin password")?;
            print_password("RackDesk admin password reset", &password);
            println!("  A running server signs out every session at its next request.");
            println!();
            return Ok(());
        }
        Some(Command::Init) => {
            let (_, initial) = store
                .load_or_init()
                .context("loading or creating the admin account")?;
            match initial {
                Some(password) => print_password("RackDesk admin account created", &password),
                None => {
                    println!("The admin account exists already. Nothing was changed.");
                    println!("For a new password, run: rackdeskd reset-password");
                }
            }
            return Ok(());
        }
        None => {}
    }

    let (creds, initial) = store
        .load_or_init()
        .context("loading or creating the admin account")?;
    if let Some(password) = initial {
        if output_goes_to_the_journal() {
            // Everything this process prints is kept in the system journal,
            // where other users of the machine may read it.
            println!(
                "RackDesk admin account created. The password is not printed here, because this output is kept in the system journal. Get a password with: rackdeskd --data-dir {} reset-password",
                args.data_dir.display()
            );
        } else {
            print_password("RackDesk admin account created", &password);
        }
    }
    if demo.is_some() {
        println!("  Demo mode. The file browser shows sample files only.");
        println!("  Everything is deleted when you stop the server with Ctrl C.");
        println!();
    }

    // An absolute path with links resolved, so the home folder and the mount
    // points can be compared with it.
    let files_root = std::fs::canonicalize(&args.files_root)
        .with_context(|| format!("opening files root {}", args.files_root.display()))?;
    let mut files = rd_fs::Root::open(&files_root)
        .with_context(|| format!("opening files root {}", files_root.display()))?;
    // With a home folder beneath the files root, the trash is the one that
    // desktop programs use too. Without, it is a folder at the top of the
    // files root.
    if let Some(home) = rackdeskd::files::home_folder(&files_root, &files) {
        files = files
            .with_trash_dir(&format!("{home}/.local/share/Trash"))
            .context("setting the trash folder")?;
    }

    #[cfg(unix)]
    if rustix::process::geteuid().as_raw() == 0 {
        tracing::warn!(
            "rackdeskd is running as root. Run it as a user with no special rights, and use rackdesk-root for actions that need root"
        );
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
            files_root,
            max_upload_bytes: args.max_upload_bytes,
            root_socket: args.root_socket.clone(),
            terminal: args.terminal,
            https: use_tls || args.behind_proxy,
            behind_proxy: args.behind_proxy,
        },
        creds,
        files,
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
            stop_signal().await;
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
            .with_graceful_shutdown(stop_signal())
            .await?;
    }
    // The demo folders are deleted here, after the last request.
    drop(demo);
    Ok(())
}

/// Resolves when the server is asked to stop: Ctrl C in a terminal, or the
/// stop signal that a service manager sends.
async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut terminate) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

/// True when systemd started this process with its output connected to the
/// journal. systemd sets `JOURNAL_STREAM` in that case.
fn output_goes_to_the_journal() -> bool {
    std::env::var_os("JOURNAL_STREAM").is_some()
}

/// Printed once, to the terminal only. Never logged through tracing, and not
/// printed at all when the output is kept in the journal, so the password
/// doesn't end up in log files.
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
