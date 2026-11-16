//! Integration tests for sign-in, sign-out, the forced password change,
//! lockout and CSRF rejection. They call the router in-process: no socket.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, Response, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use rackdeskd::{AppState, Config};
use rd_api::{ApiError, SessionInfo};
use serde_json::json;
use std::net::SocketAddr;
use tower::ServiceExt;

const NEW_PASSWORD: &str = "a new and unusual phrase";

struct Server {
    app: Router,
    state: AppState,
    /// The generated initial password.
    password: String,
    dir: tempfile::TempDir,
}

async fn server() -> Server {
    server_with(false, false).await
}

async fn server_with(https: bool, behind_proxy: bool) -> Server {
    let dir = tempfile::tempdir().unwrap();
    let store = rd_auth::CredentialStore::new(dir.path());
    let (creds, initial) = store.load_or_init().unwrap();
    let config = Config {
        data_dir: dir.path().to_owned(),
        https,
        behind_proxy,
    };
    let state = AppState::new(config, creds).await.unwrap();
    Server {
        app: rackdeskd::router(state.clone()),
        state,
        password: initial.unwrap().0,
        dir,
    }
}

/// A request as the RackDesk page sends it: same-origin, from 10.0.0.1.
struct Call {
    method: Method,
    path: &'static str,
    body: Option<serde_json::Value>,
    cookie: Option<String>,
    peer: &'static str,
    /// `None` leaves the header out.
    sec_fetch_site: Option<&'static str>,
    extra: Vec<(&'static str, &'static str)>,
}

fn call(method: Method, path: &'static str) -> Call {
    Call {
        method,
        path,
        body: None,
        cookie: None,
        peer: "10.0.0.1",
        sec_fetch_site: Some("same-origin"),
        extra: Vec::new(),
    }
}

impl Call {
    fn body(mut self, body: serde_json::Value) -> Self {
        self.body = Some(body);
        self
    }
    fn cookie(mut self, cookie: &str) -> Self {
        self.cookie = Some(cookie.to_owned());
        self
    }
    fn peer(mut self, peer: &'static str) -> Self {
        self.peer = peer;
        self
    }
    fn sec_fetch_site(mut self, value: Option<&'static str>) -> Self {
        self.sec_fetch_site = value;
        self
    }
    fn header(mut self, name: &'static str, value: &'static str) -> Self {
        self.extra.push((name, value));
        self
    }

    async fn send(self, server: &Server) -> Response<Body> {
        let mut req = Request::builder().method(self.method).uri(self.path);
        if let Some(site) = self.sec_fetch_site {
            req = req.header("sec-fetch-site", site);
        }
        if let Some(cookie) = &self.cookie {
            req = req.header(header::COOKIE, cookie);
        }
        for (name, value) in &self.extra {
            req = req.header(*name, *value);
        }
        let body = match &self.body {
            Some(json) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        let mut req = req.body(body).unwrap();
        let peer: SocketAddr = format!("{}:40000", self.peer).parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(peer));
        server.app.clone().oneshot(req).await.unwrap()
    }
}

async fn json_body<T: serde::de::DeserializeOwned>(res: Response<Body>) -> T {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "body is not the expected JSON ({e}): {}",
            String::from_utf8_lossy(&bytes)
        )
    })
}

async fn error_code(res: Response<Body>) -> String {
    json_body::<ApiError>(res).await.code
}

/// The full `Set-Cookie` value.
fn set_cookie(res: &Response<Body>) -> String {
    res.headers()
        .get(header::SET_COOKIE)
        .expect("a Set-Cookie header")
        .to_str()
        .unwrap()
        .to_owned()
}

/// `name=value`, as the browser sends it back.
fn cookie_pair(res: &Response<Body>) -> String {
    set_cookie(res).split(';').next().unwrap().to_owned()
}

fn sign_in_call(password: &str) -> Call {
    call(Method::POST, "/api/v1/session").body(json!({"username": "admin", "password": password}))
}

/// Signs in and returns the cookie pair.
async fn sign_in(server: &Server, password: &str) -> String {
    let res = sign_in_call(password).send(server).await;
    assert_eq!(res.status(), StatusCode::OK);
    cookie_pair(&res)
}

fn change_call(cookie: &str, current: &str, new: &str) -> Call {
    call(Method::POST, "/api/v1/session/password")
        .cookie(cookie)
        .body(json!({"current_password": current, "new_password": new}))
}

/// Signs in with the initial password, changes it to [`NEW_PASSWORD`], and
/// returns the cookie pair of the new session.
async fn sign_in_and_change(server: &Server) -> String {
    let cookie = sign_in(server, &server.password).await;
    let res = change_call(&cookie, &server.password, NEW_PASSWORD)
        .send(server)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    cookie_pair(&res)
}

async fn who_am_i(server: &Server, cookie: &str) -> StatusCode {
    call(Method::GET, "/api/v1/session")
        .cookie(cookie)
        .send(server)
        .await
        .status()
}

// ---------- Sign-in ----------

#[tokio::test]
async fn sign_in_returns_the_account_and_sets_a_cookie() {
    let server = server().await;
    let res = sign_in_call(&server.password).send(&server).await;
    assert_eq!(res.status(), StatusCode::OK);

    let cookie = set_cookie(&res);
    let (pair, attrs) = cookie.split_once(';').unwrap();
    let (name, id) = pair.split_once('=').unwrap();
    assert_eq!(name, "rd_session");
    assert_eq!(id.len(), 64);
    assert!(attrs.contains("HttpOnly"));
    assert!(attrs.contains("SameSite=Strict"));
    assert!(attrs.contains("Path=/"));
    assert!(attrs.contains("Max-Age=604800"));
    assert!(!attrs.contains("Secure"), "plain HTTP: no Secure attribute");

    let info: SessionInfo = json_body(res).await;
    assert_eq!(info.username, "admin");
    assert!(info.must_change_password);
}

#[tokio::test]
async fn on_https_the_cookie_is_secure_and_host_bound() {
    let server = server_with(true, false).await;
    let res = sign_in_call(&server.password).send(&server).await;
    let cookie = set_cookie(&res);
    assert!(cookie.starts_with("__Host-rd_session="));
    assert!(cookie.contains("; Secure"));
    assert!(!cookie.contains("Domain"));

    // The plain-HTTP cookie name is not accepted on HTTPS.
    let id = cookie_pair(&res);
    let id = id.split_once('=').unwrap().1;
    assert_eq!(
        who_am_i(&server, &format!("rd_session={id}")).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        who_am_i(&server, &format!("__Host-rd_session={id}")).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn wrong_password_and_unknown_user_get_the_same_answer() {
    let server = server().await;
    let wrong = sign_in_call("not the password").send(&server).await;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert!(wrong.headers().get(header::SET_COOKIE).is_none());
    let wrong: ApiError = json_body(wrong).await;

    let unknown = call(Method::POST, "/api/v1/session")
        .body(json!({"username": "root", "password": server.password}))
        .send(&server)
        .await;
    assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
    let unknown: ApiError = json_body(unknown).await;

    assert_eq!(wrong.code, "invalid_credentials");
    assert_eq!(wrong, unknown);
}

#[tokio::test]
async fn who_am_i_needs_a_session() {
    let server = server().await;
    let res = call(Method::GET, "/api/v1/session").send(&server).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(res).await, "unauthorized");

    let made_up = format!("rd_session={}", "a".repeat(64));
    assert_eq!(who_am_i(&server, &made_up).await, StatusCode::UNAUTHORIZED);

    let cookie = sign_in(&server, &server.password).await;
    let res = call(Method::GET, "/api/v1/session")
        .cookie(&cookie)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let info: SessionInfo = json_body(res).await;
    assert_eq!(info.username, "admin");
}

#[tokio::test]
async fn each_sign_in_gets_a_new_id_and_ends_the_one_it_replaces() {
    let server = server().await;
    let first = sign_in(&server, &server.password).await;
    let res = sign_in_call(&server.password)
        .cookie(&first)
        .send(&server)
        .await;
    let second = cookie_pair(&res);
    assert_ne!(first, second);
    assert_eq!(who_am_i(&server, &first).await, StatusCode::UNAUTHORIZED);
    assert_eq!(who_am_i(&server, &second).await, StatusCode::OK);
}

// ---------- Sign-out ----------

#[tokio::test]
async fn sign_out_ends_the_session_and_clears_the_cookie() {
    let server = server().await;
    let cookie = sign_in(&server, &server.password).await;

    let res = call(Method::DELETE, "/api/v1/session")
        .cookie(&cookie)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let cleared = set_cookie(&res);
    assert!(cleared.starts_with("rd_session=;"));
    assert!(cleared.contains("Max-Age=0"));

    assert_eq!(who_am_i(&server, &cookie).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sign_out_without_a_session_still_succeeds() {
    let server = server().await;
    let res = call(Method::DELETE, "/api/v1/session").send(&server).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

// ---------- Forced password change ----------

#[tokio::test]
async fn other_routes_need_a_session() {
    let server = server().await;
    let res = call(Method::GET, "/api/v1/server").send(&server).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(res).await, "unauthorized");
}

#[tokio::test]
async fn other_routes_are_refused_until_the_initial_password_is_changed() {
    let server = server().await;
    let cookie = sign_in(&server, &server.password).await;

    let res = call(Method::GET, "/api/v1/server")
        .cookie(&cookie)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(error_code(res).await, "password_change_required");

    let res = change_call(&cookie, &server.password, NEW_PASSWORD)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let new_cookie = cookie_pair(&res);
    let info: SessionInfo = json_body(res).await;
    assert!(!info.must_change_password);

    let res = call(Method::GET, "/api/v1/server")
        .cookie(&new_cookie)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_password_change_ends_every_session_and_replaces_the_password() {
    let server = server().await;
    let this_browser = sign_in(&server, &server.password).await;
    let other_browser = sign_in(&server, &server.password).await;

    let res = change_call(&this_browser, &server.password, NEW_PASSWORD)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let new_cookie = cookie_pair(&res);

    assert_eq!(
        who_am_i(&server, &this_browser).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        who_am_i(&server, &other_browser).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(who_am_i(&server, &new_cookie).await, StatusCode::OK);

    let old = sign_in_call(&server.password).send(&server).await;
    assert_eq!(old.status(), StatusCode::UNAUTHORIZED);
    sign_in(&server, NEW_PASSWORD).await;

    // The new hash is on disk, and the plaintext is not.
    let raw = std::fs::read_to_string(server.dir.path().join("credentials.json")).unwrap();
    assert!(raw.contains("\"must_change_password\": false"));
    assert!(!raw.contains(NEW_PASSWORD));
}

#[tokio::test]
async fn change_password_needs_a_session() {
    let server = server().await;
    let res = call(Method::POST, "/api/v1/session/password")
        .body(json!({"current_password": server.password, "new_password": NEW_PASSWORD}))
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn change_password_checks_the_current_password() {
    let server = server().await;
    let cookie = sign_in(&server, &server.password).await;
    let res = change_call(&cookie, "not the password", NEW_PASSWORD)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(error_code(res).await, "wrong_password");
    // The session survives, and the password is unchanged.
    assert_eq!(who_am_i(&server, &cookie).await, StatusCode::OK);
    sign_in(&server, &server.password).await;
}

#[tokio::test]
async fn weak_new_passwords_are_refused() {
    let server = server().await;
    let cookie = sign_in(&server, &server.password).await;
    let long = "ab".repeat(200);
    let cases = [
        ("short", "password_too_short"),
        ("password1234", "password_too_common"),
        ("aaaaaaaaaaaaaaaa", "password_too_common"),
        (long.as_str(), "password_too_long"),
        (server.password.as_str(), "password_unchanged"),
    ];
    for (new, code) in cases {
        let res = change_call(&cookie, &server.password, new)
            .send(&server)
            .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{new}");
        assert_eq!(error_code(res).await, code, "{new}");
    }
    // Nothing changed.
    assert_eq!(who_am_i(&server, &cookie).await, StatusCode::OK);
    sign_in(&server, &server.password).await;
}

// ---------- Lockout ----------

#[tokio::test]
async fn five_failures_lock_the_ip_address() {
    let server = server().await;
    for _ in 0..5 {
        let res = sign_in_call("not the password").send(&server).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    // Locked: even the correct password is refused, without being checked.
    let res = sign_in_call(&server.password).send(&server).await;
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after: u64 = res.headers()[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=30).contains(&retry_after), "got {retry_after}");
    assert_eq!(error_code(res).await, "rate_limited");

    // Another address is not affected.
    let res = sign_in_call(&server.password)
        .peer("10.0.0.2")
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_successful_sign_in_clears_the_failure_count() {
    let server = server().await;
    for _ in 0..2 {
        for _ in 0..4 {
            sign_in_call("not the password").send(&server).await;
        }
        sign_in(&server, &server.password).await;
    }
}

#[tokio::test]
async fn ten_failures_from_different_addresses_lock_the_account() {
    let server = server().await;
    let peers = [
        "10.0.1.1",
        "10.0.1.2",
        "10.0.1.3",
        "10.0.1.4",
        "10.0.1.5",
        "10.0.1.6",
        "10.0.1.7",
        "10.0.1.8",
        "10.0.1.9",
        "10.0.1.10",
    ];
    for peer in peers {
        let res = sign_in_call("not the password")
            .peer(peer)
            .send(&server)
            .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
    let res = sign_in_call(&server.password)
        .peer("10.0.2.1")
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after: u64 = res.headers()[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=5).contains(&retry_after), "got {retry_after}");
}

#[tokio::test]
async fn failures_for_unknown_users_do_not_lock_the_account() {
    let server = server().await;
    let peers = [
        "10.0.1.1",
        "10.0.1.2",
        "10.0.1.3",
        "10.0.1.4",
        "10.0.1.5",
        "10.0.1.6",
        "10.0.1.7",
        "10.0.1.8",
        "10.0.1.9",
        "10.0.1.10",
        "10.0.1.11",
        "10.0.1.12",
    ];
    for peer in peers {
        call(Method::POST, "/api/v1/session")
            .body(json!({"username": "root", "password": "not the password"}))
            .peer(peer)
            .send(&server)
            .await;
    }
    sign_in(&server, &server.password).await;
}

#[tokio::test]
async fn wrong_current_passwords_count_toward_the_ip_lock() {
    let server = server().await;
    let cookie = sign_in(&server, &server.password).await;
    for _ in 0..5 {
        let res = change_call(&cookie, "not the password", NEW_PASSWORD)
            .send(&server)
            .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }
    let res = change_call(&cookie, &server.password, NEW_PASSWORD)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn x_forwarded_for_is_ignored_unless_behind_a_proxy() {
    let server = server().await;
    let spoofed = ["1.1.1.1", "2.2.2.2", "3.3.3.3", "4.4.4.4", "5.5.5.5"];
    for ip in spoofed {
        sign_in_call("not the password")
            .header("x-forwarded-for", ip)
            .send(&server)
            .await;
    }
    // The 5 failures all counted against the real address.
    let res = sign_in_call(&server.password)
        .header("x-forwarded-for", "6.6.6.6")
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn behind_a_proxy_the_last_forwarded_address_is_the_client() {
    let server = server_with(true, true).await;
    for _ in 0..5 {
        // The client put "9.9.9.9" in the header itself. The proxy appended
        // the address it saw.
        sign_in_call("not the password")
            .peer("127.0.0.1")
            .header("x-forwarded-for", "9.9.9.9, 203.0.113.7")
            .send(&server)
            .await;
    }
    let locked = sign_in_call(&server.password)
        .peer("127.0.0.1")
        .header("x-forwarded-for", "8.8.8.8, 203.0.113.7")
        .send(&server)
        .await;
    assert_eq!(locked.status(), StatusCode::TOO_MANY_REQUESTS);

    let other_client = sign_in_call(&server.password)
        .peer("127.0.0.1")
        .header("x-forwarded-for", "203.0.113.8")
        .send(&server)
        .await;
    assert_eq!(other_client.status(), StatusCode::OK);
}

// ---------- CSRF ----------

#[tokio::test]
async fn cross_site_sign_in_is_rejected() {
    let server = server().await;
    for site in ["cross-site", "same-site", "none"] {
        let res = sign_in_call(&server.password)
            .sec_fetch_site(Some(site))
            .send(&server)
            .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "{site}");
        assert!(res.headers().get(header::SET_COOKIE).is_none());
        assert_eq!(error_code(res).await, "csrf_rejected");
    }
}

#[tokio::test]
async fn a_state_changing_request_with_no_origin_information_is_rejected() {
    let server = server().await;
    let res = sign_in_call(&server.password)
        .sec_fetch_site(None)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(error_code(res).await, "csrf_rejected");
}

#[tokio::test]
async fn the_origin_header_must_match_the_host() {
    let server = server().await;
    let foreign = sign_in_call(&server.password)
        .sec_fetch_site(None)
        .header("host", "desk.example:7171")
        .header("origin", "https://evil.example")
        .send(&server)
        .await;
    assert_eq!(foreign.status(), StatusCode::FORBIDDEN);

    let own = sign_in_call(&server.password)
        .sec_fetch_site(None)
        .header("host", "desk.example:7171")
        .header("origin", "http://desk.example:7171")
        .send(&server)
        .await;
    assert_eq!(own.status(), StatusCode::OK);
}

#[tokio::test]
async fn cross_site_sign_out_and_password_change_are_rejected() {
    let server = server().await;
    let cookie = sign_in(&server, &server.password).await;

    let res = call(Method::DELETE, "/api/v1/session")
        .cookie(&cookie)
        .sec_fetch_site(Some("cross-site"))
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    let res = change_call(&cookie, &server.password, NEW_PASSWORD)
        .sec_fetch_site(Some("cross-site"))
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Neither request had an effect.
    assert_eq!(who_am_i(&server, &cookie).await, StatusCode::OK);
    sign_in(&server, &server.password).await;
}

#[tokio::test]
async fn reads_are_not_subject_to_the_csrf_check() {
    let server = server().await;
    let cookie = sign_in(&server, &server.password).await;
    let res = call(Method::GET, "/api/v1/session")
        .cookie(&cookie)
        .sec_fetch_site(None)
        .send(&server)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
}

// ---------- Password reset from the command line ----------

#[tokio::test]
async fn a_reset_on_disk_ends_sessions_and_forces_a_change() {
    let server = server().await;
    let cookie = sign_in_and_change(&server).await;
    assert_eq!(who_am_i(&server, &cookie).await, StatusCode::OK);

    // What `rackdeskd reset-password` does, from another process.
    let (_, reset) = rd_auth::CredentialStore::new(server.dir.path())
        .reset()
        .unwrap();

    assert_eq!(who_am_i(&server, &cookie).await, StatusCode::UNAUTHORIZED);
    let old = sign_in_call(NEW_PASSWORD).send(&server).await;
    assert_eq!(old.status(), StatusCode::UNAUTHORIZED);

    let res = sign_in_call(&reset.0).send(&server).await;
    assert_eq!(res.status(), StatusCode::OK);
    let info: SessionInfo = json_body(res).await;
    assert!(info.must_change_password);
}

// ---------- Request handling ----------

#[tokio::test]
async fn bad_bodies_get_a_json_error() {
    let server = server().await;

    let not_json = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/session")
        .header("sec-fetch-site", "same-origin")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    let wrong_shape = sign_in_call("x").body(json!({"user": "admin"}));
    let mut not_json = not_json;
    not_json
        .extensions_mut()
        .insert(ConnectInfo("10.0.0.1:40000".parse::<SocketAddr>().unwrap()));

    let res = server.app.clone().oneshot(not_json).await.unwrap();
    assert!(res.status().is_client_error());
    assert_eq!(error_code(res).await, "bad_request");

    let res = wrong_shape.send(&server).await;
    assert!(res.status().is_client_error());
    assert_eq!(error_code(res).await, "bad_request");
}

#[tokio::test]
async fn a_body_over_the_size_limit_is_refused() {
    let server = server().await;
    let res = sign_in_call(&"x".repeat(16 * 1024)).send(&server).await;
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn unknown_api_routes_return_a_json_404() {
    let server = server().await;
    for path in ["/api/v1/nothing", "/api/v2/session", "/api/"] {
        let res = call(Method::GET, path).send(&server).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
    }
    let res = call(Method::GET, "/api/v1/nothing").send(&server).await;
    assert_eq!(error_code(res).await, "not_found");
}

#[tokio::test]
async fn api_responses_are_not_cacheable_and_carry_the_security_headers() {
    let server = server().await;
    let res = sign_in_call(&server.password).send(&server).await;
    let h = res.headers();
    assert_eq!(h[header::CACHE_CONTROL], "no-store");
    assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(h[header::X_FRAME_OPTIONS], "DENY");
    assert_eq!(h[header::REFERRER_POLICY], "no-referrer");
    assert!(h[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap()
        .contains("frame-ancestors 'none'"));
}

#[tokio::test]
async fn the_health_check_and_the_ui_need_no_session() {
    let server = server().await;
    let res = call(Method::GET, "/healthz").send(&server).await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = call(Method::GET, "/").send(&server).await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn sessions_are_counted_in_the_store() {
    let server = server().await;
    assert!(server.state.sessions().is_empty());
    let cookie = sign_in(&server, &server.password).await;
    assert_eq!(server.state.sessions().len(), 1);
    call(Method::DELETE, "/api/v1/session")
        .cookie(&cookie)
        .send(&server)
        .await;
    assert!(server.state.sessions().is_empty());
}
