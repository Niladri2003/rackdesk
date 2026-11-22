//! Sign-in, sign-out, "who am I" and change-password, plus the check that
//! protects every other API route.
//!
//! The rules come from docs/security/SECURITY.md, rows A2 to A5.

use crate::audit::Actor;
use crate::error::ApiErr;
use crate::state::AppState;
use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rd_api::{ChangePasswordRequest, LoginRequest, SessionInfo};
use rd_auth::session::ABSOLUTE_TIMEOUT;
use rd_auth::AuthError;
use std::net::{IpAddr, SocketAddr};
use std::time::Instant;

/// Cookie name on HTTPS. The `__Host-` prefix makes the browser refuse the
/// cookie unless it is `Secure`, has `Path=/` and has no `Domain`, so another
/// host or a plain-HTTP page can't set or replace it.
const SECURE_COOKIE: &str = "__Host-rd_session";
/// Cookie name on plain HTTP, which is for localhost only.
const PLAIN_COOKIE: &str = "rd_session";

/// A request that carries a live session.
struct Authed {
    session_id: String,
    username: String,
    must_change_password: bool,
}

impl Authed {
    fn info(&self) -> SessionInfo {
        SessionInfo {
            username: self.username.clone(),
            must_change_password: self.must_change_password,
        }
    }
}

fn cookie_name(state: &AppState) -> &'static str {
    if state.config().https {
        SECURE_COOKIE
    } else {
        PLAIN_COOKIE
    }
}

/// Reads the session id from the `Cookie` headers.
fn session_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

fn set_cookie(state: &AppState, session_id: &str, max_age_secs: u64) -> HeaderValue {
    let secure = if state.config().https { "; Secure" } else { "" };
    let value = format!(
        "{}={session_id}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age_secs}{secure}",
        cookie_name(state)
    );
    // The id is hex and the rest is fixed text, so this can't fail.
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn with_new_session(state: &AppState, session_id: &str, body: SessionInfo) -> Response {
    let cookie = set_cookie(state, session_id, ABSOLUTE_TIMEOUT.as_secs());
    ([(header::SET_COOKIE, cookie)], Json(body)).into_response()
}

async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<Authed, ApiErr> {
    // Load the account first: if the password was reset outside the server,
    // this ends the old sessions before the lookup below.
    let creds = state.credentials().await?;
    let id = session_cookie(headers, cookie_name(state)).ok_or_else(ApiErr::unauthorized)?;
    let username = state
        .sessions()
        .touch(id, Instant::now())
        .ok_or_else(ApiErr::unauthorized)?;
    Ok(Authed {
        session_id: id.to_owned(),
        must_change_password: creds.must_change_password,
        username,
    })
}

/// The key for the per-IP limiter.
fn client_ip(state: &AppState, headers: &HeaderMap, peer: SocketAddr) -> IpAddr {
    if state.config().behind_proxy {
        // The proxy next to us appends the address it saw as the last entry.
        // Entries before it come from the client and can't be trusted.
        let forwarded = headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .next_back()
            .and_then(|ip| ip.trim().parse().ok());
        if let Some(ip) = forwarded {
            return ip;
        }
    }
    peer.ip()
}

/// Middleware for every API route outside `/session`: the request needs a
/// session, and the initial password must have been changed.
pub async fn require_active_session(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiErr> {
    let authed = authenticate(&state, req.headers()).await?;
    if authed.must_change_password {
        return Err(ApiErr::password_change_required());
    }
    // Handlers that change something read this for the audit log.
    let ip = match req.extensions().get::<ConnectInfo<SocketAddr>>() {
        Some(ConnectInfo(peer)) => client_ip(&state, req.headers(), *peer),
        None => IpAddr::from([0, 0, 0, 0]),
    };
    req.extensions_mut().insert(Actor {
        username: authed.username,
        ip,
        session: authed.session_id,
    });
    Ok(next.run(req).await)
}

/// `GET /api/v1/session`
pub async fn who_am_i(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SessionInfo>, ApiErr> {
    Ok(Json(authenticate(&state, &headers).await?.info()))
}

/// `POST /api/v1/session`
pub async fn sign_in(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Response, ApiErr> {
    let Json(login) = body?;
    let now = Instant::now();
    let creds = state.credentials().await?;
    let known_user = login.username == creds.username;
    let actor = Actor {
        // A name that is not an account is not written to the log. People
        // type a password in the wrong field.
        username: if known_user {
            creds.username.clone()
        } else {
            "(unknown user)".to_owned()
        },
        ip: client_ip(&state, &headers, peer),
        session: String::new(),
    };
    let ip = actor.ip.to_string();

    // While a key is locked the password is not checked at all, so a locked
    // client learns nothing from its attempts.
    let locked = state.ip_limiter().check(&ip, now).and_then(|()| {
        if known_user {
            state.account_limiter().check(&creds.username, now)
        } else {
            Ok(())
        }
    });
    if let Err(wait) = locked {
        state
            .audit()
            .record(&actor, "session.sign_in", "", Err("rate_limited"));
        return Err(ApiErr::rate_limited(wait));
    }

    // The attempt is counted before the password is checked, and a correct
    // password clears the count below. The check waits for a free slot, and
    // many requests sent at once would otherwise all pass the lock above
    // before the first of them is counted.
    state.ip_limiter().failure(&ip, now);
    if known_user {
        state.account_limiter().failure(&creds.username, now);
    }

    let hash = known_user.then(|| creds.password_hash.clone());
    if !state.verify_password(login.password, hash).await? {
        tracing::warn!(%ip, "failed sign-in");
        state
            .audit()
            .record(&actor, "session.sign_in", "", Err("invalid_credentials"));
        return Err(ApiErr::invalid_credentials());
    }

    state.ip_limiter().success(&ip);
    state.account_limiter().success(&creds.username);
    // A sign-in always gets a new id. An id the browser already had is ended.
    if let Some(old) = session_cookie(&headers, cookie_name(&state)) {
        state.sessions().revoke(old);
    }
    let id = state
        .sessions()
        .create(&creds.username, now)
        .map_err(ApiErr::internal)?;
    tracing::info!(%ip, "signed in");
    state.audit().record(&actor, "session.sign_in", "", Ok(()));
    Ok(with_new_session(
        &state,
        &id,
        SessionInfo {
            username: creds.username,
            must_change_password: creds.must_change_password,
        },
    ))
}

/// `DELETE /api/v1/session`
pub async fn sign_out(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(id) = session_cookie(&headers, cookie_name(&state)) {
        // Only a session that was live is recorded.
        if let Some(username) = state.sessions().touch(id, Instant::now()) {
            let actor = Actor {
                username,
                ip: client_ip(&state, &headers, peer),
                session: String::new(),
            };
            state.audit().record(&actor, "session.sign_out", "", Ok(()));
        }
        state.sessions().revoke(id);
        state.terminals().stop(Some(id));
    }
    // Max-Age=0 makes the browser delete the cookie.
    let cookie = set_cookie(&state, "", 0);
    (StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response()
}

/// `POST /api/v1/session/password`
pub async fn change_password(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Result<Json<ChangePasswordRequest>, JsonRejection>,
) -> Result<Response, ApiErr> {
    let authed = authenticate(&state, &headers).await?;
    let Json(change) = body?;
    let now = Instant::now();
    let ip = client_ip(&state, &headers, peer).to_string();
    let creds = state.credentials().await?;

    // The current password is asked again, so a stolen session can't lock the
    // admin out. Wrong guesses here count like failed sign-ins.
    state
        .ip_limiter()
        .check(&ip, now)
        .map_err(ApiErr::rate_limited)?;
    // Counted before the check and cleared after a correct password, as in
    // `sign_in`.
    state.ip_limiter().failure(&ip, now);
    let hash = Some(creds.password_hash.clone());
    if !state
        .verify_password(change.current_password.clone(), hash)
        .await?
    {
        tracing::warn!(%ip, "wrong current password on password change");
        let actor = Actor {
            username: authed.username.clone(),
            ip: client_ip(&state, &headers, peer),
            session: String::new(),
        };
        state
            .audit()
            .record(&actor, "session.change_password", "", Err("wrong_password"));
        return Err(ApiErr::new(
            StatusCode::FORBIDDEN,
            "wrong_password",
            "The current password is wrong.",
        ));
    }

    state.ip_limiter().success(&ip);

    if change.new_password == change.current_password {
        return Err(ApiErr::new(
            StatusCode::BAD_REQUEST,
            "password_unchanged",
            "The new password must differ from the current one.",
        ));
    }
    rd_auth::check_new_password(&change.new_password).map_err(|e| {
        let code = match e {
            AuthError::WeakPassword => "password_too_short",
            AuthError::PasswordTooLong => "password_too_long",
            _ => "password_too_common",
        };
        ApiErr::new(StatusCode::BAD_REQUEST, code, e.to_string())
    })?;

    let new_hash = state.hash_password(change.new_password).await?;
    let creds = state.set_password_hash(new_hash).await?;

    // Every session of the account ends, on every device. This browser gets
    // a new one.
    state.sessions().revoke_user(&authed.username);
    state.sessions().revoke(&authed.session_id);
    let id = state
        .sessions()
        .create(&creds.username, now)
        .map_err(ApiErr::internal)?;
    tracing::info!(%ip, "password changed");
    let actor = Actor {
        username: authed.username.clone(),
        ip: client_ip(&state, &headers, peer),
        session: String::new(),
    };
    // The password change ended every session, so every terminal stops.
    state.terminals().stop(None);
    state
        .audit()
        .record(&actor, "session.change_password", "", Ok(()));
    Ok(with_new_session(
        &state,
        &id,
        SessionInfo {
            username: creds.username,
            must_change_password: creds.must_change_password,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookies(values: &[&'static str]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for v in values {
            map.append(header::COOKIE, HeaderValue::from_static(v));
        }
        map
    }

    #[test]
    fn finds_the_session_cookie_among_others() {
        let h = cookies(&["theme=dark; rd_session=abc123; other=1"]);
        assert_eq!(session_cookie(&h, "rd_session"), Some("abc123"));
    }

    #[test]
    fn reads_more_than_one_cookie_header() {
        let h = cookies(&["theme=dark", "rd_session=abc123"]);
        assert_eq!(session_cookie(&h, "rd_session"), Some("abc123"));
    }

    #[test]
    fn a_cookie_with_a_similar_name_does_not_match() {
        let h = cookies(&["xrd_session=1; rd_session_old=2; __Host-rd_session=3"]);
        assert_eq!(session_cookie(&h, "rd_session"), None);
        assert_eq!(session_cookie(&h, "__Host-rd_session"), Some("3"));
    }

    #[test]
    fn no_cookie_header_gives_none() {
        assert_eq!(session_cookie(&HeaderMap::new(), "rd_session"), None);
    }
}
