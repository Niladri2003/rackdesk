//! The terminal: a shell on the server, shown in a window in the browser.
//!
//! `GET /api/v1/term` upgrades to a WebSocket. The server starts a shell on
//! a pseudo-terminal and passes bytes between the two. The rules come from
//! docs/security/SECURITY.md, R1 to R3:
//!
//! - The route needs a session, like every other route. A WebSocket is not
//!   covered by the CSRF check for POST, so the origin is checked here.
//! - Who starts the shell depends on the mode. `helper`: the root helper
//!   starts it, as the Linux user the helper was configured with. `direct`:
//!   the server starts it, as its own user. `off`: no terminal. `off` is
//!   the default.
//! - A shell lives while its window is open. If the connection is lost, the
//!   shell is kept for 60 s, so a network fault does not kill a running
//!   command. Sign-out, a password change and the end of the session stop
//!   it at once.
//! - The audit log records each start and each end. It never records what
//!   was typed or printed.

use crate::audit::Actor;
use crate::csrf::same_origin;
use crate::error::ApiErr;
use crate::state::AppState;
use axum::extract::rejection::QueryRejection;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use rd_helper_proto::{encode_stream_frame, StreamFrame};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

/// A shell with no connected window is stopped after this time.
const DETACHED_FOR: Duration = Duration::from_secs(60);
/// How often an open terminal checks that its session still exists.
const SESSION_CHECK: Duration = Duration::from_secs(30);
/// Shells at once, for the whole server.
const MAX_TERMINALS: usize = 16;
/// Output kept for a window that connects again.
const BACKLOG_BYTES: usize = 64 * 1024;
/// Largest message from the browser.
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Who starts the shell of a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TermMode {
    /// No terminal.
    Off,
    /// The server starts the shell, as the user that runs the server. For
    /// development, and for a machine with 1 user.
    Direct,
    /// The root helper starts the shell, as the Linux user it was
    /// configured with.
    Helper,
}

/// A message to the task that owns a shell.
enum Control {
    /// A window connected. Output goes to this channel from now on.
    Attach(mpsc::Sender<Out>),
    /// The window went away without closing the terminal.
    Detach,
    Input(Vec<u8>),
    Resize {
        cols: u16,
        rows: u16,
    },
    Stop,
}

/// A message to the browser.
enum Out {
    Data(Vec<u8>),
    /// The shell has ended.
    Exit,
}

struct Entry {
    /// The sign-in session that opened the terminal.
    session: String,
    control: mpsc::Sender<Control>,
}

/// The running shells, by terminal id.
#[derive(Default)]
pub struct Terminals {
    entries: Mutex<HashMap<String, Entry>>,
}

impl Terminals {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        // Each change is 1 insert or remove, so the map stays valid if a
        // thread panics while it holds the lock.
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Stops the shells of 1 sign-in session, or all shells for `None`.
    pub fn stop(&self, session: Option<&str>) {
        for entry in self.lock().values() {
            if session.is_none_or(|s| s == entry.session) {
                // A full queue means the task is busy. It will see that its
                // session is gone at its next check.
                let _ = entry.control.try_send(Control::Stop);
            }
        }
    }
}

/// The 2 ends of a shell, whoever started it.
struct Shell {
    output: Box<dyn AsyncRead + Unpin + Send>,
    input: ShellInput,
    /// Held so the process is stopped when the terminal ends.
    child: Option<tokio::process::Child>,
}

enum ShellInput {
    Direct(pty_process::OwnedWritePty),
    Helper(tokio::net::unix::OwnedWriteHalf),
}

impl ShellInput {
    async fn write(&mut self, bytes: Vec<u8>) -> std::io::Result<()> {
        match self {
            ShellInput::Direct(pty) => pty.write_all(&bytes).await,
            ShellInput::Helper(socket) => {
                socket
                    .write_all(&encode_stream_frame(&StreamFrame::Input(bytes)))
                    .await
            }
        }
    }

    async fn resize(&mut self, cols: u16, rows: u16) -> std::io::Result<()> {
        match self {
            ShellInput::Direct(pty) => pty
                .resize(pty_process::Size::new(rows, cols))
                .map_err(std::io::Error::other),
            ShellInput::Helper(socket) => {
                socket
                    .write_all(&encode_stream_frame(&StreamFrame::Resize { cols, rows }))
                    .await
            }
        }
    }
}

fn clamp_size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.clamp(1, 500), rows.clamp(1, 500))
}

/// Starts a shell as the user that runs the server.
fn start_direct(cols: u16, rows: u16, cwd: Option<PathBuf>) -> Result<Shell, ApiErr> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| std::path::Path::new(s).is_absolute())
        .unwrap_or_else(|| "/bin/sh".to_owned());
    let (pty, pts) = pty_process::open().map_err(ApiErr::internal)?;
    pty.resize(pty_process::Size::new(rows, cols))
        .map_err(ApiErr::internal)?;
    let mut command = pty_process::Command::new(&shell)
        // A login shell reads the user's profile, as a terminal over SSH does.
        .arg("-l")
        .env("TERM", "xterm-256color")
        .kill_on_drop(true);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(dir) = cwd.or(home).filter(|d| d.is_dir()) {
        command = command.current_dir(dir);
    }
    let child = command.spawn(pts).map_err(ApiErr::internal)?;
    let (output, input) = pty.into_split();
    Ok(Shell {
        output: Box::new(output),
        input: ShellInput::Direct(input),
        child: Some(child),
    })
}

#[derive(Debug, Deserialize)]
pub struct TermQuery {
    #[serde(default = "default_cols")]
    cols: u16,
    #[serde(default = "default_rows")]
    rows: u16,
    /// Folder to start in, as a path beneath the files root. Used in
    /// `direct` mode only.
    cwd: Option<String>,
    /// A terminal to connect to again, after a lost connection or a reload.
    id: Option<String>,
}

fn default_cols() -> u16 {
    80
}

fn default_rows() -> u16 {
    24
}

fn terminal_error(status: StatusCode, code: &'static str, message: &str) -> ApiErr {
    ApiErr::new(status, code, message)
}

/// `GET /api/v1/term`
pub async fn connect(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    query: Result<Query<TermQuery>, QueryRejection>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiErr> {
    let Query(q) = query?;
    let mode = state.config().terminal;
    if mode == TermMode::Off {
        return Err(terminal_error(
            StatusCode::NOT_FOUND,
            "terminal_off",
            "The terminal is turned off on this server.",
        ));
    }
    // A page on another site can open a WebSocket to this server, and the
    // browser would send the request. It must not get a shell.
    if !same_origin(&headers, None) {
        return Err(ApiErr::csrf_rejected());
    }
    let (cols, rows) = clamp_size(q.cols, q.rows);

    // Connect again to a shell of this session, if the window names one.
    let existing = q.id.as_deref().and_then(|id| {
        let entries = state.terminals().lock();
        let entry = entries.get(id)?;
        (entry.session == actor.session).then(|| (id.to_owned(), entry.control.clone()))
    });
    let (id, control) = match existing {
        Some(found) => found,
        None => {
            if state.terminals().len() >= MAX_TERMINALS {
                return Err(terminal_error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "too_many_terminals",
                    "Too many terminals are open. Close one first.",
                ));
            }
            let started = match mode {
                TermMode::Helper => match state.helper() {
                    Some(helper) => helper.shell(cols, rows).await.map(|socket| {
                        let (output, input) = socket.into_split();
                        Shell {
                            output: Box::new(output),
                            input: ShellInput::Helper(input),
                            child: None,
                        }
                    }),
                    None => Err(crate::helper::no_helper()),
                },
                _ => {
                    let cwd = match &q.cwd {
                        Some(path) => real_folder(&state, path).await,
                        None => None,
                    };
                    start_direct(cols, rows, cwd)
                }
            };
            let id = rd_auth::random_hex(16).map_err(ApiErr::internal)?;
            let target = format!(
                "{id} ({})",
                if mode == TermMode::Helper {
                    "helper"
                } else {
                    "direct"
                }
            );
            let result = started.as_ref().map(|_| ()).map_err(ApiErr::code);
            state
                .audit()
                .record(&actor, "terminal.open", &target, result);
            let shell = started?;

            let (control, receiver) = mpsc::channel(256);
            state.terminals().lock().insert(
                id.clone(),
                Entry {
                    session: actor.session.clone(),
                    control: control.clone(),
                },
            );
            tokio::spawn(run_shell(
                state.clone(),
                actor.clone(),
                id.clone(),
                target,
                shell,
                receiver,
            ));
            (id, control)
        }
    };

    Ok(upgrade
        .max_message_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| serve_window(socket, id, control, cols, rows)))
}

/// The folder on disk for a path beneath the files root, if it is a folder.
async fn real_folder(state: &AppState, ui_path: &str) -> Option<PathBuf> {
    let path = ui_path.to_owned();
    let is_dir = crate::files::blocking(state, move |root| root.entry(&path))
        .await
        .is_ok_and(|e| e.kind == rd_api::EntryKind::Dir);
    let rel = rd_fs::clean_path(ui_path).ok()?;
    is_dir.then(|| state.config().files_root.join(rel))
}

/// Owns 1 shell: reads its output, writes its input, and ends it by the
/// rules at the top of this file.
async fn run_shell(
    state: AppState,
    actor: Actor,
    id: String,
    target: String,
    mut shell: Shell,
    mut control: mpsc::Receiver<Control>,
) {
    let mut window: Option<mpsc::Sender<Out>> = None;
    let mut backlog: VecDeque<u8> = VecDeque::new();
    // Set while no window is connected.
    let mut detached_since = Some(Instant::now());
    let mut session_check = tokio::time::interval(SESSION_CHECK);
    let mut buf = vec![0u8; 8192];

    loop {
        let stop_at =
            detached_since.map(|since| tokio::time::Instant::from_std(since + DETACHED_FOR));
        tokio::select! {
            read = shell.output.read(&mut buf) => {
                let n = match read {
                    Ok(n) if n > 0 => n,
                    // The shell has ended.
                    _ => break,
                };
                backlog.extend(&buf[..n]);
                let extra = backlog.len().saturating_sub(BACKLOG_BYTES);
                backlog.drain(..extra);
                if let Some(to_window) = &window {
                    if to_window.send(Out::Data(buf[..n].to_vec())).await.is_err() {
                        window = None;
                        detached_since = Some(Instant::now());
                    }
                }
            }
            message = control.recv() => match message {
                Some(Control::Attach(to_window)) => {
                    // What the window missed, or all of it after a reload.
                    let seen: Vec<u8> = backlog.iter().copied().collect();
                    if seen.is_empty() || to_window.send(Out::Data(seen)).await.is_ok() {
                        window = Some(to_window);
                        detached_since = None;
                    }
                }
                Some(Control::Detach) => {
                    window = None;
                    detached_since = Some(Instant::now());
                }
                Some(Control::Input(bytes)) => {
                    // Typing counts as use of the session, like a request.
                    if state.sessions().touch(&actor.session, Instant::now()).is_none()
                        || shell.input.write(bytes).await.is_err()
                    {
                        break;
                    }
                }
                Some(Control::Resize { cols, rows }) => {
                    let _ = shell.input.resize(cols, rows).await;
                }
                Some(Control::Stop) | None => break,
            },
            _ = session_check.tick() => {
                if !state.sessions().is_live(&actor.session, Instant::now()) {
                    break;
                }
            }
            _ = async { tokio::time::sleep_until(stop_at.unwrap_or_else(tokio::time::Instant::now)).await }, if stop_at.is_some() => break,
        }
    }

    if let Some(mut child) = shell.child.take() {
        let _ = child.kill().await;
    }
    // Recorded before the terminal leaves the list, so "no terminal in the
    // list" always means "its end is in the log".
    state
        .audit()
        .record(&actor, "terminal.close", &target, Ok(()));
    state.terminals().lock().remove(&id);
    if let Some(to_window) = window {
        let _ = to_window.send(Out::Exit).await;
    }
}

/// Passes messages between 1 browser window and the task of its shell.
///
/// From the browser: a binary message starts with a type byte. 0: the rest
/// is input. 1: the next 4 bytes are the new size, columns then rows. The
/// text message "close" ends the terminal.
/// To the browser: binary messages with output, and 2 text messages, a
/// JSON `ready` with the terminal id and a JSON `exit`.
async fn serve_window(
    socket: WebSocket,
    id: String,
    control: mpsc::Sender<Control>,
    cols: u16,
    rows: u16,
) {
    let (mut to_browser, mut from_browser) = socket.split();
    let ready = serde_json::json!({"type": "ready", "id": id}).to_string();
    if to_browser.send(Message::Text(ready.into())).await.is_err() {
        return;
    }
    let (to_window, mut output) = mpsc::channel(64);
    if control.send(Control::Attach(to_window)).await.is_err() {
        return;
    }
    // The size of this window. It can differ from the one the shell had.
    let _ = control.send(Control::Resize { cols, rows }).await;

    let writer = tokio::spawn(async move {
        while let Some(out) = output.recv().await {
            let message = match out {
                Out::Data(bytes) => Message::Binary(bytes.into()),
                Out::Exit => {
                    let _ = to_browser
                        .send(Message::Text(r#"{"type":"exit"}"#.into()))
                        .await;
                    break;
                }
            };
            if to_browser.send(message).await.is_err() {
                break;
            }
        }
        let _ = to_browser.close().await;
    });

    let mut closed_by_user = false;
    while let Some(Ok(message)) = from_browser.next().await {
        let sent = match message {
            Message::Binary(bytes) => match bytes.split_first() {
                Some((0, input)) => control.send(Control::Input(input.to_vec())).await,
                Some((1, [c0, c1, r0, r1])) => {
                    let (cols, rows) = clamp_size(
                        u16::from_be_bytes([*c0, *c1]),
                        u16::from_be_bytes([*r0, *r1]),
                    );
                    control.send(Control::Resize { cols, rows }).await
                }
                _ => Ok(()),
            },
            Message::Text(text) if text.as_str() == "close" => {
                closed_by_user = true;
                control.send(Control::Stop).await
            }
            Message::Close(_) => break,
            _ => Ok(()),
        };
        if sent.is_err() {
            break;
        }
    }
    if !closed_by_user {
        let _ = control.send(Control::Detach).await;
    }
    writer.abort();
}
