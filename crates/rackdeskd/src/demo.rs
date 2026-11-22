//! Demo mode: the server with sample files, for trying RackDesk on any
//! machine before it is installed on a server.
//!
//! The data folder and the files root are 2 new folders in the temporary
//! folder of the system. They are deleted when the server stops. The file
//! browser sees the sample files and nothing else on the machine.
//!
//! The terminal is not part of the demo. A shell is not confined to the
//! files root: it would reach every file of the user that runs the demo.

use crate::term::TermMode;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;

/// The largest upload in demo mode. It keeps a test upload from filling the
/// temporary folder of the machine.
pub const MAX_UPLOAD_BYTES: u64 = 64 * 1024 * 1024;

/// The settings that demo mode can't be combined with.
pub struct Request<'a> {
    pub listen: &'a SocketAddr,
    pub terminal: TermMode,
    pub root_socket: bool,
}

/// Refuses a combination that would let the demo reach real files, or would
/// offer it to other machines. `Err` holds the text for the user.
pub fn check(request: &Request) -> Result<(), &'static str> {
    if !request.listen.ip().is_loopback() {
        return Err("--demo listens on this machine only. Use a 127.0.0.1 or [::1] address");
    }
    if request.terminal != TermMode::Off {
        return Err(
            "--demo has no terminal: a shell could reach the real files of this machine. Remove --terminal",
        );
    }
    if request.root_socket {
        return Err("--demo does not use rackdesk-root. Remove --root-socket");
    }
    Ok(())
}

/// The folders of a running demo. Dropping it deletes them.
pub struct Demo {
    dir: tempfile::TempDir,
}

impl Demo {
    /// Makes the 2 folders and writes the sample files.
    pub fn create() -> io::Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("rackdesk-demo-")
            .tempdir()?;
        let demo = Self { dir };
        fs::create_dir(demo.data_dir())?;
        let root = demo.files_root();
        for (path, content) in SAMPLE_FILES {
            let file = root.join(path);
            if let Some(folder) = file.parent() {
                fs::create_dir_all(folder)?;
            }
            fs::write(file, content)?;
        }
        Ok(demo)
    }

    pub fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    pub fn files_root(&self) -> PathBuf {
        self.dir.path().join("files")
    }
}

/// A small server in files: a web server config, an app, logs and notes.
const SAMPLE_FILES: &[(&str, &str)] = &[
    (
        "README.txt",
        "This is the RackDesk demo.\n\
         \n\
         These files are samples. They are in a temporary folder and are deleted when\n\
         the demo stops. Nothing here changes the real files of this machine.\n\
         \n\
         Things to try:\n\
         - Open etc/nginx/nginx.conf, change a line and save it.\n\
         - Drag a file from your computer into this window to upload it.\n\
         - Select a few files and make a zip archive of them.\n\
         - Delete a file, then restore it from the Trash.\n\
         - Press Ctrl K (Cmd K on a Mac) and search for \"nginx\".\n\
         - Open Settings, then Activity, to see every change you made.\n",
    ),
    (
        "etc/nginx/nginx.conf",
        "user www-data;\n\
         worker_processes auto;\n\
         pid /run/nginx.pid;\n\
         \n\
         events {\n\
         \x20   worker_connections 768;\n\
         }\n\
         \n\
         http {\n\
         \x20   sendfile on;\n\
         \x20   keepalive_timeout 65;\n\
         \x20   include /etc/nginx/mime.types;\n\
         \x20   access_log /var/log/nginx/access.log;\n\
         \x20   error_log /var/log/nginx/error.log;\n\
         \x20   include /etc/nginx/sites-enabled/*;\n\
         }\n",
    ),
    (
        "etc/nginx/sites-enabled/app.conf",
        "server {\n\
         \x20   listen 443 ssl;\n\
         \x20   server_name example.org;\n\
         \x20   root /srv/app/public;\n\
         \n\
         \x20   location /api/ {\n\
         \x20       proxy_pass http://127.0.0.1:3000;\n\
         \x20   }\n\
         }\n",
    ),
    (
        "etc/hosts",
        "127.0.0.1 localhost\n127.0.1.1 server\n",
    ),
    (
        "srv/app/public/index.html",
        "<!doctype html>\n<title>Example app</title>\n<h1>It works</h1>\n",
    ),
    (
        "srv/app/config.yml",
        "# Settings of the example app\nport: 3000\ndatabase: /var/lib/app/app.db\nlog_level: info\n",
    ),
    (
        "srv/app/deploy.sh",
        "#!/bin/sh\n# Pulls the newest code and restarts the app.\nset -eu\ngit pull --ff-only\nsystemctl restart app\n",
    ),
    (
        "var/log/nginx/access.log",
        "203.0.113.7 - - [12/Nov/2026:09:14:02 +0000] \"GET / HTTP/1.1\" 200 612\n\
         203.0.113.7 - - [12/Nov/2026:09:14:03 +0000] \"GET /api/status HTTP/1.1\" 200 48\n\
         198.51.100.23 - - [12/Nov/2026:09:20:41 +0000] \"GET /wp-login.php HTTP/1.1\" 404 153\n",
    ),
    (
        "var/log/nginx/error.log",
        "2026/11/12 09:20:41 [error] 812#812: *31 open() \"/srv/app/public/wp-login.php\" failed (2: No such file or directory)\n",
    ),
    (
        "home/admin/notes.md",
        "# Server notes\n\n- The app runs on port 3000, behind nginx.\n- Backups run every night at 02:00.\n- The certificate is renewed by a timer.\n",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn request<'a>(listen: &'a SocketAddr, terminal: TermMode, root_socket: bool) -> Request<'a> {
        Request {
            listen,
            terminal,
            root_socket,
        }
    }

    #[test]
    fn the_demo_is_refused_on_a_public_address_with_a_terminal_or_with_the_root_program() {
        let local: SocketAddr = "127.0.0.1:7171".parse().unwrap();
        let local6: SocketAddr = "[::1]:7171".parse().unwrap();
        let public: SocketAddr = "0.0.0.0:7171".parse().unwrap();
        assert!(check(&request(&local, TermMode::Off, false)).is_ok());
        assert!(check(&request(&local6, TermMode::Off, false)).is_ok());
        assert!(check(&request(&public, TermMode::Off, false)).is_err());
        assert!(check(&request(&local, TermMode::Direct, false)).is_err());
        assert!(check(&request(&local, TermMode::AsUser, false)).is_err());
        assert!(check(&request(&local, TermMode::Off, true)).is_err());
    }

    #[test]
    fn the_folders_hold_the_samples_and_are_deleted_at_the_end() {
        let demo = Demo::create().unwrap();
        let root = demo.files_root();
        assert!(root.join("etc/nginx/nginx.conf").is_file());
        assert!(demo.data_dir().is_dir());
        let top = root.parent().unwrap().to_owned();
        drop(demo);
        assert!(!top.exists());
    }
}
