//! The file browser API, part 2: routes that change files.
//!
//! New folder, rename and move, copy, delete, trash, save, and upload. Like
//! the read routes, every path goes to `rd_fs::Root`. Each route changes
//! state, so each one passes the CSRF check in `csrf.rs` first.
//!
//! An upload is sent in chunks and can resume (docs/security/SECURITY.md,
//! F3 and F5):
//!
//! 1. `POST /fs/uploads` announces the folder, the name and the size. The
//!    server checks the limits and the free space and answers with an id.
//! 2. `PATCH /fs/uploads/{id}?offset=N` sends the bytes from position N. The
//!    server appends them to a hidden temporary file in the target folder.
//!    A chunk that does not start where the file ends is refused with the
//!    position to continue from.
//! 3. When the last byte arrives, the server renames the temporary file to
//!    its final name.
//!
//! `GET /fs/uploads/{id}` gives the position after a lost connection, and
//! `DELETE` cancels and removes the temporary file.

use crate::audit::Actor;
use crate::error::ApiErr;
use crate::files::blocking;
use crate::state::AppState;
use axum::body::Bytes;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Extension, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rd_api::{
    Conflict, PathRequest, PathResult, SaveResult, TransferRequest, TrashItem, TrashRequest,
    UploadRequest, UploadStatus,
};
use rd_fs::{ArchiveLimits, FsError};
use serde::Deserialize;
use std::time::{Duration, Instant};

/// Largest file the editor can save.
pub const MAX_SAVE_BYTES: usize = 2 * 1024 * 1024;
/// Largest chunk of an upload.
pub const MAX_CHUNK_BYTES: usize = 8 * 1024 * 1024;
/// Uploads in progress at once, for the whole server.
const MAX_UPLOADS: usize = 16;
/// An upload with no chunk for this long is removed.
const UPLOAD_IDLE: Duration = Duration::from_secs(24 * 60 * 60);
/// Free space that an upload must leave on the disk.
const SPACE_RESERVE: u64 = 64 * 1024 * 1024;

/// An upload in progress. The bytes received are the length of the
/// temporary file, so they are not stored here.
#[derive(Debug, Clone)]
pub struct Upload {
    dir: String,
    /// Name of the temporary file in `dir`. Made by the server.
    temp: String,
    name: String,
    size: u64,
    conflict: Conflict,
    last_used: Instant,
    /// Held while a chunk is added. 2 chunks of 1 upload sent at the same
    /// moment would both pass the check of the length.
    appending: std::sync::Arc<tokio::sync::Mutex<()>>,
}

type Reply<T> = Result<Json<T>, ApiErr>;

/// Adds the outcome of a change to the audit log, and passes it on.
fn audited<T>(
    state: &AppState,
    actor: &Actor,
    action: &str,
    target: &str,
    outcome: Result<T, ApiErr>,
) -> Result<T, ApiErr> {
    let result = outcome.as_ref().map(|_| ()).map_err(ApiErr::code);
    state.audit().record(actor, action, target, result);
    outcome
}

/// `POST /api/v1/fs/mkdir`
pub async fn mkdir(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<PathRequest>, JsonRejection>,
) -> Result<StatusCode, ApiErr> {
    let Json(req) = body?;
    let path = req.path.clone();
    let done = blocking(&state, move |root| root.create_dir(&req.path)).await;
    audited(&state, &actor, "fs.mkdir", &path, done)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/fs/rename`. Also moves: `to` can be in another folder.
pub async fn rename(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<TransferRequest>, JsonRejection>,
) -> Reply<PathResult> {
    let Json(req) = body?;
    let target = format!("{} -> {}", req.from, req.to);
    let done = blocking(&state, move |root| {
        root.rename(&req.from, &req.to, req.conflict)
    })
    .await;
    let path = audited(&state, &actor, "fs.rename", &target, done)?;
    Ok(Json(PathResult { path }))
}

/// `POST /api/v1/fs/copy`
pub async fn copy(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<TransferRequest>, JsonRejection>,
) -> Reply<PathResult> {
    let Json(req) = body?;
    let target = format!("{} -> {}", req.from, req.to);
    let done = blocking(&state, move |root| {
        root.copy(&req.from, &req.to, req.conflict)
    })
    .await;
    let path = audited(&state, &actor, "fs.copy", &target, done)?;
    Ok(Json(PathResult { path }))
}

/// `POST /api/v1/fs/delete`. Deletes for good.
pub async fn delete(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<PathRequest>, JsonRejection>,
) -> Result<StatusCode, ApiErr> {
    let Json(req) = body?;
    let path = req.path.clone();
    let done = blocking(&state, move |root| root.remove(&req.path)).await;
    audited(&state, &actor, "fs.delete", &path, done)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/fs/trash`. Moves an entry to the trash.
pub async fn trash(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<PathRequest>, JsonRejection>,
) -> Result<StatusCode, ApiErr> {
    let Json(req) = body?;
    let path = req.path.clone();
    let done = blocking(&state, move |root| root.trash(&req.path)).await;
    audited(&state, &actor, "fs.trash", &path, done)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/fs/trash`
pub async fn trash_list(State(state): State<AppState>) -> Reply<Vec<TrashItem>> {
    Ok(Json(blocking(&state, |root| root.trash_list()).await?))
}

/// `POST /api/v1/fs/trash/restore`
pub async fn trash_restore(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<TrashRequest>, JsonRejection>,
) -> Reply<PathResult> {
    let Json(req) = body?;
    let id = req.id.clone();
    let done = blocking(&state, move |root| {
        root.trash_restore(&req.id, req.conflict)
    })
    .await;
    let path = audited(&state, &actor, "fs.trash_restore", &id, done)?;
    Ok(Json(PathResult { path }))
}

/// `POST /api/v1/fs/trash/delete`. Deletes 1 item in the trash for good.
pub async fn trash_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<TrashRequest>, JsonRejection>,
) -> Result<StatusCode, ApiErr> {
    let Json(req) = body?;
    let id = req.id.clone();
    let done = blocking(&state, move |root| root.trash_delete(&req.id)).await;
    audited(&state, &actor, "fs.trash_delete", &id, done)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/fs/trash/empty`
pub async fn trash_empty(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
) -> Result<StatusCode, ApiErr> {
    let done = blocking(&state, |root| root.trash_empty()).await;
    audited(&state, &actor, "fs.trash_empty", "", done)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/fs/compress`. Makes a zip file at `to` from `from`.
pub async fn compress(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<TransferRequest>, JsonRejection>,
) -> Reply<PathResult> {
    let Json(req) = body?;
    let target = format!("{} -> {}", req.from, req.to);
    let done = blocking(&state, move |root| {
        root.compress(&req.from, &req.to, req.conflict, &ArchiveLimits::default())
    })
    .await;
    let path = audited(&state, &actor, "fs.compress", &target, done)?;
    Ok(Json(PathResult { path }))
}

/// `POST /api/v1/fs/extract`. Extracts the zip file `from` into a new
/// folder at `to`.
pub async fn extract(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<TransferRequest>, JsonRejection>,
) -> Reply<PathResult> {
    let Json(req) = body?;
    let target = format!("{} -> {}", req.from, req.to);
    let done = blocking(&state, move |root| {
        root.extract(&req.from, &req.to, req.conflict, &ArchiveLimits::default())
    })
    .await;
    let path = audited(&state, &actor, "fs.extract", &target, done)?;
    Ok(Json(PathResult { path }))
}

#[derive(Debug, Deserialize)]
pub struct SaveQuery {
    path: String,
    /// Write even if the file changed since it was read.
    #[serde(default)]
    force: bool,
}

/// `PUT /api/v1/fs/file`
///
/// Saves the body as the content of a file. The caller says which version
/// it read with `If-Match`, using the `ETag` from the download. If the file
/// has changed since, the answer is 412 and nothing is written. Without
/// `If-Match`, the file must not exist yet.
pub async fn save(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    query: Result<Query<SaveQuery>, QueryRejection>,
    body: Bytes,
) -> Reply<SaveResult> {
    let Query(q) = query?;
    let read_version = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().trim_matches('"').to_owned());
    let path = q.path.clone();
    let done = blocking(&state, move |root| {
        if q.force {
            return root.write_file(&q.path, &body, None);
        }
        match &read_version {
            Some(version) => root.write_file(&q.path, &body, Some(version)),
            None => match root.entry(&q.path) {
                Ok(_) => Err(FsError::AlreadyExists),
                Err(FsError::NotFound) => root.write_file(&q.path, &body, None),
                Err(e) => Err(e),
            },
        }
    })
    .await;
    let version = audited(&state, &actor, "fs.save", &path, done)?;
    Ok(Json(SaveResult {
        etag: format!("\"{version}\""),
    }))
}

fn upload_status(id: &str, upload: &Upload, offset: u64, path: Option<String>) -> UploadStatus {
    UploadStatus {
        id: id.to_owned(),
        offset,
        size: upload.size,
        path,
    }
}

fn no_such_upload() -> ApiErr {
    ApiErr::new(
        StatusCode::NOT_FOUND,
        "not_found",
        "This upload does not exist. It was finished, cancelled, or the server restarted.",
    )
}

/// `POST /api/v1/fs/uploads`
pub async fn upload_begin(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<UploadRequest>, JsonRejection>,
) -> Reply<UploadStatus> {
    let Json(req) = body?;
    // The name is 1 plain file name. A path would let the client choose
    // another folder than the one it named.
    let bad_name = req.name.is_empty()
        || req.name == "."
        || req.name == ".."
        || req.name.contains(['/', '\\', '\0'])
        || req.name.len() > 255;
    if bad_name {
        return Err(FsError::InvalidPath("the file name is not valid").into());
    }
    if req.size > state.config().max_upload_bytes {
        return Err(ApiErr::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            "The file is larger than the upload limit of this server.",
        ));
    }

    // Uploads that were left alone are removed here, when a new one starts.
    // Nothing runs in the background for this.
    let stale = state.take_stale_uploads(UPLOAD_IDLE);
    if !stale.is_empty() {
        let _ = blocking(&state, move |root| {
            for upload in &stale {
                let _ = root.upload_abort(&upload.dir, &upload.temp);
            }
            Ok(())
        })
        .await;
    }

    let id = rd_auth::random_hex(16).map_err(ApiErr::internal)?;
    let upload = Upload {
        dir: req.dir,
        temp: format!(".rd-upload-{id}.part"),
        name: req.name,
        size: req.size,
        conflict: req.conflict,
        last_used: Instant::now(),
        appending: Default::default(),
    };
    let begin = upload.clone();
    blocking(&state, move |root| {
        let target = format!("{}/{}", begin.dir, begin.name);
        if begin.conflict == Conflict::Fail && root.entry(&target).is_ok() {
            return Err(FsError::AlreadyExists);
        }
        let (_, free) = root.disk_usage(&begin.dir)?;
        if begin.size.saturating_add(SPACE_RESERVE) > free {
            return Err(FsError::Io(std::io::ErrorKind::StorageFull.into()));
        }
        root.upload_begin(&begin.dir, &begin.temp)
    })
    .await?;

    if !state.add_upload(&id, upload.clone(), MAX_UPLOADS) {
        let _ = blocking(&state, move |root| {
            root.upload_abort(&upload.dir, &upload.temp)
        })
        .await;
        return Err(ApiErr::new(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_uploads",
            "Too many uploads are in progress. Try again when one has finished.",
        ));
    }
    // A file of 0 bytes has no chunk to send. It is complete now.
    if upload.size == 0 {
        return finish(&state, &actor, &id, upload).await.map(Json);
    }
    Ok(Json(upload_status(&id, &upload, 0, None)))
}

async fn finish(
    state: &AppState,
    actor: &Actor,
    id: &str,
    upload: Upload,
) -> Result<UploadStatus, ApiErr> {
    state.remove_upload(id);
    let done = upload.clone();
    let finished = blocking(state, move |root| {
        let finished =
            root.upload_finish(&done.dir, &done.temp, &done.name, done.size, done.conflict);
        if finished.is_err() {
            let _ = root.upload_abort(&done.dir, &done.temp);
        }
        finished
    })
    .await;
    // An upload is recorded once, when the file is complete.
    let target = format!("{}/{}", upload.dir.trim_end_matches('/'), upload.name);
    let path = audited(state, actor, "fs.upload", &target, finished)?;
    Ok(upload_status(id, &upload, upload.size, Some(path)))
}

#[derive(Debug, Deserialize)]
pub struct ChunkQuery {
    offset: u64,
}

/// `PATCH /api/v1/fs/uploads/{id}`
pub async fn upload_chunk(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    query: Result<Query<ChunkQuery>, QueryRejection>,
    body: Bytes,
) -> Result<Response, ApiErr> {
    let Query(q) = query?;
    let upload = state.touch_upload(&id).ok_or_else(no_such_upload)?;
    let appending = upload.appending.clone();
    let _one_chunk_at_a_time = appending.lock().await;
    let append = upload.clone();
    let appended = blocking(&state, move |root| {
        root.upload_append(&append.dir, &append.temp, q.offset, &body, append.size)
    })
    .await;
    let offset = match appended {
        Ok(offset) => offset,
        Err(e) if e.code() == "offset_mismatch" => {
            // The client sent a chunk twice or lost track. Tell it where the
            // file ends, so it can continue from there.
            let held = upload.clone();
            let offset =
                blocking(&state, move |root| root.upload_len(&held.dir, &held.temp)).await?;
            let mut res = e.into_response();
            res.headers_mut()
                .insert("upload-offset", HeaderValue::from(offset));
            return Ok(res);
        }
        Err(e) => return Err(e),
    };
    let status = if offset == upload.size {
        finish(&state, &actor, &id, upload).await?
    } else {
        upload_status(&id, &upload, offset, None)
    };
    Ok(Json(status).into_response())
}

/// `GET /api/v1/fs/uploads/{id}`
pub async fn upload_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Reply<UploadStatus> {
    let upload = state.touch_upload(&id).ok_or_else(no_such_upload)?;
    let held = upload.clone();
    let offset = blocking(&state, move |root| root.upload_len(&held.dir, &held.temp)).await?;
    Ok(Json(upload_status(&id, &upload, offset, None)))
}

/// `DELETE /api/v1/fs/uploads/{id}`
pub async fn upload_cancel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    if let Some(upload) = state.remove_upload(&id) {
        blocking(&state, move |root| {
            root.upload_abort(&upload.dir, &upload.temp)
        })
        .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

impl Upload {
    pub(crate) fn idle_since(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_used)
    }

    pub(crate) fn used(&mut self, now: Instant) {
        self.last_used = now;
    }
}
