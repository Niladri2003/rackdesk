//! Tests of the `rackdeskd` program itself: the commands and the first start.
//! They run the built binary, because the behaviour under test is what it
//! prints and to where.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

const BINARY: &str = env!("CARGO_BIN_EXE_rackdeskd");

fn run(data_dir: &std::path::Path, command: &str) -> String {
    let output = Command::new(BINARY)
        .arg("--data-dir")
        .arg(data_dir)
        .arg(command)
        .env_remove("JOURNAL_STREAM")
        .output()
        .unwrap();
    assert!(output.status.success(), "{command} failed: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn init_creates_the_account_once() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");

    let first = run(&dir, "init");
    assert!(first.contains("password: "), "no password in: {first}");
    let account = std::fs::read(dir.join("credentials.json")).unwrap();

    let second = run(&dir, "init");
    assert!(!second.contains("password: "), "a password in: {second}");
    assert_eq!(
        std::fs::read(dir.join("credentials.json")).unwrap(),
        account,
        "a second init must not change the account"
    );
}

/// systemd keeps the output of a service in the journal. The first start
/// under systemd must not write the password there.
#[test]
fn the_first_start_under_systemd_prints_no_password() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = Command::new(BINARY)
        .arg("--data-dir")
        .arg(tmp.path().join("data"))
        .args(["--listen", "127.0.0.1:0"])
        .env("JOURNAL_STREAM", "8:12345")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // The line about the new account is the first thing on stdout.
    let mut line = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    server.kill().unwrap();
    server.wait().unwrap();

    assert!(line.contains("admin account created"), "got: {line}");
    assert!(line.contains("reset-password"), "got: {line}");
    assert!(!line.contains("password: "), "got: {line}");
}
