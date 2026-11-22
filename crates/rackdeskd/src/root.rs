//! The server's side of the root program (`rackdesk-root`).
//!
//! The server runs with no special rights. For an action that needs root,
//! and only when the admin asked for it with `elevate`, it sends 1 request
//! to the root program over a local socket. Without a configured socket of the root program,
//! such a request is refused.

use crate::error::ApiErr;
use axum::http::StatusCode;
use rd_root_proto::{read_response, write_request, ProtoError, Request, Response};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// The root program answers a request in milliseconds. A longer wait means it is
/// stuck or gone.
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct RootClient {
    socket: PathBuf,
}

impl RootClient {
    pub fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    /// Sends 1 request and waits for the answer. An error answer from the
    /// root program becomes an `ApiErr` with the same code.
    pub async fn call(&self, request: Request) -> Result<(Response, Vec<u8>), ApiErr> {
        let socket = self.socket.clone();
        let answer = tokio::task::spawn_blocking(move || -> Result<_, ProtoError> {
            let mut stream = UnixStream::connect(&socket)?;
            stream.set_read_timeout(Some(TIMEOUT))?;
            stream.set_write_timeout(Some(TIMEOUT))?;
            write_request(&mut stream, &request)?;
            read_response(&mut stream)
        })
        .await
        .map_err(ApiErr::internal)?;
        match answer {
            Ok((Response::Error { code, message }, _)) => Err(from_root(&code, message)),
            Ok(answer) => Ok(answer),
            Err(e) => {
                tracing::error!(error = %e, "the root program did not answer");
                Err(ApiErr::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "root_program_unavailable",
                    "rackdesk-root, the program for administrator actions, did not answer.",
                ))
            }
        }
    }
}

/// The root program uses the error codes of the file API. Any other code is a
/// fault in the root program and is not passed to the client.
fn from_root(code: &str, message: String) -> ApiErr {
    let (status, code) = match code {
        "not_found" => (StatusCode::NOT_FOUND, "not_found"),
        "permission_denied" => (StatusCode::FORBIDDEN, "permission_denied"),
        "invalid_path" => (StatusCode::BAD_REQUEST, "invalid_path"),
        "not_a_file" => (StatusCode::BAD_REQUEST, "not_a_file"),
        "not_a_directory" => (StatusCode::BAD_REQUEST, "not_a_directory"),
        "bad_request" => (StatusCode::BAD_REQUEST, "bad_request"),
        other => {
            return ApiErr::internal(format!("the root program answered with {other}: {message}"))
        }
    };
    ApiErr::new(status, code, message)
}

impl RootClient {
    /// Asks the root program for a shell. The returned socket is the terminal:
    /// output arrives as raw bytes, and input is sent as stream frames.
    pub async fn shell(&self, cols: u16, rows: u16) -> Result<tokio::net::UnixStream, ApiErr> {
        let socket = self.socket.clone();
        let opened = tokio::task::spawn_blocking(move || -> Result<_, ProtoError> {
            let mut stream = UnixStream::connect(&socket)?;
            stream.set_read_timeout(Some(TIMEOUT))?;
            stream.set_write_timeout(Some(TIMEOUT))?;
            write_request(&mut stream, &Request::Shell { cols, rows })?;
            let (response, _) = read_response(&mut stream)?;
            // From here on the socket is used by the async runtime.
            stream.set_read_timeout(None)?;
            stream.set_write_timeout(None)?;
            stream.set_nonblocking(true)?;
            Ok((response, stream))
        })
        .await
        .map_err(ApiErr::internal)?;
        match opened {
            Ok((Response::Ok, stream)) => {
                tokio::net::UnixStream::from_std(stream).map_err(ApiErr::internal)
            }
            Ok((Response::Error { code, message }, _)) => Err(match code.as_str() {
                "terminal_off" => ApiErr::new(StatusCode::NOT_FOUND, "terminal_off", message),
                _ => from_root(&code, message),
            }),
            Ok(_) => Err(ApiErr::internal(
                "the root program gave an answer that does not fit a shell request",
            )),
            Err(e) => {
                tracing::error!(error = %e, "the root program did not answer");
                Err(ApiErr::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "root_program_unavailable",
                    "rackdesk-root, the program for administrator actions, did not answer.",
                ))
            }
        }
    }
}

pub fn no_root() -> ApiErr {
    ApiErr::new(
        StatusCode::BAD_REQUEST,
        "no_root_program",
        "This server does not run rackdesk-root, the program for administrator actions.",
    )
}
