//! Helpers shared by the integration tests. They call the router in-process:
//! no socket.
// Each test file uses a part of these helpers.
#![allow(dead_code)]

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, Response, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use rackdeskd::{AppState, Config};
use rd_api::ApiError;
use serde_json::json;
use std::net::SocketAddr;
use tower::ServiceExt;

pub const NEW_PASSWORD: &str = "a new and unusual phrase";

pub struct Server {
    pub app: Router,
    pub state: AppState,
    /// The generated initial password.
    pub password: String,
    pub dir: tempfile::TempDir,
}

impl Server {
    /// The folder the file browser shows as "/".
    pub fn files_root(&self) -> std::path::PathBuf {
        self.dir.path().join("files")
    }
}

pub async fn server() -> Server {
    server_with(false, false).await
}

pub async fn server_with(https: bool, behind_proxy: bool) -> Server {
    server_from(|config| {
        config.https = https;
        config.behind_proxy = behind_proxy;
    })
    .await
}

/// A server whose settings are changed by `edit` before it starts.
pub async fn server_from(edit: impl FnOnce(&mut Config)) -> Server {
    let dir = tempfile::tempdir().unwrap();
    let store = rd_auth::CredentialStore::new(dir.path());
    let (creds, initial) = store.load_or_init().unwrap();
    let files_root = dir.path().join("files");
    std::fs::create_dir(&files_root).unwrap();
    let mut config = Config {
        data_dir: dir.path().to_owned(),
        files_root: files_root.clone(),
        // Larger than any disk, so a test can ask for more than the free
        // space without reaching this limit first.
        max_upload_bytes: u64::MAX / 4,
        root_socket: None,
        terminal: rackdeskd::term::TermMode::Off,
        https: false,
        behind_proxy: false,
    };
    edit(&mut config);
    let files = rd_fs::Root::open(&files_root).unwrap();
    let state = AppState::new(config, creds, files).await.unwrap();
    Server {
        app: rackdeskd::router(state.clone()),
        state,
        password: initial.unwrap().0,
        dir,
    }
}

/// A request as the RackDesk page sends it: same-origin, from 10.0.0.1.
pub struct Call {
    method: Method,
    path: String,
    body: Option<serde_json::Value>,
    cookie: Option<String>,
    peer: &'static str,
    /// `None` leaves the header out.
    sec_fetch_site: Option<&'static str>,
    extra: Vec<(&'static str, &'static str)>,
}

pub fn call(method: Method, path: &str) -> Call {
    Call {
        method,
        path: path.to_owned(),
        body: None,
        cookie: None,
        peer: "10.0.0.1",
        sec_fetch_site: Some("same-origin"),
        extra: Vec::new(),
    }
}

impl Call {
    pub fn body(mut self, body: serde_json::Value) -> Self {
        self.body = Some(body);
        self
    }
    pub fn cookie(mut self, cookie: &str) -> Self {
        self.cookie = Some(cookie.to_owned());
        self
    }
    pub fn peer(mut self, peer: &'static str) -> Self {
        self.peer = peer;
        self
    }
    pub fn sec_fetch_site(mut self, value: Option<&'static str>) -> Self {
        self.sec_fetch_site = value;
        self
    }
    pub fn header(mut self, name: &'static str, value: &'static str) -> Self {
        self.extra.push((name, value));
        self
    }

    pub async fn send(self, server: &Server) -> Response<Body> {
        let mut req = Request::builder().method(self.method).uri(&self.path);
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

pub async fn json_body<T: serde::de::DeserializeOwned>(res: Response<Body>) -> T {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "body is not the expected JSON ({e}): {}",
            String::from_utf8_lossy(&bytes)
        )
    })
}

pub async fn error_code(res: Response<Body>) -> String {
    json_body::<ApiError>(res).await.code
}

/// The full `Set-Cookie` value.
pub fn set_cookie(res: &Response<Body>) -> String {
    res.headers()
        .get(header::SET_COOKIE)
        .expect("a Set-Cookie header")
        .to_str()
        .unwrap()
        .to_owned()
}

/// `name=value`, as the browser sends it back.
pub fn cookie_pair(res: &Response<Body>) -> String {
    set_cookie(res).split(';').next().unwrap().to_owned()
}

pub fn sign_in_call(password: &str) -> Call {
    call(Method::POST, "/api/v1/session").body(json!({"username": "admin", "password": password}))
}

/// Signs in and returns the cookie pair.
pub async fn sign_in(server: &Server, password: &str) -> String {
    let res = sign_in_call(password).send(server).await;
    assert_eq!(res.status(), StatusCode::OK);
    cookie_pair(&res)
}

pub fn change_call(cookie: &str, current: &str, new: &str) -> Call {
    call(Method::POST, "/api/v1/session/password")
        .cookie(cookie)
        .body(json!({"current_password": current, "new_password": new}))
}

/// Signs in with the initial password, changes it to [`NEW_PASSWORD`], and
/// returns the cookie pair of the new session.
pub async fn sign_in_and_change(server: &Server) -> String {
    let cookie = sign_in(server, &server.password).await;
    let res = change_call(&cookie, &server.password, NEW_PASSWORD)
        .send(server)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    cookie_pair(&res)
}

pub async fn who_am_i(server: &Server, cookie: &str) -> StatusCode {
    call(Method::GET, "/api/v1/session")
        .cookie(cookie)
        .send(server)
        .await
        .status()
}

/// Signs in, replaces the initial password, and returns the cookie pair of a
/// session that may use every route.
pub async fn active_session(server: &Server) -> String {
    sign_in_and_change(server).await
}

pub async fn bytes_body(res: Response<Body>) -> Vec<u8> {
    res.into_body().collect().await.unwrap().to_bytes().to_vec()
}

// ---------- The terminal WebSocket ----------

use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Error, Message};

pub type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub async fn listen(server: &Server) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = server
        .app
        .clone()
        .into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, app).await });
    addr
}

/// Opens the terminal WebSocket. `origin` is the page that asks for it.
pub async fn open(
    addr: SocketAddr,
    cookie: Option<&str>,
    origin: Option<&str>,
    query: &str,
) -> Result<Socket, Error> {
    let mut request = format!("ws://{addr}/api/v1/term?{query}")
        .into_client_request()
        .unwrap();
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert("cookie", cookie.parse().unwrap());
    }
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
    }
    tokio_tungstenite::connect_async(request)
        .await
        .map(|(socket, _)| socket)
}

pub fn status_of(result: Result<Socket, Error>) -> StatusCode {
    match result {
        Err(Error::Http(response)) => response.status(),
        Ok(_) => panic!("the terminal opened"),
        Err(other) => panic!("unexpected error: {other}"),
    }
}

/// Reads the `ready` message and returns the terminal id.
pub async fn ready(socket: &mut Socket) -> String {
    let message = socket.next().await.unwrap().unwrap();
    let json: serde_json::Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
    assert_eq!(json["type"], "ready");
    json["id"].as_str().unwrap().to_owned()
}

pub async fn type_line(socket: &mut Socket, line: &str) {
    let mut bytes = vec![0u8];
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');
    socket.send(Message::Binary(bytes.into())).await.unwrap();
}

/// Reads output until it holds `wanted`, or 10 s have passed.
pub async fn read_until(socket: &mut Socket, wanted: &str) -> String {
    let mut seen = String::new();
    let wait = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(Ok(message)) = socket.next().await {
            if let Message::Binary(bytes) = message {
                seen.push_str(&String::from_utf8_lossy(&bytes));
                if seen.contains(wanted) {
                    break;
                }
            }
        }
    });
    let _ = wait.await;
    assert!(seen.contains(wanted), "wanted {wanted:?} in: {seen}");
    seen
}
