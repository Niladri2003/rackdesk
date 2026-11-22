//! The server together with a real root program on a real socket. The root program
//! needs Linux, so these tests are compiled on Linux only. They run in CI
//! and with `scripts/test-linux.sh`.
#![cfg(target_os = "linux")]

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;

/// A server whose root program serves the same files root, in a thread of this
/// process. The root program accepts the user that runs the test.
async fn setup() -> (Server, String) {
    let socket_dir = tempfile::tempdir().unwrap().keep();
    let socket = socket_dir.join("root.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = server_from(|config| config.root_socket = Some(socket.clone())).await;
    let root = rd_fs::Root::open(&server.files_root()).unwrap();
    let me = rustix::process::getuid().as_raw();
    std::thread::spawn(move || {
        rackdesk_root::serve(listener, root, rackdesk_root::Options::files_only(me))
    });

    fs::write(server.files_root().join("secret.conf"), "token = 1").unwrap();
    fs::write(server.dir.path().join("outside.txt"), "outside").unwrap();
    let cookie = active_session(&server).await;
    (server, cookie)
}

#[tokio::test]
async fn an_elevated_change_goes_through_the_root_program_and_is_recorded() {
    let (server, cookie) = setup().await;
    let file = server.files_root().join("secret.conf");
    let res = call(Method::POST, "/api/v1/fs/chmod")
        .cookie(&cookie)
        .body(json!({"path": "/secret.conf", "mode": 0o600, "elevate": true}))
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o7777,
        0o600
    );

    // The root program keeps the same limits as the server.
    for body in [
        json!({"path": "/secret.conf", "mode": 0o4755, "elevate": true}),
        json!({"path": "/../outside.txt", "mode": 0o777, "elevate": true}),
    ] {
        let res = call(Method::POST, "/api/v1/fs/chmod")
            .cookie(&cookie)
            .body(body.clone())
            .send(&server)
            .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{body}");
    }
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o7777,
        0o600
    );

    let log = fs::read_to_string(server.dir.path().join("audit.log")).unwrap();
    let lines: Vec<Value> = log
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(lines
        .iter()
        .any(|l| l["action"] == "root.chmod" && l["result"] == "ok"));
    assert!(lines
        .iter()
        .any(|l| l["action"] == "root.chmod" && l["result"] == "invalid_path"));
}

#[tokio::test]
async fn an_elevated_download_sends_the_whole_file() {
    let (server, cookie) = setup().await;
    // Larger than 1 part of the root program protocol, so it takes several.
    let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    fs::write(server.files_root().join("big.bin"), &big).unwrap();

    let res = call(
        Method::GET,
        "/api/v1/fs/download?path=/big.bin&elevate=true",
    )
    .cookie(&cookie)
    .send(&server)
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .starts_with("attachment"));
    assert_eq!(bytes_body(res).await, big);

    let res = call(
        Method::GET,
        "/api/v1/fs/download?path=/../outside.txt&elevate=true",
    )
    .cookie(&cookie)
    .send(&server)
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let res = call(Method::GET, "/api/v1/fs/download?path=/&elevate=true")
        .cookie(&cookie)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

/// A terminal whose shell is started by the root program. It needs root, because
/// the root program switches to the shell user with no password.
#[tokio::test]
async fn a_terminal_runs_through_the_root_program() {
    use futures_util::SinkExt;
    use rackdeskd::term::TermMode;
    if rustix::process::getuid().as_raw() != 0 {
        eprintln!("skipped: this test needs root");
        return;
    }
    let socket_dir = tempfile::tempdir().unwrap().keep();
    let socket = socket_dir.join("root.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = server_from(|config| {
        config.root_socket = Some(socket.clone());
        config.terminal = TermMode::AsUser;
    })
    .await;
    let root = rd_fs::Root::open(&server.files_root()).unwrap();
    std::thread::spawn(move || {
        let options = rackdesk_root::Options {
            allow_uid: 0,
            shell_user: Some("root".into()),
            allow_root_shell: true,
        };
        rackdesk_root::serve(listener, root, options)
    });
    let addr = listen(&server).await;
    let cookie = active_session(&server).await;
    let origin = format!("http://{addr}");

    let mut window = open(addr, Some(&cookie), Some(&origin), "cols=90&rows=20")
        .await
        .unwrap();
    ready(&mut window).await;
    type_line(&mut window, "echo via-helper-$((40+2)); stty size").await;
    let seen = read_until(&mut window, "via-helper-42").await;
    // The window size reached the shell through the root program.
    let seen = if seen.contains("20 90") {
        seen
    } else {
        read_until(&mut window, "20 90").await
    };
    assert!(seen.contains("20 90"), "{seen}");

    window
        .send(tokio_tungstenite::tungstenite::Message::Text(
            "close".into(),
        ))
        .await
        .unwrap();
    for _ in 0..100 {
        if server.state.terminals().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(server.state.terminals().is_empty());
    let log = fs::read_to_string(server.dir.path().join("audit.log")).unwrap();
    assert!(log.contains("terminal.open") && log.contains("(as-user)"));
}
