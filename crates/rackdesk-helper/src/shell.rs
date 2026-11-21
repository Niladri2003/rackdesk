//! Starts a login shell on a pseudo-terminal, as the configured Linux user,
//! and passes bytes between it and the server.
//!
//! The helper does not switch users itself. It runs `runuser -l <user>`
//! (or `su -l <user>` where `runuser` is missing). That program is part of
//! every Linux system, and it sets the user, all of the user's groups and
//! the login environment in the way the system expects.

use crate::Options;
use pty_process::blocking::{Command, Pty};
use pty_process::Size;
use rd_helper_proto::{read_stream_frame, write_response, Response, StreamFrame};
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;

/// The programs that start a shell as another user, in order of choice.
const SWITCH_PROGRAMS: [&str; 4] = [
    "/usr/sbin/runuser",
    "/sbin/runuser",
    "/usr/bin/su",
    "/bin/su",
];

/// A user name as Linux tools accept it: it starts with a letter or "_", and
/// holds letters, digits, "_", "-" and ".". This also keeps the name from
/// being read as an option of `runuser`.
pub(crate) fn valid_user_name(name: &str) -> bool {
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    first_ok
        && name.len() <= 32
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// The user id of a name, from `/etc/passwd`.
fn user_id(passwd: &str, name: &str) -> Option<u32> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next()? == name).then(|| fields.nth(1)?.parse().ok())?
    })
}

/// Decides if a shell may start, and as whom. `Err` holds an error code.
pub(crate) fn shell_user<'a>(options: &'a Options, passwd: &str) -> Result<&'a str, &'static str> {
    let user = options.shell_user.as_deref().ok_or("terminal_off")?;
    if !valid_user_name(user) {
        return Err("bad_request");
    }
    match user_id(passwd, user) {
        None => Err("not_found"),
        Some(0) if !options.allow_root_shell => Err("permission_denied"),
        Some(_) => Ok(user),
    }
}

fn refuse(stream: &mut UnixStream, code: &str) -> Result<(), String> {
    let message = match code {
        "terminal_off" => "this helper was started without a terminal user",
        "permission_denied" => "a root terminal is not allowed on this server",
        "not_found" => "the terminal user does not exist",
        _ => "the terminal could not start",
    };
    let response = Response::Error {
        code: code.to_owned(),
        message: message.to_owned(),
    };
    let _ = write_response(stream, &response, &[]);
    Err(code.to_owned())
}

/// Runs a shell until it ends or the server closes the connection.
pub(crate) fn run(
    mut stream: UnixStream,
    options: &Options,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let user = match shell_user(options, &passwd) {
        Ok(user) => user,
        Err(code) => return refuse(&mut stream, code),
    };
    let Some(program) = SWITCH_PROGRAMS.iter().find(|p| Path::new(p).exists()) else {
        return refuse(&mut stream, "internal");
    };

    let started = (|| -> pty_process::Result<_> {
        let (pty, pts) = pty_process::blocking::open()?;
        pty.resize(Size::new(rows.clamp(1, 500), cols.clamp(1, 500)))?;
        // "-l" gives a login shell: the user's own shell, home folder and
        // environment. "--" ends the options, so the name is never one.
        let child = Command::new(program)
            .args(["-l", "--", user])
            .env("TERM", "xterm-256color")
            .spawn(pts)?;
        Ok((pty, child))
    })();
    let (pty, mut child) = match started {
        Ok(started) => started,
        Err(_) => return refuse(&mut stream, "internal"),
    };
    write_response(&mut stream, &Response::Ok, &[]).map_err(|e| e.to_string())?;
    // A shell can be idle for hours. The timeouts of a request don't apply.
    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(None);

    let outcome = std::thread::scope(|scope| {
        // Shell output to the server. Ends when the shell ends.
        let mut to_server = stream.try_clone().map_err(|e| e.to_string())?;
        let output = &pty;
        scope.spawn(move || {
            let mut reader: &Pty = output;
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || to_server.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            // Wakes the loop below, which waits for a frame from the server.
            let _ = to_server.shutdown(Shutdown::Both);
        });

        // Input and window sizes from the server. Ends when the server
        // closes the connection, or sends a frame that is not valid.
        let mut writer: &Pty = &pty;
        while let Ok(frame) = read_stream_frame(&mut stream) {
            let ok = match frame {
                StreamFrame::Input(bytes) => writer.write_all(&bytes).is_ok(),
                StreamFrame::Resize { cols, rows } => pty
                    .resize(Size::new(rows.clamp(1, 500), cols.clamp(1, 500)))
                    .is_ok(),
            };
            if !ok {
                break;
            }
        }
        // The server is gone or the shell has ended: stop the shell. This
        // also ends the read in the thread above.
        let _ = child.kill();
        let _ = child.wait();
        Ok(())
    });
    drop(pty);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\ndev:x:1000:1000::/home/dev:/bin/bash\n";

    fn options(user: Option<&str>, allow_root: bool) -> Options {
        Options {
            allow_uid: 1,
            shell_user: user.map(str::to_owned),
            allow_root_shell: allow_root,
        }
    }

    #[test]
    fn a_shell_starts_only_as_the_configured_user() {
        assert_eq!(
            shell_user(&options(None, false), PASSWD),
            Err("terminal_off")
        );
        assert_eq!(shell_user(&options(Some("dev"), false), PASSWD), Ok("dev"));
        assert_eq!(
            shell_user(&options(Some("ghost"), false), PASSWD),
            Err("not_found")
        );
        // Root needs the second, explicit option.
        assert_eq!(
            shell_user(&options(Some("root"), false), PASSWD),
            Err("permission_denied")
        );
        assert_eq!(shell_user(&options(Some("root"), true), PASSWD), Ok("root"));
    }

    #[test]
    fn user_names_that_could_be_options_or_paths_are_refused() {
        for name in ["dev", "www-data", "_apt", "a.b"] {
            assert!(valid_user_name(name), "{name}");
        }
        for name in [
            "", "-l", "--login", "root;id", "a b", "../x", "0day", "dev\n", "$(id)",
        ] {
            assert!(!valid_user_name(name), "{name:?}");
            assert_eq!(
                shell_user(&options(Some(name), true), PASSWD),
                Err("bad_request"),
                "{name:?}"
            );
        }
    }

    /// A real shell through a real socket. It needs root (to switch users
    /// with no password) and Linux. `scripts/test-linux.sh` runs it.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_shell_runs_a_command_and_ends_when_the_connection_closes() {
        use rd_helper_proto::{read_response, write_request, write_stream_frame, Request};
        use std::os::unix::net::UnixListener;
        use std::time::{Duration, Instant};
        if rustix::process::getuid().as_raw() != 0 {
            eprintln!("skipped: this test needs root");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("helper.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let root = rd_fs::Root::open(dir.path()).unwrap();
        std::thread::spawn(move || crate::serve(listener, root, options_for_root()));

        let mut stream = UnixStream::connect(&socket).unwrap();
        write_request(&mut stream, &Request::Shell { cols: 80, rows: 24 }).unwrap();
        let (response, _) = read_response(&mut stream).unwrap();
        assert_eq!(response, Response::Ok);

        write_stream_frame(
            &mut stream,
            &StreamFrame::Resize {
                cols: 100,
                rows: 30,
            },
        )
        .unwrap();
        write_stream_frame(
            &mut stream,
            &StreamFrame::Input(b"echo helper-$((40+2)); id -un\n".to_vec()),
        )
        .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut seen = String::new();
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut buf = [0u8; 4096];
        while !(seen.contains("helper-42") && seen.contains("\nroot")) && Instant::now() < deadline
        {
            if let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                seen.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        }
        assert!(seen.contains("helper-42"), "output was: {seen}");
        assert!(
            seen.replace('\r', "").contains("\nroot"),
            "output was: {seen}"
        );
    }

    #[cfg(target_os = "linux")]
    fn options_for_root() -> Options {
        Options {
            allow_uid: 0,
            shell_user: Some("root".into()),
            allow_root_shell: true,
        }
    }
}
