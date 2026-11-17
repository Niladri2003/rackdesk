//! The server's side of the root helper (`rackdesk-helper`).
//!
//! The server runs with no special rights. For an action that needs root,
//! and only when the admin asked for it with `elevate`, it sends 1 request
//! to the helper over a local socket. Without a configured helper socket,
//! such a request is refused.

use crate::error::ApiErr;
use axum::http::StatusCode;
use rd_helper_proto::{read_response, write_request, ProtoError, Request, Response};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// The helper answers a request in milliseconds. A longer wait means it is
/// stuck or gone.
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct Helper {
    socket: PathBuf,
}

impl Helper {
    pub fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    /// Sends 1 request and waits for the answer. An error answer from the
    /// helper becomes an `ApiErr` with the same code.
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
            Ok((Response::Error { code, message }, _)) => Err(from_helper(&code, message)),
            Ok(answer) => Ok(answer),
            Err(e) => {
                tracing::error!(error = %e, "the helper did not answer");
                Err(ApiErr::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "helper_unavailable",
                    "The helper for administrator actions did not answer.",
                ))
            }
        }
    }
}

/// The helper uses the error codes of the file API. Any other code is a
/// fault in the helper and is not passed to the client.
fn from_helper(code: &str, message: String) -> ApiErr {
    let (status, code) = match code {
        "not_found" => (StatusCode::NOT_FOUND, "not_found"),
        "permission_denied" => (StatusCode::FORBIDDEN, "permission_denied"),
        "invalid_path" => (StatusCode::BAD_REQUEST, "invalid_path"),
        "not_a_file" => (StatusCode::BAD_REQUEST, "not_a_file"),
        "not_a_directory" => (StatusCode::BAD_REQUEST, "not_a_directory"),
        "bad_request" => (StatusCode::BAD_REQUEST, "bad_request"),
        other => return ApiErr::internal(format!("the helper answered with {other}: {message}")),
    };
    ApiErr::new(status, code, message)
}

pub fn no_helper() -> ApiErr {
    ApiErr::new(
        StatusCode::BAD_REQUEST,
        "no_helper",
        "This server has no helper for administrator actions.",
    )
}
