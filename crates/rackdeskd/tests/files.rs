//! Integration tests for the file browser API: sign-in is required, paths
//! can't leave the files root, and file content is served safely.

mod common;

use axum::http::{header, Method, StatusCode};
use common::*;
use rd_api::{DirEntry, DirListing, EntryKind, Places, SearchResults};
use std::fs;

/// A server whose files root holds a small tree, and a session that may use
/// every route.
async fn setup() -> (Server, String) {
    let server = server().await;
    let root = server.files_root();
    fs::create_dir_all(root.join("docs/deep")).unwrap();
    fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
    fs::write(root.join("docs/deep/compose.yml"), "services: {}\n").unwrap();
    fs::write(root.join("notes.txt"), "0123456789").unwrap();
    fs::write(root.join(".hidden"), "x").unwrap();
    fs::write(root.join("photo.png"), b"\x89PNG fake").unwrap();
    fs::write(root.join("page.html"), "<script>alert(1)</script>").unwrap();
    fs::write(root.join("drawing.svg"), "<svg onload=alert(1)>").unwrap();
    // A file outside the root, next to it.
    fs::write(server.dir.path().join("outside.txt"), "outside").unwrap();
    let cookie = active_session(&server).await;
    (server, cookie)
}

async fn get(server: &Server, cookie: &str, path: &str) -> axum::http::Response<axum::body::Body> {
    call(Method::GET, path).cookie(cookie).send(server).await
}

fn names(listing: &DirListing) -> Vec<&str> {
    listing.entries.iter().map(|e| e.name.as_str()).collect()
}

const ROUTES: [&str; 7] = [
    "/api/v1/fs/properties?path=/notes.txt",
    "/api/v1/fs/zip?path=/docs",
    "/api/v1/fs/list?path=/",
    "/api/v1/fs/entry?path=/notes.txt",
    "/api/v1/fs/places",
    "/api/v1/fs/search?q=notes",
    "/api/v1/fs/download?path=/notes.txt",
];

// ---------- Access ----------

#[tokio::test]
async fn every_file_route_needs_a_session() {
    let (server, _cookie) = setup().await;
    for route in ROUTES {
        let res = call(Method::GET, route).send(&server).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{route}");
        assert_eq!(error_code(res).await, "unauthorized", "{route}");
    }
}

#[tokio::test]
async fn file_routes_are_refused_until_the_initial_password_is_changed() {
    let server = server().await;
    fs::write(server.files_root().join("notes.txt"), "x").unwrap();
    let cookie = sign_in(&server, &server.password).await;
    for route in ROUTES {
        let res = get(&server, &cookie, route).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "{route}");
        assert_eq!(error_code(res).await, "password_change_required", "{route}");
    }
}

// ---------- Paths ----------

#[tokio::test]
async fn paths_with_dot_dot_are_refused_on_every_route() {
    let (server, cookie) = setup().await;
    for route in [
        "/api/v1/fs/list?path=/..",
        "/api/v1/fs/list?path=/docs/../..",
        "/api/v1/fs/list?path=%2E%2E",
        "/api/v1/fs/entry?path=/../outside.txt",
        "/api/v1/fs/search?q=outside&path=/..",
        "/api/v1/fs/download?path=/../outside.txt",
        "/api/v1/fs/download?path=/docs/../../outside.txt",
        "/api/v1/fs/download?path=..%2Foutside.txt",
        "/api/v1/fs/zip?path=/..",
        "/api/v1/fs/properties?path=/../outside.txt",
        "/api/v1/fs/zip?path=/docs/../..",
    ] {
        let res = get(&server, &cookie, route).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{route}");
        assert_eq!(error_code(res).await, "invalid_path", "{route}");
    }
}

#[tokio::test]
async fn an_absolute_server_path_stays_beneath_the_root() {
    let (server, cookie) = setup().await;
    // "/etc/passwd" means "<files root>/etc/passwd", which does not exist.
    let res = get(&server, &cookie, "/api/v1/fs/download?path=/etc/passwd").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let outside = server.dir.path().join("outside.txt");
    let route = format!("/api/v1/fs/download?path={}", outside.display());
    let res = get(&server, &cookie, &route).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[cfg(unix)]
#[tokio::test]
async fn symlinks_out_of_the_root_are_not_followed() {
    let (server, cookie) = setup().await;
    let root = server.files_root();
    std::os::unix::fs::symlink(server.dir.path(), root.join("escape")).unwrap();
    std::os::unix::fs::symlink(server.dir.path().join("outside.txt"), root.join("link.txt"))
        .unwrap();
    std::os::unix::fs::symlink("/", root.join("slash")).unwrap();

    for route in [
        "/api/v1/fs/list?path=/escape",
        "/api/v1/fs/download?path=/escape/outside.txt",
        "/api/v1/fs/download?path=/escape/credentials.json",
        "/api/v1/fs/download?path=/link.txt",
        "/api/v1/fs/download?path=/slash/etc/hosts",
        "/api/v1/fs/list?path=/slash/etc",
    ] {
        let res = get(&server, &cookie, route).await;
        assert!(
            res.status().is_client_error() || res.status().is_server_error(),
            "{route}"
        );
        let body = String::from_utf8_lossy(&bytes_body(res).await).into_owned();
        assert!(!body.contains("outside"), "{route} leaked file content");
        assert!(!body.contains("argon2"), "{route} leaked the credentials");
    }

    // The links are listed, as links with no target kind.
    let listing: DirListing =
        json_body(get(&server, &cookie, "/api/v1/fs/list?path=/&hidden=true").await).await;
    let link = listing.entries.iter().find(|e| e.name == "escape").unwrap();
    assert_eq!((link.kind, link.target), (EntryKind::Symlink, None));
}

#[tokio::test]
async fn error_messages_do_not_show_server_paths() {
    let (server, cookie) = setup().await;
    let root = server.files_root().display().to_string();
    for route in [
        "/api/v1/fs/list?path=/nope",
        "/api/v1/fs/download?path=/nope.txt",
        "/api/v1/fs/list?path=/notes.txt",
        "/api/v1/fs/download?path=/docs",
    ] {
        let res = get(&server, &cookie, route).await;
        assert!(res.status().is_client_error(), "{route}");
        let body = String::from_utf8(bytes_body(res).await).unwrap();
        assert!(!body.contains(&root), "{route}: {body}");
    }
}

// ---------- Listing ----------

#[tokio::test]
async fn list_returns_folders_first_and_hides_dot_files_by_default() {
    let (server, cookie) = setup().await;
    let res = get(&server, &cookie, "/api/v1/fs/list?path=/").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
    let listing: DirListing = json_body(res).await;
    assert_eq!(
        names(&listing),
        ["docs", "drawing.svg", "notes.txt", "page.html", "photo.png"]
    );
    assert_eq!((listing.total, listing.path.as_str()), (5, "/"));

    let all: DirListing =
        json_body(get(&server, &cookie, "/api/v1/fs/list?path=/&hidden=true").await).await;
    assert_eq!(all.total, 6);
    assert_eq!(all.entries[1].name, ".hidden");
}

#[tokio::test]
async fn list_pages_sorts_and_filters() {
    let (server, cookie) = setup().await;
    let page: DirListing =
        json_body(get(&server, &cookie, "/api/v1/fs/list?path=/&offset=1&limit=2").await).await;
    assert_eq!(names(&page), ["drawing.svg", "notes.txt"]);
    assert_eq!((page.total, page.offset), (5, 1));

    let by_size: DirListing = json_body(
        get(
            &server,
            &cookie,
            "/api/v1/fs/list?path=/&sort=size&desc=true",
        )
        .await,
    )
    .await;
    assert_eq!(names(&by_size)[..2], ["docs", "page.html"]);

    let filtered: DirListing =
        json_body(get(&server, &cookie, "/api/v1/fs/list?path=/&q=P").await).await;
    assert_eq!(names(&filtered), ["page.html", "photo.png"]);
    assert_eq!(filtered.total, 2);
}

#[tokio::test]
async fn bad_query_values_get_a_json_error() {
    let (server, cookie) = setup().await;
    let long = "x".repeat(101);
    for route in [
        "/api/v1/fs/list?path=/&sort=colour".to_owned(),
        "/api/v1/fs/list?path=/&offset=-1".to_owned(),
        "/api/v1/fs/list?path=/&limit=many".to_owned(),
        "/api/v1/fs/entry".to_owned(),
        "/api/v1/fs/search".to_owned(),
        format!("/api/v1/fs/search?q={long}"),
        format!("/api/v1/fs/list?path=/&q={long}"),
    ] {
        let res = get(&server, &cookie, &route).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{route}");
        assert_eq!(error_code(res).await, "bad_request", "{route}");
    }
}

#[tokio::test]
async fn a_page_is_never_larger_than_the_limit() {
    let (server, cookie) = setup().await;
    let big = server.files_root().join("big");
    fs::create_dir(&big).unwrap();
    for i in 0..(rd_fs::MAX_PAGE + 10) {
        fs::write(big.join(format!("f{i}")), "").unwrap();
    }
    let listing: DirListing = json_body(
        get(
            &server,
            &cookie,
            "/api/v1/fs/list?path=/big&limit=4000000000",
        )
        .await,
    )
    .await;
    assert_eq!(listing.entries.len() as u32, rd_fs::MAX_PAGE);
    assert_eq!(listing.total, rd_fs::MAX_PAGE + 10);
}

#[tokio::test]
async fn entry_describes_one_file() {
    let (server, cookie) = setup().await;
    let e: DirEntry =
        json_body(get(&server, &cookie, "/api/v1/fs/entry?path=/notes.txt").await).await;
    assert_eq!(
        (e.name.as_str(), e.kind, e.size),
        ("notes.txt", EntryKind::File, 10)
    );
    let res = get(&server, &cookie, "/api/v1/fs/entry?path=/nope").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(error_code(res).await, "not_found");
}

#[tokio::test]
async fn places_reports_the_root_disk() {
    let (server, cookie) = setup().await;
    let places: Places = json_body(get(&server, &cookie, "/api/v1/fs/places").await).await;
    assert_eq!(places.disks[0].path, "/");
    assert!(places.disks[0].total > 0);
    assert!(places.disks[0].free <= places.disks[0].total);
    // The home folder of the test user is outside the temporary files root.
    assert_eq!(places.home, None);
}

#[tokio::test]
async fn search_finds_names_in_folders_below() {
    let (server, cookie) = setup().await;
    let found: SearchResults =
        json_body(get(&server, &cookie, "/api/v1/fs/search?q=COMPOSE").await).await;
    assert_eq!(found.hits.len(), 1);
    assert_eq!(
        (
            found.hits[0].dir.as_str(),
            found.hits[0].entry.name.as_str()
        ),
        ("/docs/deep", "compose.yml")
    );
    assert!(!found.truncated);

    let scoped: SearchResults =
        json_body(get(&server, &cookie, "/api/v1/fs/search?q=notes&path=/docs").await).await;
    assert!(scoped.hits.is_empty());
}

// ---------- Download ----------

#[tokio::test]
async fn download_sends_the_file_as_an_attachment() {
    let (server, cookie) = setup().await;
    let res = get(&server, &cookie, "/api/v1/fs/download?path=/notes.txt").await;
    assert_eq!(res.status(), StatusCode::OK);
    let h = res.headers();
    assert_eq!(h[header::CONTENT_TYPE], "application/octet-stream");
    assert_eq!(h[header::CONTENT_LENGTH], "10");
    assert_eq!(h[header::ACCEPT_RANGES], "bytes");
    assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(
        h[header::CONTENT_SECURITY_POLICY],
        "default-src 'none'; sandbox"
    );
    assert!(h[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("attachment; filename=\"notes.txt\""));
    assert!(h.contains_key(header::ETAG));
    assert_eq!(bytes_body(res).await, b"0123456789");
}

#[tokio::test]
async fn files_that_can_run_script_are_never_shown_inline() {
    let (server, cookie) = setup().await;
    for name in ["page.html", "drawing.svg", "notes.txt", "docs/guide.md"] {
        let route = format!("/api/v1/fs/download?path=/{name}&inline=true");
        let res = get(&server, &cookie, &route).await;
        assert_eq!(res.status(), StatusCode::OK, "{name}");
        let h = res.headers();
        assert_eq!(
            h[header::CONTENT_TYPE],
            "application/octet-stream",
            "{name}"
        );
        assert!(
            h[header::CONTENT_DISPOSITION]
                .to_str()
                .unwrap()
                .starts_with("attachment"),
            "{name}"
        );
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff", "{name}");
        assert_eq!(
            h[header::CONTENT_SECURITY_POLICY],
            "default-src 'none'; sandbox",
            "{name}"
        );
    }
}

#[tokio::test]
async fn an_image_is_shown_inline_only_on_request() {
    let (server, cookie) = setup().await;
    let res = get(
        &server,
        &cookie,
        "/api/v1/fs/download?path=/photo.png&inline=true",
    )
    .await;
    let h = res.headers();
    assert_eq!(h[header::CONTENT_TYPE], "image/png");
    assert!(h[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("inline"));
    assert_eq!(
        h[header::CONTENT_SECURITY_POLICY],
        "default-src 'none'; sandbox"
    );

    let res = get(&server, &cookie, "/api/v1/fs/download?path=/photo.png").await;
    assert_eq!(
        res.headers()[header::CONTENT_TYPE],
        "application/octet-stream"
    );
    assert!(res.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("attachment"));
}

#[tokio::test]
async fn a_range_request_gets_that_part_of_the_file() {
    let (server, cookie) = setup().await;
    let cases = [
        ("bytes=2-5", "bytes 2-5/10", "2345"),
        ("bytes=7-", "bytes 7-9/10", "789"),
        ("bytes=-3", "bytes 7-9/10", "789"),
        ("bytes=8-100", "bytes 8-9/10", "89"),
    ];
    for (range, content_range, body) in cases {
        let res = call(Method::GET, "/api/v1/fs/download?path=/notes.txt")
            .cookie(&cookie)
            .header("range", range)
            .send(&server)
            .await;
        assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT, "{range}");
        assert_eq!(
            res.headers()[header::CONTENT_RANGE],
            content_range,
            "{range}"
        );
        assert_eq!(
            res.headers()[header::CONTENT_LENGTH],
            body.len().to_string().as_str(),
            "{range}"
        );
        assert_eq!(bytes_body(res).await, body.as_bytes(), "{range}");
    }
}

#[tokio::test]
async fn a_range_after_the_end_is_refused() {
    let (server, cookie) = setup().await;
    let res = call(Method::GET, "/api/v1/fs/download?path=/notes.txt")
        .cookie(&cookie)
        .header("range", "bytes=10-")
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(res.headers()[header::CONTENT_RANGE], "bytes */10");
}

#[tokio::test]
async fn if_range_with_an_old_tag_gets_the_whole_file() {
    let (server, cookie) = setup().await;
    let res = call(Method::GET, "/api/v1/fs/download?path=/notes.txt")
        .cookie(&cookie)
        .header("range", "bytes=2-5")
        .header("if-range", "\"old-tag\"")
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(bytes_body(res).await, b"0123456789");
}

#[tokio::test]
async fn a_folder_can_not_be_downloaded_as_a_file() {
    let (server, cookie) = setup().await;
    let res = get(&server, &cookie, "/api/v1/fs/download?path=/docs").await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(error_code(res).await, "not_a_file");
}

#[tokio::test]
async fn the_file_name_in_the_header_is_encoded() {
    let (server, cookie) = setup().await;
    fs::write(server.files_root().join("caf\u{e9} \"menu\".txt"), "x").unwrap();
    let res = get(
        &server,
        &cookie,
        "/api/v1/fs/download?path=/caf%C3%A9%20%22menu%22.txt",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"caf_ _menu_.txt\"; filename*=UTF-8''caf%C3%A9%20%22menu%22.txt"
    );
}

#[tokio::test]
async fn a_folder_downloads_as_a_zip_made_on_the_fly() {
    use std::io::Read;
    let (server, cookie) = setup().await;
    let res = get(&server, &cookie, "/api/v1/fs/zip?path=/docs").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "application/zip");
    assert!(res.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("attachment; filename=\"docs.zip\""));

    let bytes = bytes_body(res).await;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name("docs/deep/compose.yml")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "services: {}\n");

    let missing = get(&server, &cookie, "/api/v1/fs/zip?path=/nope").await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(error_code(missing).await, "not_found");
}
