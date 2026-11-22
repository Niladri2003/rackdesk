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

/// Starts the server with an empty data folder and returns the first line
/// it prints. `journal_stream` makes the value of `JOURNAL_STREAM` from the
/// write end of the pipe that becomes the standard output.
fn first_line_of_a_first_start(
    journal_stream: impl FnOnce(&std::io::PipeWriter) -> String,
) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let (reader, writer) = std::io::pipe().unwrap();
    let value = journal_stream(&writer);
    let mut server = Command::new(BINARY)
        .arg("--data-dir")
        .arg(tmp.path().join("data"))
        .args(["--listen", "127.0.0.1:0"])
        .env("JOURNAL_STREAM", value)
        .stdout(writer)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // The line about the new account is the first thing on stdout. A first
    // start prints an empty line before the password block.
    let mut lines = BufReader::new(reader).lines();
    let mut line = String::new();
    while line.trim().is_empty() {
        line = lines.next().unwrap().unwrap();
    }
    server.kill().unwrap();
    server.wait().unwrap();
    line
}

/// systemd keeps the output of a service in the journal, and names that
/// stream in `JOURNAL_STREAM`. The first start must not write the password
/// there.
#[cfg(target_os = "linux")]
#[test]
fn the_first_start_into_the_journal_prints_no_password() {
    let line = first_line_of_a_first_start(|stdout| {
        let stat = rustix::fs::fstat(stdout).unwrap();
        format!("{}:{}", stat.st_dev, stat.st_ino)
    });
    assert!(line.contains("admin account created"), "got: {line}");
    assert!(line.contains("reset-password"), "got: {line}");
    assert!(!line.contains("password: "), "got: {line}");
}

/// A process started by a service inherits `JOURNAL_STREAM`, also when its
/// own output goes somewhere else. A CI runner is such a service. The
/// password must then be printed as usual.
#[test]
fn an_inherited_journal_variable_does_not_hide_the_password() {
    let line = first_line_of_a_first_start(|_| "8:12345".to_owned());
    assert!(
        line.contains("RackDesk admin account created"),
        "got: {line}"
    );
    assert!(!line.contains("reset-password"), "got: {line}");
}
