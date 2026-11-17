//! The messages between the server (`rackdeskd`) and the root helper
//! (`rackdesk-helper`), and how they are put on the socket.
//!
//! The helper runs as root, so this is the most sensitive input in the
//! project (docs/security/SECURITY.md, P1). The format is small on purpose:
//!
//! - A message is a frame: 4 bytes with the length (big-endian), then that
//!   many bytes of JSON.
//! - A request frame is at most [`MAX_REQUEST_BYTES`]. A longer one is
//!   refused before any of it is read into memory.
//! - [`Request`] is the full list of what the helper can do. An unknown
//!   action or an unknown field is an error, not something to ignore.
//! - The answer to `Read` is a frame with [`Response::Data`], then the raw
//!   bytes of the file part.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Largest request frame, in bytes. A request holds 1 path and a few numbers.
pub const MAX_REQUEST_BYTES: u32 = 16 * 1024;
/// Largest response frame, in bytes.
pub const MAX_RESPONSE_BYTES: u32 = 64 * 1024;
/// Largest part of a file in 1 `Read` answer.
pub const MAX_READ_BYTES: u32 = 1024 * 1024;

/// Everything the helper can be asked to do. Paths are beneath the files
/// root of the helper, in the same form as in the HTTP API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    /// Checks that the helper answers.
    Ping,
    /// Describes an entry: its kind, size, owner and permission bits.
    Stat { path: String },
    /// Sets the 9 plain permission bits.
    Chmod { path: String, mode: u32 },
    /// Sets the owner, the group, or both.
    Chown {
        path: String,
        uid: Option<u32>,
        gid: Option<u32>,
    },
    /// Reads a part of a regular file. `len` is at most [`MAX_READ_BYTES`].
    Read { path: String, offset: u64, len: u32 },
}

impl Request {
    /// A short name for logs, such as "chmod".
    pub fn name(&self) -> &'static str {
        match self {
            Request::Ping => "ping",
            Request::Stat { .. } => "stat",
            Request::Chmod { .. } => "chmod",
            Request::Chown { .. } => "chown",
            Request::Read { .. } => "read",
        }
    }

    pub fn path(&self) -> &str {
        match self {
            Request::Ping => "",
            Request::Stat { path }
            | Request::Chmod { path, .. }
            | Request::Chown { path, .. }
            | Request::Read { path, .. } => path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Stat {
        is_file: bool,
        is_dir: bool,
        /// Bytes.
        size: u64,
        uid: u32,
        gid: u32,
        /// Permission bits, such as 0o644.
        mode: u32,
    },
    /// `len` raw bytes follow this frame. Fewer than asked for means the
    /// file ends there.
    Data {
        len: u32,
    },
    /// `code` is one of the error codes of the HTTP API, such as
    /// "not_found" or "permission_denied".
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("socket: {0}")]
    Io(#[from] std::io::Error),
    #[error("a frame of {0} bytes is over the limit")]
    TooLarge(u32),
    #[error("not a valid message: {0}")]
    Invalid(String),
}

/// Reads 1 frame of at most `max` bytes.
fn read_frame(from: &mut impl Read, max: u32) -> Result<Vec<u8>, ProtoError> {
    let mut len = [0u8; 4];
    from.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len);
    if len > max {
        return Err(ProtoError::TooLarge(len));
    }
    let mut body = vec![0u8; len as usize];
    from.read_exact(&mut body)?;
    Ok(body)
}

fn write_frame(to: &mut impl Write, body: &[u8], max: u32) -> Result<(), ProtoError> {
    let len = u32::try_from(body.len()).unwrap_or(u32::MAX);
    if len > max {
        return Err(ProtoError::TooLarge(len));
    }
    to.write_all(&len.to_be_bytes())?;
    to.write_all(body)?;
    to.flush()?;
    Ok(())
}

pub fn read_request(from: &mut impl Read) -> Result<Request, ProtoError> {
    let body = read_frame(from, MAX_REQUEST_BYTES)?;
    // The JSON library also accepts a request written as a list, such as
    // ["chmod", "/a", 420]. Only the object form is part of this protocol.
    if body.first() != Some(&b'{') {
        return Err(ProtoError::Invalid("a request is a JSON object".into()));
    }
    serde_json::from_slice(&body).map_err(|e| ProtoError::Invalid(e.to_string()))
}

pub fn write_request(to: &mut impl Write, request: &Request) -> Result<(), ProtoError> {
    let body = serde_json::to_vec(request).map_err(|e| ProtoError::Invalid(e.to_string()))?;
    write_frame(to, &body, MAX_REQUEST_BYTES)
}

/// Reads a response and, for `Data`, the bytes that follow it.
pub fn read_response(from: &mut impl Read) -> Result<(Response, Vec<u8>), ProtoError> {
    let body = read_frame(from, MAX_RESPONSE_BYTES)?;
    let response: Response =
        serde_json::from_slice(&body).map_err(|e| ProtoError::Invalid(e.to_string()))?;
    let mut data = Vec::new();
    if let Response::Data { len } = response {
        if len > MAX_READ_BYTES {
            return Err(ProtoError::TooLarge(len));
        }
        data.resize(len as usize, 0);
        from.read_exact(&mut data)?;
    }
    Ok((response, data))
}

/// Writes a response. `data` is sent after a `Data` response and must have
/// the length that the response states.
pub fn write_response(
    to: &mut impl Write,
    response: &Response,
    data: &[u8],
) -> Result<(), ProtoError> {
    let body = serde_json::to_vec(response).map_err(|e| ProtoError::Invalid(e.to_string()))?;
    write_frame(to, &body, MAX_RESPONSE_BYTES)?;
    if matches!(response, Response::Data { .. }) {
        to.write_all(data)?;
        to.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn frame(body: &[u8]) -> Vec<u8> {
        let mut out = (body.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn a_request_survives_the_trip() {
        let request = Request::Chown {
            path: "/srv/app".into(),
            uid: Some(1000),
            gid: None,
        };
        let mut wire = Vec::new();
        write_request(&mut wire, &request).unwrap();
        assert_eq!(read_request(&mut Cursor::new(wire)).unwrap(), request);
    }

    #[test]
    fn data_follows_its_response() {
        let mut wire = Vec::new();
        write_response(&mut wire, &Response::Data { len: 5 }, b"hello").unwrap();
        let (response, data) = read_response(&mut Cursor::new(wire)).unwrap();
        assert_eq!(response, Response::Data { len: 5 });
        assert_eq!(data, b"hello");
    }

    #[test]
    fn unknown_actions_and_fields_are_errors() {
        for body in [
            r#"{"op":"exec","cmd":"id"}"#,
            r#"{"op":"chmod","path":"/a","mode":420,"recursive":true}"#,
            r#"{"op":"chmod","path":"/a"}"#,
            r#"{"op":"chmod","path":"/a","mode":-1}"#,
            r#"{"op":"chmod","path":"/a","mode":4294967296}"#,
            r#"{"path":"/a"}"#,
            r#"["chmod","/a",420]"#,
            r#"{"op":"ping"}{"op":"ping"}"#,
            "",
            "null",
        ] {
            let got = read_request(&mut Cursor::new(frame(body.as_bytes())));
            assert!(
                matches!(got, Err(ProtoError::Invalid(_))),
                "{body} gave {got:?}"
            );
        }
    }

    #[test]
    fn a_frame_over_the_limit_is_refused_before_it_is_read() {
        // The length says 4 GiB. No such body follows, and none is needed:
        // the answer must come from the length alone.
        let wire = u32::MAX.to_be_bytes().to_vec();
        assert!(matches!(
            read_request(&mut Cursor::new(wire)),
            Err(ProtoError::TooLarge(u32::MAX))
        ));
        let just_over = (MAX_REQUEST_BYTES + 1).to_be_bytes().to_vec();
        assert!(matches!(
            read_request(&mut Cursor::new(just_over)),
            Err(ProtoError::TooLarge(_))
        ));
    }

    /// Random bytes as input. The reader must return an error or a request.
    /// It must never panic or ask for more memory than the frame limit.
    #[test]
    fn random_input_never_panics() {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let samples = [
            r#"{"op":"read","path":"/etc/shadow","offset":0,"len":10}"#,
            r#"{"op":"chown","path":"/a","uid":0,"gid":0}"#,
            r#"{"op":"ping"}"#,
        ];
        for round in 0..20_000 {
            let mut input: Vec<u8> = if round % 3 == 0 {
                // A valid frame with some bytes changed.
                let mut f = frame(samples[round % samples.len()].as_bytes());
                for _ in 0..(1 + next() % 4) {
                    let at = (next() as usize) % f.len();
                    f[at] = next() as u8;
                }
                f
            } else {
                (0..(next() % 64)).map(|_| next() as u8).collect()
            };
            if round % 7 == 0 {
                input.truncate((next() as usize) % (input.len() + 1));
            }
            let _ = read_request(&mut Cursor::new(&input));
            let _ = read_response(&mut Cursor::new(&input));
        }
    }
}
