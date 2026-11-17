//! The file browser API: list, describe, search and download.
//!
//! Every path from a request goes to `rd_fs::Root`, which confines it beneath
//! the files root. Nothing here builds a filesystem path from request data.
//! The rules for serving file content are in docs/security/SECURITY.md, F1
//! and F2.

use crate::error::ApiErr;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rd_api::{DirEntry, DirListing, Disk, EntryKind, Places, SearchResults, SortKey};
use rd_fs::{FsError, ListOptions, Root, SearchLimits};
use serde::Deserialize;
use std::io::SeekFrom;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

/// Entries on a page when the request gives no limit.
const DEFAULT_PAGE: u32 = 200;
/// Longest search or filter text.
const MAX_QUERY_CHARS: usize = 100;
/// Most filesystems listed in `Places`.
const MAX_DISKS: usize = 16;
/// Bytes read from the file per chunk of the response body.
const CHUNK_BYTES: usize = 64 * 1024;

/// Sent with file content. A browser that shows the file as a page of its
/// own then runs no script from it and gives it no access to RackDesk.
const FILE_CSP: &str = "default-src 'none'; sandbox";

impl From<FsError> for ApiErr {
    fn from(e: FsError) -> Self {
        let status = match e {
            FsError::NotFound => StatusCode::NOT_FOUND,
            FsError::PermissionDenied => StatusCode::FORBIDDEN,
            FsError::InvalidPath(_)
            | FsError::NotADirectory
            | FsError::NotAFile
            | FsError::CrossDevice
            | FsError::TooMany => StatusCode::BAD_REQUEST,
            FsError::AlreadyExists | FsError::OffsetMismatch(_) => StatusCode::CONFLICT,
            FsError::ChangedOnDisk => StatusCode::PRECONDITION_FAILED,
            FsError::Io(ref io) if io.kind() == std::io::ErrorKind::StorageFull => {
                return ApiErr::new(
                    StatusCode::INSUFFICIENT_STORAGE,
                    "no_space",
                    "There is not enough free space on the disk.",
                );
            }
            FsError::Io(ref io) => return ApiErr::internal(io),
        };
        // The text names no server path. `InvalidPath` holds a fixed reason.
        ApiErr::new(status, e.code(), e.to_string())
    }
}

impl From<QueryRejection> for ApiErr {
    fn from(_: QueryRejection) -> Self {
        ApiErr::bad_request("The query parameters are not valid.")
    }
}

fn root_path() -> String {
    "/".into()
}

/// Runs a file operation off the async runtime.
pub(crate) async fn blocking<T, F>(state: &AppState, op: F) -> Result<T, ApiErr>
where
    T: Send + 'static,
    F: FnOnce(&Root) -> Result<T, FsError> + Send + 'static,
{
    let root: Arc<Root> = state.files();
    tokio::task::spawn_blocking(move || op(&root))
        .await
        .map_err(ApiErr::internal)?
        .map_err(ApiErr::from)
}

fn check_query_text(text: &str) -> Result<(), ApiErr> {
    if text.chars().count() > MAX_QUERY_CHARS {
        return Err(ApiErr::bad_request("The search text is too long."));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "root_path")]
    path: String,
    #[serde(default)]
    offset: u32,
    limit: Option<u32>,
    #[serde(default)]
    sort: SortKey,
    #[serde(default)]
    desc: bool,
    #[serde(default)]
    hidden: bool,
    /// Keeps entries whose name contains this text.
    q: Option<String>,
}

/// `GET /api/v1/fs/list`
pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<DirListing>, ApiErr> {
    let Query(q) = query?;
    let filter = q.q.filter(|text| !text.trim().is_empty());
    if let Some(text) = &filter {
        check_query_text(text)?;
    }
    let opts = ListOptions {
        offset: q.offset,
        limit: q.limit.unwrap_or(DEFAULT_PAGE),
        sort: q.sort,
        descending: q.desc,
        show_hidden: q.hidden,
        filter,
    };
    Ok(Json(
        blocking(&state, move |root| root.list(&q.path, &opts)).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct PathQuery {
    path: String,
}

/// `GET /api/v1/fs/entry`
pub async fn entry(
    State(state): State<AppState>,
    query: Result<Query<PathQuery>, QueryRejection>,
) -> Result<Json<DirEntry>, ApiErr> {
    let Query(q) = query?;
    Ok(Json(
        blocking(&state, move |root| root.entry(&q.path)).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    q: String,
    /// The folder to search beneath. Without it, the home folder is
    /// searched first and then the whole files root.
    path: Option<String>,
}

/// `GET /api/v1/fs/search`
pub async fn search(
    State(state): State<AppState>,
    query: Result<Query<SearchQuery>, QueryRejection>,
) -> Result<Json<SearchResults>, ApiErr> {
    let Query(q) = query?;
    check_query_text(&q.q)?;
    // A search can read many folders. Only a few run at once.
    let _permit = state.search_permit().await?;
    let files_root = state.config().files_root.clone();
    Ok(Json(
        blocking(&state, move |root| {
            let limits = SearchLimits::default();
            if let Some(path) = &q.path {
                return root.search(path, &q.q, &limits);
            }
            // Most files a person looks for are in the home folder, and a
            // walk from "/" can use up its time before it gets there. So
            // each of the 2 walks gets half of the time.
            let half = SearchLimits {
                max_time: limits.max_time / 2,
                ..limits
            };
            let mut found = match home_folder(&files_root, root) {
                Some(home) if home != "/" => root.search(&home, &q.q, &half)?,
                _ => SearchResults {
                    hits: Vec::new(),
                    truncated: false,
                },
            };
            if found.hits.len() < limits.max_hits {
                let rest = SearchLimits {
                    max_hits: limits.max_hits - found.hits.len(),
                    ..half
                };
                let everywhere = root.search("/", &q.q, &rest)?;
                found.truncated |= everywhere.truncated;
                for hit in everywhere.hits {
                    if !found.hits.contains(&hit) {
                        found.hits.push(hit);
                    }
                }
            }
            Ok(found)
        })
        .await?,
    ))
}

/// The home folder of the user that runs the server, as a UI path. `None`
/// if it is outside the files root or is not a folder.
pub fn home_folder(files_root: &Path, root: &Root) -> Option<String> {
    std::env::var_os("HOME")
        .and_then(|home| ui_path(files_root, Path::new(&home)))
        .filter(|path| root.entry(path).is_ok_and(|e| e.kind == EntryKind::Dir))
}

/// `GET /api/v1/fs/places`
pub async fn places(State(state): State<AppState>) -> Result<Json<Places>, ApiErr> {
    let files_root = state.config().files_root.clone();
    Ok(Json(
        blocking(&state, move |root| {
            let home = home_folder(&files_root, root);
            let mut disks = Vec::new();
            for path in mount_points(&files_root) {
                if let Ok((total, free)) = root.disk_usage(&path) {
                    if total > 0 {
                        disks.push(Disk { path, total, free });
                    }
                }
            }
            Ok(Places { home, disks })
        })
        .await?,
    ))
}

/// Turns an absolute server path into a path beneath the files root, as the
/// UI writes it. `None` if it is outside the root or is not valid UTF-8.
fn ui_path(files_root: &Path, absolute: &Path) -> Option<String> {
    let rel = absolute.strip_prefix(files_root).ok()?.to_str()?;
    Some(if rel.is_empty() {
        "/".into()
    } else {
        format!("/{rel}")
    })
}

/// The files root, then each mounted disk beneath it. The list comes from
/// the kernel (`/proc/self/mounts`), which exists on Linux only.
fn mount_points(files_root: &Path) -> Vec<String> {
    let mut paths = vec!["/".to_owned()];
    let mut devices = Vec::new();
    let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    for line in mounts.lines() {
        let mut fields = line.split(' ');
        let (Some(device), Some(mount)) = (fields.next(), fields.next()) else {
            continue;
        };
        // Real disks only. This leaves out proc, sysfs, tmpfs and overlays.
        if !device.starts_with("/dev/") || devices.contains(&device) {
            continue;
        }
        let mount = unescape_mount(mount);
        if let Some(path) = ui_path(files_root, Path::new(&mount)) {
            devices.push(device);
            if !paths.contains(&path) && paths.len() < MAX_DISKS {
                paths.push(path);
            }
        }
    }
    paths
}

/// The kernel writes a space in a mount path as `\040`, and likewise for tab
/// (`\011`), newline (`\012`) and backslash (`\134`).
fn unescape_mount(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        let code = rest.get(i + 1..i + 4);
        match code.and_then(|c| u8::from_str_radix(c, 8).ok()) {
            Some(byte) if byte.is_ascii() => {
                out.push(byte as char);
                rest = &rest[i + 4..];
            }
            _ => {
                out.push('\\');
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
    path: String,
    /// Ask for the file to be shown in the page, not saved. The server
    /// agrees only for the types in [`inline_type`].
    #[serde(default)]
    inline: bool,
}

/// The content type for a file that may be shown inside the RackDesk page:
/// images, video, audio and PDF. The list has no type that can run script in
/// our origin. HTML, SVG and XML are never on it.
fn inline_type(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "pdf" => "application/pdf",
        _ => return None,
    })
}

/// The part of a file to send.
#[derive(Debug, PartialEq, Eq)]
enum Range {
    Full,
    /// First and last byte, both included.
    Part(u64, u64),
    /// The range starts after the end of the file.
    Unsatisfiable,
}

/// Reads a `Range` header for a file of `len` bytes. Only 1 range is
/// supported. Anything else is answered with the whole file, as HTTP allows.
fn parse_range(header: Option<&str>, len: u64) -> Range {
    let Some(spec) = header.and_then(|h| h.strip_prefix("bytes=")) else {
        return Range::Full;
    };
    let Some((first, last)) = spec.trim().split_once('-') else {
        return Range::Full;
    };
    if spec.contains(',') {
        return Range::Full;
    }
    match (first.parse::<u64>(), last.parse::<u64>()) {
        // "bytes=-500": the last 500 bytes.
        (Err(_), Ok(count)) if first.is_empty() => {
            if count == 0 || len == 0 {
                Range::Unsatisfiable
            } else {
                Range::Part(len.saturating_sub(count), len - 1)
            }
        }
        // "bytes=500-": from byte 500 to the end.
        (Ok(start), Err(_)) if last.is_empty() => {
            if start >= len {
                Range::Unsatisfiable
            } else {
                Range::Part(start, len - 1)
            }
        }
        (Ok(start), Ok(end)) if start <= end => {
            if start >= len {
                Range::Unsatisfiable
            } else {
                Range::Part(start, end.min(len - 1))
            }
        }
        _ => Range::Full,
    }
}

/// `Content-Disposition` with the file name in 2 forms: plain ASCII for old
/// clients, and UTF-8 with percent-encoding (RFC 5987) for the rest.
fn content_disposition(kind: &str, name: &str) -> HeaderValue {
    let ascii: String = name
        .chars()
        .map(|c| match c {
            '"' | '\\' | '/' => '_',
            c if c.is_ascii() && !c.is_ascii_control() => c,
            _ => '_',
        })
        .collect();
    let mut encoded = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    let value = format!("{kind}; filename=\"{ascii}\"; filename*=UTF-8''{encoded}");
    // Both forms hold visible ASCII only, so this can't fail.
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

/// `GET /api/v1/fs/download`
///
/// Sends a file with support for `Range`, so a download can resume and a
/// video can seek. By default the browser saves the file. It is shown in the
/// page only on request, and only for the types in [`inline_type`].
pub async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<DownloadQuery>, QueryRejection>,
) -> Result<Response, ApiErr> {
    let Query(q) = query?;
    let name = q
        .path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned();
    let path = q.path;
    let open = blocking(&state, move |root| root.open_file(&path)).await?;
    let len = open.len;

    let shown = if q.inline { inline_type(&name) } else { None };
    let content_type = shown.unwrap_or("application/octet-stream");
    let disposition = if shown.is_some() {
        "inline"
    } else {
        "attachment"
    };
    // Changes when the size or the modified time of the file changes. The
    // editor sends it back with a save, as `If-Match`.
    let etag = format!("\"{}\"", open.version);

    let range_header = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        // `If-Range` asks for the part only if the file has not changed.
        .filter(|_| {
            headers
                .get(header::IF_RANGE)
                .is_none_or(|v| v.as_bytes() == etag.as_bytes())
        });
    let (status, start, count) = match parse_range(range_header, len) {
        Range::Full => (StatusCode::OK, 0, len),
        Range::Part(first, last) => (StatusCode::PARTIAL_CONTENT, first, last - first + 1),
        Range::Unsatisfiable => {
            let mut res = ApiErr::new(
                StatusCode::RANGE_NOT_SATISFIABLE,
                "range_not_satisfiable",
                "The requested range starts after the end of the file.",
            )
            .into_response();
            if let Ok(value) = HeaderValue::from_str(&format!("bytes */{len}")) {
                res.headers_mut().insert(header::CONTENT_RANGE, value);
            }
            return Ok(res);
        }
    };

    let mut file = tokio::fs::File::from_std(open.file);
    if start > 0 {
        file.seek(SeekFrom::Start(start))
            .await
            .map_err(ApiErr::internal)?;
    }
    let body = Body::from_stream(ReaderStream::with_capacity(file.take(count), CHUNK_BYTES));

    let mut res = Response::new(body);
    *res.status_mut() = status;
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(count));
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(
        header::CONTENT_DISPOSITION,
        content_disposition(disposition, &name),
    );
    if let Ok(value) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, value);
    }
    if status == StatusCode::PARTIAL_CONTENT {
        let value = format!("bytes {start}-{}/{len}", start + count - 1);
        if let Ok(value) = HeaderValue::from_str(&value) {
            h.insert(header::CONTENT_RANGE, value);
        }
    }
    // The browser's PDF viewer does not work in a sandbox. A PDF gets the
    // normal policy of the app, which already forbids script from files.
    if content_type != "application/pdf" {
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(FILE_CSP),
        );
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_header_forms() {
        assert_eq!(parse_range(None, 100), Range::Full);
        assert_eq!(parse_range(Some("bytes=0-9"), 100), Range::Part(0, 9));
        assert_eq!(parse_range(Some("bytes=90-"), 100), Range::Part(90, 99));
        assert_eq!(parse_range(Some("bytes=-10"), 100), Range::Part(90, 99));
        assert_eq!(parse_range(Some("bytes=0-0"), 100), Range::Part(0, 0));
    }

    #[test]
    fn a_range_is_cut_at_the_end_of_the_file() {
        assert_eq!(parse_range(Some("bytes=50-999"), 100), Range::Part(50, 99));
        assert_eq!(parse_range(Some("bytes=-999"), 100), Range::Part(0, 99));
    }

    #[test]
    fn a_range_after_the_end_is_unsatisfiable() {
        assert_eq!(parse_range(Some("bytes=100-"), 100), Range::Unsatisfiable);
        assert_eq!(
            parse_range(Some("bytes=100-200"), 100),
            Range::Unsatisfiable
        );
        assert_eq!(parse_range(Some("bytes=0-"), 0), Range::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=-0"), 100), Range::Unsatisfiable);
    }

    #[test]
    fn unsupported_or_broken_ranges_give_the_whole_file() {
        for header in [
            "bytes=0-1,5-6",
            "bytes=9-1",
            "bytes=a-b",
            "bytes=",
            "bytes=-",
            "items=0-1",
            "bytes=18446744073709551616-",
        ] {
            assert_eq!(parse_range(Some(header), 100), Range::Full, "{header}");
        }
    }

    #[test]
    fn only_safe_types_are_shown_inline() {
        assert_eq!(inline_type("photo.JPG"), Some("image/jpeg"));
        assert_eq!(inline_type("clip.mp4"), Some("video/mp4"));
        assert_eq!(inline_type("doc.pdf"), Some("application/pdf"));
        for name in [
            "page.html",
            "page.htm",
            "drawing.svg",
            "feed.xml",
            "app.js",
            "notes.txt",
            "archive.png.html",
            "noextension",
            ".png",
        ] {
            if name == ".png" {
                // A file named only ".png" is still an image by its name.
                assert_eq!(inline_type(name), Some("image/png"));
            } else {
                assert_eq!(inline_type(name), None, "{name}");
            }
        }
    }

    #[test]
    fn file_names_are_encoded_in_the_download_header() {
        let plain = content_disposition("attachment", "report.pdf");
        assert_eq!(
            plain,
            "attachment; filename=\"report.pdf\"; filename*=UTF-8''report.pdf"
        );
        let odd = content_disposition("attachment", "a \"b\"\r\n.txt");
        let odd = odd.to_str().unwrap();
        assert!(odd.contains("filename=\"a _b___.txt\""));
        assert!(odd.contains("filename*=UTF-8''a%20%22b%22%0D%0A.txt"));
        let utf8 = content_disposition("inline", "caf\u{e9}.png");
        assert_eq!(
            utf8,
            "inline; filename=\"caf_.png\"; filename*=UTF-8''caf%C3%A9.png"
        );
    }

    #[test]
    fn mount_paths_are_unescaped() {
        assert_eq!(unescape_mount("/mnt/data"), "/mnt/data");
        assert_eq!(unescape_mount("/mnt/my\\040disk"), "/mnt/my disk");
        assert_eq!(unescape_mount("/mnt/a\\134b"), "/mnt/a\\b");
        assert_eq!(unescape_mount("/mnt/odd\\"), "/mnt/odd\\");
        assert_eq!(unescape_mount("/mnt/odd\\9"), "/mnt/odd\\9");
    }

    #[test]
    fn server_paths_map_to_ui_paths_beneath_the_root() {
        let root = Path::new("/srv/files");
        assert_eq!(ui_path(root, Path::new("/srv/files")), Some("/".into()));
        assert_eq!(
            ui_path(root, Path::new("/srv/files/a/b")),
            Some("/a/b".into())
        );
        assert_eq!(ui_path(root, Path::new("/srv/other")), None);
        assert_eq!(
            ui_path(Path::new("/"), Path::new("/home/dev")),
            Some("/home/dev".into())
        );
    }
}
