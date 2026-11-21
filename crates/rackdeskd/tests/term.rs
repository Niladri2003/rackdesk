//! The terminal over a real WebSocket: who may open one, and when its shell
//! is stopped. These tests need a listening socket, because a WebSocket
//! can't be opened against the router in-process.

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use futures_util::SinkExt;
use rackdeskd::term::TermMode;
use std::net::SocketAddr;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

async fn terminal_server() -> (Server, SocketAddr, String) {
    let server = server_from(|config| config.terminal = TermMode::Direct).await;
    let addr = listen(&server).await;
    let cookie = active_session(&server).await;
    (server, addr, cookie)
}

async fn wait_for_no_terminals(server: &Server) {
    for _ in 0..100 {
        if server.state.terminals().is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("a shell is still running");
}

#[tokio::test]
async fn the_terminal_is_off_by_default() {
    let server = server().await;
    let addr = listen(&server).await;
    let cookie = active_session(&server).await;
    let origin = format!("http://{addr}");
    assert_eq!(
        status_of(open(addr, Some(&cookie), Some(&origin), "").await),
        StatusCode::NOT_FOUND
    );
    assert!(server.state.terminals().is_empty());
}

#[tokio::test]
async fn a_terminal_needs_a_session_and_the_right_origin() {
    let (server, addr, cookie) = terminal_server().await;
    let origin = format!("http://{addr}");

    assert_eq!(
        status_of(open(addr, None, Some(&origin), "").await),
        StatusCode::UNAUTHORIZED
    );
    let made_up = format!("rd_session={}", "a".repeat(64));
    assert_eq!(
        status_of(open(addr, Some(&made_up), Some(&origin), "").await),
        StatusCode::UNAUTHORIZED
    );

    // A page on another site, and a client that says nothing about itself.
    assert_eq!(
        status_of(open(addr, Some(&cookie), Some("https://evil.example"), "").await),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status_of(open(addr, Some(&cookie), None, "").await),
        StatusCode::FORBIDDEN
    );

    // No refused request started a shell.
    assert!(server.state.terminals().is_empty());
}

#[tokio::test]
async fn a_shell_runs_commands_and_stops_at_sign_out() {
    let (server, addr, cookie) = terminal_server().await;
    let origin = format!("http://{addr}");
    let mut socket = open(addr, Some(&cookie), Some(&origin), "cols=100&rows=30")
        .await
        .unwrap();
    ready(&mut socket).await;

    // The sum shows that the shell ran the line, not only echoed it.
    type_line(&mut socket, "echo term-$((40+2))").await;
    read_until(&mut socket, "term-42").await;
    assert_eq!(server.state.terminals().len(), 1);

    let res = call(Method::DELETE, "/api/v1/session")
        .cookie(&cookie)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    wait_for_no_terminals(&server).await;

    let log = std::fs::read_to_string(server.dir.path().join("audit.log")).unwrap();
    assert!(log.contains("\"action\":\"terminal.open\""));
    assert!(log.contains("\"action\":\"terminal.close\""));
    // What was typed is never recorded.
    assert!(!log.contains("term-"));
}

#[tokio::test]
async fn a_window_can_connect_again_and_only_to_its_own_shell() {
    let (server, addr, cookie) = terminal_server().await;
    let origin = format!("http://{addr}");
    let mut socket = open(addr, Some(&cookie), Some(&origin), "").await.unwrap();
    let id = ready(&mut socket).await;
    type_line(&mut socket, "echo first-$((1+1))").await;
    read_until(&mut socket, "first-2").await;

    // The connection is lost. The shell is kept.
    drop(socket);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.state.terminals().len(), 1);

    // Another sign-in session can't take over the shell: it gets a new one.
    let other = sign_in(&server, NEW_PASSWORD).await;
    let mut stranger = open(addr, Some(&other), Some(&origin), &format!("id={id}"))
        .await
        .unwrap();
    assert_ne!(ready(&mut stranger).await, id);
    stranger.send(Message::Text("close".into())).await.unwrap();

    // The same session gets the same shell, with the output it missed.
    let mut again = open(addr, Some(&cookie), Some(&origin), &format!("id={id}"))
        .await
        .unwrap();
    assert_eq!(ready(&mut again).await, id);
    read_until(&mut again, "first-2").await;

    // Closing the window stops the shell at once.
    again.send(Message::Text("close".into())).await.unwrap();
    wait_for_no_terminals(&server).await;
}

#[tokio::test]
async fn a_password_change_stops_every_shell() {
    let (server, addr, cookie) = terminal_server().await;
    let origin = format!("http://{addr}");
    let mut socket = open(addr, Some(&cookie), Some(&origin), "").await.unwrap();
    ready(&mut socket).await;
    assert_eq!(server.state.terminals().len(), 1);

    let res = change_call(&cookie, NEW_PASSWORD, "another unusual phrase")
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    wait_for_no_terminals(&server).await;
}
