//! `rackdesk-helper`: the part of RackDesk that runs as root.
//!
//! The server (`rackdeskd`) runs as a user with no special rights. When the
//! admin asks for an action that needs root, the server sends a request to
//! this program over a local socket. The full list of actions is
//! `rd_helper_proto::Request`: describe an entry, set permission bits, set
//! the owner, and read a part of a file.
//!
//! What limits a request (docs/security/SECURITY.md, P1):
//!
//! - Only the configured user may connect. The kernel tells the helper who
//!   is on the other end of the socket (`SO_PEERCRED`); the client can't
//!   choose that value.
//! - Every path is resolved by `rd_fs::Root`, beneath the files root that
//!   the helper was started with. `..` and symlinks can't leave it.
//! - Only the 9 plain permission bits can be set. No set-uid or set-gid bit.
//! - Only regular files are read, at most 1 MiB per request.
//! - A shell is started only as the Linux user that the helper was started
//!   with (`--shell-user`). A request can't name a user. Without that
//!   option the helper starts no shell at all, and it starts a root shell
//!   only with `--allow-root-shell`.
//! - Every action is written to the log with the id of the calling process.
//!
//! What it does not limit: beneath its files root, the helper does what it
//! is asked. A server that an attacker controls can use it to read any file
//! there or to change its owner. Start the helper with the smallest files
//! root that the admin needs.

mod shell;

use rd_fs::{FsError, Root};
use rd_helper_proto::{
    read_request, write_response, ProtoError, Request, Response, MAX_READ_BYTES,
};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Connections served at once. More are closed at once.
const MAX_CONNECTIONS: usize = 8;
/// A connection that sends nothing for this long is closed.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

fn error(e: &FsError) -> Response {
    Response::Error {
        code: e.code().to_owned(),
        // The text of an I/O error can name a path on the server. The code
        // says enough.
        message: match e {
            FsError::Io(_) => "the action failed".to_owned(),
            other => other.to_string(),
        },
    }
}

fn bad_request(message: &str) -> Response {
    Response::Error {
        code: "bad_request".to_owned(),
        message: message.to_owned(),
    }
}

/// How the helper was started.
#[derive(Debug, Clone)]
pub struct Options {
    /// Only this user id is served.
    pub allow_uid: u32,
    /// The Linux user that a terminal runs as. `None`: no terminal.
    pub shell_user: Option<String>,
    /// Allows `shell_user` to be root.
    pub allow_root_shell: bool,
}

impl Options {
    /// Serves `allow_uid` and starts no shell.
    pub fn files_only(allow_uid: u32) -> Self {
        Self {
            allow_uid,
            shell_user: None,
            allow_root_shell: false,
        }
    }
}

/// Does 1 request. Returns the response and, for `Read`, the bytes.
pub fn handle(root: &Root, request: &Request) -> (Response, Vec<u8>) {
    let done = |result: Result<(), FsError>| match result {
        Ok(()) => Response::Ok,
        Err(e) => error(&e),
    };
    let response = match request {
        Request::Ping => Response::Ok,
        Request::Stat { path } => match root.entry(path).and_then(|e| Ok((e, root.owner(path)?))) {
            Ok((entry, (uid, gid))) => Response::Stat {
                is_file: entry.kind == rd_fs::EntryKind::File,
                is_dir: entry.kind == rd_fs::EntryKind::Dir,
                size: entry.size,
                uid,
                gid,
                mode: entry.mode.unwrap_or(0),
            },
            Err(e) => error(&e),
        },
        Request::Chmod { path, mode } => done(root.set_mode(path, *mode)),
        Request::Chown { path, uid, gid } => {
            if uid.is_none() && gid.is_none() {
                bad_request("give an owner, a group, or both")
            } else {
                done(root.set_owner(path, *uid, *gid))
            }
        }
        // A shell takes over the connection. `serve_connection` starts it.
        Request::Shell { .. } => bad_request("a shell is not a single request"),
        Request::Read { path, offset, len } => {
            if *len > MAX_READ_BYTES {
                bad_request("the part is larger than 1 MiB")
            } else {
                return match read_part(root, path, *offset, *len) {
                    Ok(data) => (
                        Response::Data {
                            len: data.len() as u32,
                        },
                        data,
                    ),
                    Err(e) => (error(&e), Vec::new()),
                };
            }
        }
    };
    (response, Vec::new())
}

fn read_part(root: &Root, path: &str, offset: u64, len: u32) -> Result<Vec<u8>, FsError> {
    // `open_file` refuses folders, devices, pipes and sockets.
    let mut file = root.open_file(path)?.file;
    file.seek(SeekFrom::Start(offset))?;
    let mut data = Vec::new();
    file.take(u64::from(len)).read_to_end(&mut data)?;
    Ok(data)
}

/// The user id of the process on the other end of the socket. The kernel
/// fills this in. The client has no say in it.
#[cfg(target_os = "linux")]
pub fn peer_ids(stream: &UnixStream) -> std::io::Result<(u32, i32)> {
    let cred = rustix::net::sockopt::socket_peercred(stream)?;
    Ok((cred.uid.as_raw(), cred.pid.as_raw_nonzero().get()))
}

/// The helper is for Linux. Other systems have no `SO_PEERCRED`, and without
/// it the helper can't know who is calling.
#[cfg(not(target_os = "linux"))]
pub fn peer_ids(_stream: &UnixStream) -> std::io::Result<(u32, i32)> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "rackdesk-helper runs on Linux only",
    ))
}

/// Writes 1 line about an action to stderr, which systemd puts in the
/// journal. A line never holds file content.
fn log(peer_pid: i32, request: &Request, response: &Response) {
    let result = match response {
        Response::Error { code, .. } => code.as_str(),
        _ => "ok",
    };
    let line = serde_json::json!({
        "helper": request.name(),
        "path": request.path(),
        "pid": peer_pid,
        "result": result,
    });
    eprintln!("{line}");
}

fn serve_connection(mut stream: UnixStream, root: &Root, options: &Options) {
    let _ = stream.set_read_timeout(Some(IDLE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IDLE_TIMEOUT));
    let peer_pid = match peer_ids(&stream) {
        Ok((uid, pid)) if uid == options.allow_uid => pid,
        Ok((uid, pid)) => {
            eprintln!(
                "{}",
                serde_json::json!({"helper": "refused", "uid": uid, "pid": pid})
            );
            return;
        }
        Err(e) => {
            eprintln!(
                "{}",
                serde_json::json!({"helper": "refused", "error": e.to_string()})
            );
            return;
        }
    };
    loop {
        let request = match read_request(&mut stream) {
            Ok(request) => request,
            // The client closed the connection, or went quiet.
            Err(ProtoError::Io(_)) => return,
            Err(e) => {
                // A message that is not valid ends the connection. The
                // client is the server, and it does not send such messages.
                let _ = write_response(&mut stream, &bad_request(&e.to_string()), &[]);
                return;
            }
        };
        if let Request::Shell { cols, rows } = request {
            // The connection is the terminal from here on. It ends when the
            // shell ends or the server closes it.
            let outcome = shell::run(stream, options, cols, rows);
            let result = outcome.as_ref().err().map_or("ok", |e| e.as_str());
            eprintln!(
                "{}",
                serde_json::json!({"helper": "shell", "user": options.shell_user, "pid": peer_pid, "result": result})
            );
            return;
        }
        let (response, data) = handle(root, &request);
        if !matches!(request, Request::Ping) {
            log(peer_pid, &request, &response);
        }
        if write_response(&mut stream, &response, &data).is_err() {
            return;
        }
    }
}

/// Accepts connections for as long as the process runs. Each connection
/// gets a thread.
pub fn serve(listener: UnixListener, root: Root, options: Options) {
    let root = Arc::new(root);
    let options = Arc::new(options);
    let open = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if open.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            open.fetch_sub(1, Ordering::SeqCst);
            continue;
        }
        let root = Arc::clone(&root);
        let open = Arc::clone(&open);
        let options = Arc::clone(&options);
        std::thread::spawn(move || {
            serve_connection(stream, &root, &options);
            open.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn fixture() -> (tempfile::TempDir, Root) {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("app.conf"), "0123456789").unwrap();
        fs::write(base.path().join("outside.txt"), "outside").unwrap();
        fs::set_permissions(
            base.path().join("outside.txt"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        std::os::unix::fs::symlink(base.path(), root.join("escape")).unwrap();
        std::os::unix::fs::symlink(base.path().join("outside.txt"), root.join("link")).unwrap();
        (base, Root::open(&root).unwrap())
    }

    fn code(response: &Response) -> &str {
        match response {
            Response::Error { code, .. } => code,
            _ => "ok",
        }
    }

    #[test]
    fn every_action_stays_beneath_the_files_root() {
        let (base, root) = fixture();
        let outside = base.path().join("outside.txt");
        for path in [
            "/../outside.txt",
            "/escape/outside.txt",
            "/link",
            "/escape",
            "/..",
        ] {
            let requests = [
                Request::Stat { path: path.into() },
                Request::Chmod {
                    path: path.into(),
                    mode: 0o777,
                },
                Request::Chown {
                    path: path.into(),
                    uid: Some(0),
                    gid: Some(0),
                },
                Request::Read {
                    path: path.into(),
                    offset: 0,
                    len: 100,
                },
            ];
            for request in requests {
                let (response, data) = handle(&root, &request);
                // `Stat` and `Chown` may act on a link itself. They must
                // never reach what it points to.
                let on_link_itself = matches!(path, "/link" | "/escape")
                    && matches!(request, Request::Stat { .. } | Request::Chown { .. });
                if !on_link_itself {
                    assert_ne!(code(&response), "ok", "{request:?}");
                }
                assert!(data.is_empty(), "{request:?} returned file content");
            }
        }
        assert_eq!(
            fs::metadata(&outside).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_to_string(&outside).unwrap(), "outside");
    }

    #[test]
    fn chmod_sets_plain_bits_only() {
        let (base, root) = fixture();
        let file = base.path().join("root/app.conf");
        let (ok, _) = handle(
            &root,
            &Request::Chmod {
                path: "/app.conf".into(),
                mode: 0o640,
            },
        );
        assert_eq!(ok, Response::Ok);
        for mode in [0o4755, 0o2755, 0o1644] {
            let (refused, _) = handle(
                &root,
                &Request::Chmod {
                    path: "/app.conf".into(),
                    mode,
                },
            );
            assert_eq!(code(&refused), "invalid_path", "{mode:o}");
        }
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o7777,
            0o640
        );
    }

    #[test]
    fn read_returns_the_part_that_was_asked_for() {
        let (_base, root) = fixture();
        let (response, data) = handle(
            &root,
            &Request::Read {
                path: "/app.conf".into(),
                offset: 3,
                len: 4,
            },
        );
        assert_eq!(
            (response, data.as_slice()),
            (Response::Data { len: 4 }, b"3456".as_slice())
        );
        // Past the end: fewer bytes, then none.
        let (_, tail) = handle(
            &root,
            &Request::Read {
                path: "/app.conf".into(),
                offset: 8,
                len: 100,
            },
        );
        assert_eq!(tail, b"89");
        let (_, none) = handle(
            &root,
            &Request::Read {
                path: "/app.conf".into(),
                offset: 50,
                len: 100,
            },
        );
        assert!(none.is_empty());

        let (too_much, _) = handle(
            &root,
            &Request::Read {
                path: "/app.conf".into(),
                offset: 0,
                len: MAX_READ_BYTES + 1,
            },
        );
        assert_eq!(code(&too_much), "bad_request");
        let (folder, _) = handle(
            &root,
            &Request::Read {
                path: "/".into(),
                offset: 0,
                len: 10,
            },
        );
        assert_ne!(code(&folder), "ok");
    }

    #[test]
    fn chown_needs_an_owner_or_a_group() {
        let (_base, root) = fixture();
        let (response, _) = handle(
            &root,
            &Request::Chown {
                path: "/app.conf".into(),
                uid: None,
                gid: None,
            },
        );
        assert_eq!(code(&response), "bad_request");
    }

    /// Needs Linux, for `SO_PEERCRED`. It connects to a real socket and
    /// checks that the user id from the kernel decides who is served.
    #[cfg(target_os = "linux")]
    #[test]
    fn only_the_allowed_user_is_served() {
        use rd_helper_proto::{read_response, write_request};
        let (base, root) = fixture();
        let me = rustix::process::getuid().as_raw();
        let ask = |allow_uid: u32, name: &str| {
            let socket = base.path().join(name);
            let listener = UnixListener::bind(&socket).unwrap();
            let root = Root::open(&base.path().join("root")).unwrap();
            std::thread::spawn(move || serve(listener, root, Options::files_only(allow_uid)));
            let mut stream = UnixStream::connect(&socket).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let _ = write_request(
                &mut stream,
                &Request::Stat {
                    path: "/app.conf".into(),
                },
            );
            read_response(&mut stream)
        };
        drop(root);
        // This process is allowed: it gets an answer.
        let (response, _) = ask(me, "allowed.sock").unwrap();
        assert!(
            matches!(response, Response::Stat { size: 10, .. }),
            "{response:?}"
        );
        // Another user id is configured: the connection is closed with no
        // answer at all.
        assert!(ask(me + 1, "refused.sock").is_err());
    }
}
