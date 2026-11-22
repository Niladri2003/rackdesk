//! Integration tests for the routes that change files: access, path
//! escapes, the save check, and the upload edge cases.

mod common;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, Response, StatusCode};
use common::*;
use rd_api::{PathResult, SaveResult, TrashItem, UploadStatus};
use serde_json::{json, Value};
use std::fs;
use std::net::SocketAddr;
use tower::ServiceExt;

async fn setup() -> (Server, String) {
    let server = server().await;
    let root = server.files_root();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join("docs/guide.md"), "guide").unwrap();
    fs::write(root.join("notes.txt"), "notes").unwrap();
    fs::write(server.dir.path().join("outside.txt"), "outside").unwrap();
    let cookie = active_session(&server).await;
    (server, cookie)
}

async fn post(server: &Server, cookie: &str, path: &str, body: Value) -> Response<Body> {
    call(Method::POST, path)
        .cookie(cookie)
        .body(body)
        .send(server)
        .await
}

/// A request with a raw body: a file save or an upload chunk.
async fn raw(
    server: &Server,
    cookie: &str,
    method: Method,
    path: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> Response<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("sec-fetch-site", "same-origin")
        .header(header::COOKIE, cookie);
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let mut req = req.body(Body::from(body)).unwrap();
    let peer: SocketAddr = "10.0.0.1:40000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    server.app.clone().oneshot(req).await.unwrap()
}

/// Every route that changes files, with a body that would succeed.
fn write_routes() -> Vec<(Method, &'static str, Value)> {
    vec![
        (Method::POST, "/api/v1/fs/mkdir", json!({"path": "/made"})),
        (
            Method::POST,
            "/api/v1/fs/rename",
            json!({"from": "/notes.txt", "to": "/moved.txt"}),
        ),
        (
            Method::POST,
            "/api/v1/fs/copy",
            json!({"from": "/notes.txt", "to": "/copy.txt"}),
        ),
        (
            Method::POST,
            "/api/v1/fs/delete",
            json!({"path": "/notes.txt"}),
        ),
        (
            Method::POST,
            "/api/v1/fs/trash",
            json!({"path": "/notes.txt"}),
        ),
        (
            Method::POST,
            "/api/v1/fs/trash/restore",
            json!({"id": "notes.txt"}),
        ),
        (
            Method::POST,
            "/api/v1/fs/trash/delete",
            json!({"id": "notes.txt"}),
        ),
        (Method::POST, "/api/v1/fs/trash/empty", json!({})),
        (
            Method::POST,
            "/api/v1/fs/chmod",
            json!({"path": "/notes.txt", "mode": 0o600}),
        ),
        (
            Method::POST,
            "/api/v1/fs/chown",
            json!({"path": "/notes.txt", "group": "0"}),
        ),
        (
            Method::POST,
            "/api/v1/fs/compress",
            json!({"from": "/docs", "to": "/docs.zip"}),
        ),
        (
            Method::POST,
            "/api/v1/fs/extract",
            json!({"from": "/docs.zip", "to": "/out"}),
        ),
        (
            Method::POST,
            "/api/v1/fs/uploads",
            json!({"dir": "/", "name": "up.bin", "size": 4}),
        ),
        (Method::PUT, "/api/v1/fs/file?path=/new.txt", json!("text")),
        (
            Method::PATCH,
            "/api/v1/fs/uploads/abc?offset=0",
            json!("data"),
        ),
        (Method::DELETE, "/api/v1/fs/uploads/abc", json!({})),
    ]
}

/// The names in the files root and in the folder that holds it.
fn snapshot(server: &Server) -> (Vec<String>, Vec<String>, String) {
    let names = |dir: &std::path::Path| {
        let mut v: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        v.sort();
        v
    };
    (
        names(&server.files_root()),
        names(server.dir.path()),
        fs::read_to_string(server.dir.path().join("outside.txt")).unwrap(),
    )
}

#[tokio::test]
async fn write_routes_need_a_session_and_refuse_other_sites() {
    let (server, cookie) = setup().await;
    let before = snapshot(&server);
    for (method, path, body) in write_routes() {
        let no_session = call(method.clone(), path)
            .body(body.clone())
            .send(&server)
            .await;
        assert_eq!(
            no_session.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );

        let cross_site = call(method.clone(), path)
            .cookie(&cookie)
            .body(body)
            .sec_fetch_site(Some("cross-site"))
            .send(&server)
            .await;
        assert_eq!(
            cross_site.status(),
            StatusCode::FORBIDDEN,
            "{method} {path}"
        );
        assert_eq!(
            error_code(cross_site).await,
            "csrf_rejected",
            "{method} {path}"
        );
    }
    assert_eq!(
        snapshot(&server),
        before,
        "a refused request changed a file"
    );
}

#[tokio::test]
async fn write_routes_are_refused_until_the_initial_password_is_changed() {
    let server = server().await;
    fs::write(server.files_root().join("notes.txt"), "notes").unwrap();
    let cookie = sign_in(&server, &server.password).await;
    for (method, path, body) in write_routes() {
        let res = call(method.clone(), path)
            .cookie(&cookie)
            .body(body)
            .send(&server)
            .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "{method} {path}");
        assert_eq!(
            error_code(res).await,
            "password_change_required",
            "{method} {path}"
        );
    }
    assert_eq!(
        fs::read_to_string(server.files_root().join("notes.txt")).unwrap(),
        "notes"
    );
}

#[tokio::test]
async fn paths_that_leave_the_root_are_refused_and_change_nothing() {
    let (server, cookie) = setup().await;
    let before = snapshot(&server);
    let cases = [
        ("/api/v1/fs/mkdir", json!({"path": "/../made"})),
        (
            "/api/v1/fs/rename",
            json!({"from": "/notes.txt", "to": "/../moved.txt"}),
        ),
        (
            "/api/v1/fs/rename",
            json!({"from": "/../outside.txt", "to": "/in.txt"}),
        ),
        (
            "/api/v1/fs/copy",
            json!({"from": "/../outside.txt", "to": "/in.txt"}),
        ),
        (
            "/api/v1/fs/copy",
            json!({"from": "/notes.txt", "to": "/docs/../../out.txt"}),
        ),
        ("/api/v1/fs/delete", json!({"path": "/../outside.txt"})),
        ("/api/v1/fs/trash", json!({"path": "/../outside.txt"})),
        (
            "/api/v1/fs/trash/restore",
            json!({"id": "../../outside.txt"}),
        ),
        ("/api/v1/fs/trash/delete", json!({"id": "../info"})),
        (
            "/api/v1/fs/compress",
            json!({"from": "/docs", "to": "/../docs.zip"}),
        ),
        (
            "/api/v1/fs/compress",
            json!({"from": "/..", "to": "/all.zip"}),
        ),
        (
            "/api/v1/fs/chmod",
            json!({"path": "/../outside.txt", "mode": 0o777}),
        ),
        (
            "/api/v1/fs/chown",
            json!({"path": "/../outside.txt", "group": "0"}),
        ),
        (
            "/api/v1/fs/extract",
            json!({"from": "/../x.zip", "to": "/out"}),
        ),
        (
            "/api/v1/fs/uploads",
            json!({"dir": "/..", "name": "up.bin", "size": 1}),
        ),
        (
            "/api/v1/fs/uploads",
            json!({"dir": "/", "name": "../up.bin", "size": 1}),
        ),
        (
            "/api/v1/fs/uploads",
            json!({"dir": "/", "name": "docs/up.bin", "size": 1}),
        ),
    ];
    for (path, body) in cases {
        let res = post(&server, &cookie, path, body.clone()).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{path} {body}");
        assert_eq!(error_code(res).await, "invalid_path", "{path} {body}");
    }
    let res = raw(
        &server,
        &cookie,
        Method::PUT,
        "/api/v1/fs/file?path=/../made.txt",
        &[],
        b"x".to_vec(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(snapshot(&server), before);
}

#[tokio::test]
async fn a_save_is_refused_when_the_file_changed_since_it_was_read() {
    let (server, cookie) = setup().await;
    let file = server.files_root().join("notes.txt");
    let read = call(Method::GET, "/api/v1/fs/download?path=/notes.txt")
        .cookie(&cookie)
        .send(&server)
        .await;
    let etag = read.headers()[header::ETAG].to_str().unwrap().to_owned();

    let saved = raw(
        &server,
        &cookie,
        Method::PUT,
        "/api/v1/fs/file?path=/notes.txt",
        &[("if-match", &etag)],
        b"mine".to_vec(),
    )
    .await;
    assert_eq!(saved.status(), StatusCode::OK);
    let new_etag = json_body::<SaveResult>(saved).await.etag;
    assert_eq!(fs::read_to_string(&file).unwrap(), "mine");

    // The tag from before the save is old now.
    let stale = raw(
        &server,
        &cookie,
        Method::PUT,
        "/api/v1/fs/file?path=/notes.txt",
        &[("if-match", &etag)],
        b"lost".to_vec(),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(error_code(stale).await, "changed_on_disk");
    assert_eq!(fs::read_to_string(&file).unwrap(), "mine");

    // With no tag, an existing file is not replaced.
    let blind = raw(
        &server,
        &cookie,
        Method::PUT,
        "/api/v1/fs/file?path=/notes.txt",
        &[],
        b"lost".to_vec(),
    )
    .await;
    assert_eq!(blind.status(), StatusCode::CONFLICT);
    assert_eq!(fs::read_to_string(&file).unwrap(), "mine");

    let again = raw(
        &server,
        &cookie,
        Method::PUT,
        "/api/v1/fs/file?path=/notes.txt",
        &[("if-match", &new_etag)],
        b"next".to_vec(),
    )
    .await;
    assert_eq!(again.status(), StatusCode::OK);
    let forced = raw(
        &server,
        &cookie,
        Method::PUT,
        "/api/v1/fs/file?path=/notes.txt&force=true",
        &[],
        b"forced".to_vec(),
    )
    .await;
    assert_eq!(forced.status(), StatusCode::OK);
    assert_eq!(fs::read_to_string(&file).unwrap(), "forced");
}

#[tokio::test]
async fn request_bodies_have_size_limits() {
    let (server, cookie) = setup().await;
    let big_save = vec![b'x'; rackdeskd::files_write::MAX_SAVE_BYTES + 1];
    let res = raw(
        &server,
        &cookie,
        Method::PUT,
        "/api/v1/fs/file?path=/big.txt",
        &[],
        big_save,
    )
    .await;
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!server.files_root().join("big.txt").exists());

    let begin = post(
        &server,
        &cookie,
        "/api/v1/fs/uploads",
        json!({"dir": "/", "name": "big.bin", "size": 1 << 30}),
    )
    .await;
    let id = json_body::<UploadStatus>(begin).await.id;
    let big_chunk = vec![0u8; rackdeskd::files_write::MAX_CHUNK_BYTES + 1];
    let res = raw(
        &server,
        &cookie,
        Method::PATCH,
        &format!("/api/v1/fs/uploads/{id}?offset=0"),
        &[],
        big_chunk,
    )
    .await;
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn an_upload_resumes_after_a_repeated_chunk_and_ends_as_one_file() {
    let (server, cookie) = setup().await;
    let begin = post(
        &server,
        &cookie,
        "/api/v1/fs/uploads",
        json!({"dir": "/docs", "name": "hello.txt", "size": 11}),
    )
    .await;
    assert_eq!(begin.status(), StatusCode::OK);
    let id = json_body::<UploadStatus>(begin).await.id;
    let chunk = |offset: u64| format!("/api/v1/fs/uploads/{id}?offset={offset}");

    let first = raw(
        &server,
        &cookie,
        Method::PATCH,
        &chunk(0),
        &[],
        b"hello ".to_vec(),
    )
    .await;
    assert_eq!(json_body::<UploadStatus>(first).await.offset, 6);
    // Nothing shows under the final name until the upload is complete.
    assert!(!server.files_root().join("docs/hello.txt").exists());

    // The same chunk again: refused, with the place to continue from.
    let repeat = raw(
        &server,
        &cookie,
        Method::PATCH,
        &chunk(0),
        &[],
        b"hello ".to_vec(),
    )
    .await;
    assert_eq!(repeat.status(), StatusCode::CONFLICT);
    assert_eq!(repeat.headers()["upload-offset"], "6");
    assert_eq!(error_code(repeat).await, "offset_mismatch");

    // After a lost connection the client asks where to continue.
    let status = call(Method::GET, &format!("/api/v1/fs/uploads/{id}"))
        .cookie(&cookie)
        .send(&server)
        .await;
    assert_eq!(json_body::<UploadStatus>(status).await.offset, 6);

    let last = raw(
        &server,
        &cookie,
        Method::PATCH,
        &chunk(6),
        &[],
        b"world".to_vec(),
    )
    .await;
    let done: UploadStatus = json_body(last).await;
    assert_eq!(done.path.as_deref(), Some("/docs/hello.txt"));
    assert_eq!(
        fs::read_to_string(server.files_root().join("docs/hello.txt")).unwrap(),
        "hello world"
    );

    // No temporary file is left, and the id is gone.
    let left: Vec<_> = fs::read_dir(server.files_root().join("docs"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left.len(), 2, "{left:?}");
    let gone = raw(
        &server,
        &cookie,
        Method::PATCH,
        &chunk(11),
        &[],
        b"x".to_vec(),
    )
    .await;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_cancelled_upload_leaves_no_file() {
    let (server, cookie) = setup().await;
    let begin = post(
        &server,
        &cookie,
        "/api/v1/fs/uploads",
        json!({"dir": "/", "name": "half.bin", "size": 100}),
    )
    .await;
    let id = json_body::<UploadStatus>(begin).await.id;
    raw(
        &server,
        &cookie,
        Method::PATCH,
        &format!("/api/v1/fs/uploads/{id}?offset=0"),
        &[],
        vec![1; 40],
    )
    .await;

    let cancel = call(Method::DELETE, &format!("/api/v1/fs/uploads/{id}"))
        .cookie(&cookie)
        .send(&server)
        .await;
    assert_eq!(cancel.status(), StatusCode::NO_CONTENT);
    let names = snapshot(&server).0;
    assert_eq!(
        names,
        ["docs", "notes.txt"],
        "the temporary file was not removed"
    );
}

#[tokio::test]
async fn an_upload_is_refused_when_it_is_too_large_or_the_disk_is_too_full() {
    let (server, cookie) = setup().await;
    // More than any disk has free, but under the size limit of this test server.
    let res = post(
        &server,
        &cookie,
        "/api/v1/fs/uploads",
        json!({"dir": "/", "name": "huge.bin", "size": 1u64 << 60}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(error_code(res).await, "no_space");

    let res = post(
        &server,
        &cookie,
        "/api/v1/fs/uploads",
        json!({"dir": "/", "name": "huge.bin", "size": u64::MAX / 2}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error_code(res).await, "too_large");

    // An existing name is refused at the start, not after the bytes are sent.
    let res = post(
        &server,
        &cookie,
        "/api/v1/fs/uploads",
        json!({"dir": "/", "name": "notes.txt", "size": 5}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(snapshot(&server).0, ["docs", "notes.txt"]);
}

#[tokio::test]
async fn trash_then_restore_through_the_api() {
    let (server, cookie) = setup().await;
    let res = post(
        &server,
        &cookie,
        "/api/v1/fs/trash",
        json!({"path": "/docs/guide.md"}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert!(!server.files_root().join("docs/guide.md").exists());

    let list = call(Method::GET, "/api/v1/fs/trash")
        .cookie(&cookie)
        .send(&server)
        .await;
    let items: Vec<TrashItem> = json_body(list).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].original_path, "/docs/guide.md");

    let res = post(
        &server,
        &cookie,
        "/api/v1/fs/trash/restore",
        json!({"id": items[0].id}),
    )
    .await;
    assert_eq!(json_body::<PathResult>(res).await.path, "/docs/guide.md");
    assert_eq!(
        fs::read_to_string(server.files_root().join("docs/guide.md")).unwrap(),
        "guide"
    );
}

#[tokio::test]
async fn changes_and_sign_ins_are_written_to_the_audit_log() {
    let (server, cookie) = setup().await;
    post(
        &server,
        &cookie,
        "/api/v1/fs/rename",
        json!({"from": "/notes.txt", "to": "/n.txt"}),
    )
    .await;
    post(
        &server,
        &cookie,
        "/api/v1/fs/delete",
        json!({"path": "/../outside.txt"}),
    )
    .await;
    sign_in_call("a wrong password").send(&server).await;

    let path = server.dir.path().join("audit.log");
    let text = fs::read_to_string(&path).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let find = |action: &str, result: &str| {
        lines
            .iter()
            .find(|l| l["action"] == action && l["result"] == result)
            .unwrap_or_else(|| panic!("no line for {action} {result} in:\n{text}"))
    };
    let rename = find("fs.rename", "ok");
    assert_eq!(rename["target"], "/notes.txt -> /n.txt");
    assert_eq!(rename["user"], "admin");
    assert_eq!(rename["ip"], "10.0.0.1");
    // A refused change is recorded too, with its error code.
    assert_eq!(
        find("fs.delete", "invalid_path")["target"],
        "/../outside.txt"
    );
    find("session.sign_in", "ok");
    find("session.sign_in", "invalid_credentials");
    find("session.change_password", "ok");

    // The view route gives the same lines, newest first, to a session only.
    let res = call(Method::GET, "/api/v1/audit")
        .cookie(&cookie)
        .send(&server)
        .await;
    let entries: Vec<rd_api::AuditEntry> = json_body(res).await;
    assert_eq!(entries.len(), lines.len());
    assert_eq!(entries[0].action, "session.sign_in");
    assert_eq!(entries[0].result, "invalid_credentials");
    let res = call(Method::GET, "/api/v1/audit").send(&server).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // No password in any form, and only the owner may read the file.
    assert!(!text.contains("a wrong password"));
    assert!(!text.contains(&server.password));
    assert!(!text.contains(NEW_PASSWORD));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn chmod_sets_plain_bits_and_root_actions_need_the_root_program() {
    use std::os::unix::fs::PermissionsExt;
    let (server, cookie) = setup().await;
    let file = server.files_root().join("notes.txt");
    let mode = || fs::metadata(&file).unwrap().permissions().mode() & 0o7777;

    let res = post(
        &server,
        &cookie,
        "/api/v1/fs/chmod",
        json!({"path": "/notes.txt", "mode": 0o640}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert_eq!(mode(), 0o640);

    // The set-uid bit is refused.
    let res = post(
        &server,
        &cookie,
        "/api/v1/fs/chmod",
        json!({"path": "/notes.txt", "mode": 0o4755}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(mode(), 0o640);

    // This test server has no root program. Nothing is done as root.
    let res = post(
        &server,
        &cookie,
        "/api/v1/fs/chmod",
        json!({"path": "/notes.txt", "mode": 0o600, "elevate": true}),
    )
    .await;
    assert_eq!(error_code(res).await, "no_root_program");
    let res = call(
        Method::GET,
        "/api/v1/fs/download?path=/notes.txt&elevate=true",
    )
    .cookie(&cookie)
    .send(&server)
    .await;
    assert_eq!(error_code(res).await, "no_root_program");
    assert_eq!(mode(), 0o640);
}
