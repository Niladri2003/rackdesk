//! Starts `rackdesk-helper`: opens the files root, makes the socket, and
//! serves requests. See `lib.rs` for what the helper does and what limits it.

use anyhow::Context;
use clap::Parser;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// The socket to listen on. Production: /run/rackdesk/helper.sock.
    #[arg(long, env = "RACKDESK_HELPER_SOCKET")]
    socket: PathBuf,
    /// The folder that requests may act beneath. Give the smallest folder
    /// that the admin needs.
    #[arg(long, env = "RACKDESK_FILES_ROOT", default_value = "/")]
    files_root: PathBuf,
    /// The numeric id of the user that runs rackdeskd. Only this user is
    /// served. Find it with `id -u rackdesk`.
    #[arg(long, env = "RACKDESK_HELPER_ALLOW_UID")]
    allow_uid: u32,
    /// The Linux user that a terminal runs as. Without this option the
    /// helper starts no shell.
    #[arg(long, env = "RACKDESK_SHELL_USER")]
    shell_user: Option<String>,
    /// Allows --shell-user to be root. A root terminal can do anything on
    /// the machine.
    #[arg(long, env = "RACKDESK_ALLOW_ROOT_SHELL")]
    allow_root_shell: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if cfg!(not(target_os = "linux")) {
        anyhow::bail!(
            "rackdesk-helper runs on Linux only: it needs SO_PEERCRED to know who is calling"
        );
    }
    if rustix::process::geteuid().as_raw() != 0 {
        eprintln!("rackdesk-helper is not running as root. Actions that need root will fail.");
    }
    let root = rd_fs::Root::open(&args.files_root)
        .with_context(|| format!("opening files root {}", args.files_root.display()))?;

    // A socket file from an earlier run would make the bind fail.
    match std::fs::remove_file(&args.socket) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(e).with_context(|| format!("removing {}", args.socket.display()));
        }
        _ => {}
    }
    let listener = UnixListener::bind(&args.socket)
        .with_context(|| format!("binding {}", args.socket.display()))?;
    // Only the allowed user can open the socket at all. The check of the
    // caller's user id in `serve` is the second lock on the same door.
    std::fs::set_permissions(&args.socket, std::fs::Permissions::from_mode(0o600))
        .context("setting the socket permissions")?;
    if let Err(e) = std::os::unix::fs::chown(&args.socket, Some(args.allow_uid), None) {
        eprintln!("could not give the socket to user {}: {e}", args.allow_uid);
    }
    eprintln!(
        "rackdesk-helper listening on {} for user {}, files root {}",
        args.socket.display(),
        args.allow_uid,
        args.files_root.display()
    );
    rackdesk_helper::serve(
        listener,
        root,
        rackdesk_helper::Options {
            allow_uid: args.allow_uid,
            shell_user: args.shell_user,
            allow_root_shell: args.allow_root_shell,
        },
    );
    Ok(())
}
