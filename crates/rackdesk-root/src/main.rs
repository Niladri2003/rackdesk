//! Starts `rackdesk-root`: opens the files root, makes the socket, and
//! serves requests. See `lib.rs` for what the root program does and what limits it.

use anyhow::Context;
use clap::Parser;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// The socket to listen on. Production: /run/rackdesk/root.sock.
    #[arg(long, env = "RACKDESK_ROOT_SOCKET")]
    socket: PathBuf,
    /// The folder that requests may act beneath. Give the smallest folder
    /// that the admin needs.
    #[arg(long, env = "RACKDESK_FILES_ROOT", default_value = "/")]
    files_root: PathBuf,
    /// The numeric id of the user that runs rackdeskd. Only this user is
    /// served. Find it with `id -u rackdesk`.
    #[arg(long, env = "RACKDESK_ROOT_ALLOW_UID")]
    allow_uid: u32,
    /// The Linux user that a terminal runs as. Without this option the
    /// program starts no shell.
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
            "rackdesk-root runs on Linux only: it needs SO_PEERCRED to know who is calling"
        );
    }
    if rustix::process::geteuid().as_raw() != 0 {
        eprintln!("rackdesk-root is not running as root. Actions that need root will fail.");
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
    // Only the allowed user can open the socket at all. The check of the
    // caller's user id in `serve` is the second lock on the same door.
    //
    // The file is made with mode 0600 from the start, by the mask, and its
    // owner is set without following a link. A call by path that follows
    // links could be sent to another file by a user who can write in the
    // folder of the socket.
    let old_mask = rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o177));
    let bound = UnixListener::bind(&args.socket);
    rustix::process::umask(old_mask);
    let listener = bound.with_context(|| format!("binding {}", args.socket.display()))?;
    std::os::unix::fs::lchown(&args.socket, Some(args.allow_uid), None).with_context(|| {
        format!(
            "giving the socket {} to user {}",
            args.socket.display(),
            args.allow_uid
        )
    })?;
    eprintln!(
        "rackdesk-root listening on {} for user {}, files root {}",
        args.socket.display(),
        args.allow_uid,
        args.files_root.display()
    );
    rackdesk_root::serve(
        listener,
        root,
        rackdesk_root::Options {
            allow_uid: args.allow_uid,
            shell_user: args.shell_user,
            allow_root_shell: args.allow_root_shell,
        },
    );
    Ok(())
}
