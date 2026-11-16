//! Rejects state-changing requests that another website caused.
//!
//! The session cookie is `SameSite=Strict`, which already keeps it out of
//! cross-site requests. This check is the second control: it also covers
//! requests that carry no cookie, such as a sign-in attempt
//! (docs/security/SECURITY.md, A5).

use crate::error::ApiErr;
use axum::extract::Request;
use axum::http::{header, HeaderMap, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub async fn reject_cross_site(req: Request, next: Next) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if safe || same_origin(req.headers(), req.uri().authority().map(|a| a.as_str())) {
        next.run(req).await
    } else {
        ApiErr::csrf_rejected().into_response()
    }
}

/// `uri_authority` is the host and port from the request line. HTTP/2 sends
/// it there in place of a `Host` header.
fn same_origin(headers: &HeaderMap, uri_authority: Option<&str>) -> bool {
    // Browsers set `Sec-Fetch-Site` themselves. Page scripts can't change it.
    if let Some(site) = headers.get("sec-fetch-site") {
        return site.as_bytes() == b"same-origin";
    }
    // Older browsers, and pages on plain HTTP, send only `Origin`. Its host
    // and port must be the ones this request was sent to.
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        // Neither header: not a browser we can check, so refuse.
        return false;
    };
    let Some(origin_authority) = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
    else {
        return false;
    };
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .or(uri_authority);
    match host {
        Some(host) => !host.is_empty() && origin_authority.eq_ignore_ascii_case(host),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, HeaderValue::from_static(value));
        }
        map
    }

    #[test]
    fn sec_fetch_site_same_origin_passes() {
        assert!(same_origin(
            &headers(&[("sec-fetch-site", "same-origin")]),
            None
        ));
    }

    #[test]
    fn other_sec_fetch_site_values_fail() {
        for value in ["cross-site", "same-site", "none", ""] {
            let h = headers(&[
                ("sec-fetch-site", value),
                ("origin", "http://desk.example"),
                ("host", "desk.example"),
            ]);
            assert!(!same_origin(&h, None), "{value:?} should be refused");
        }
    }

    #[test]
    fn origin_must_match_the_host() {
        let ok = headers(&[
            ("origin", "https://Desk.example:7171"),
            ("host", "desk.example:7171"),
        ]);
        assert!(same_origin(&ok, None));

        let other_site = headers(&[("origin", "https://evil.example"), ("host", "desk.example")]);
        assert!(!same_origin(&other_site, None));

        let other_port = headers(&[
            ("origin", "http://desk.example:8080"),
            ("host", "desk.example:7171"),
        ]);
        assert!(!same_origin(&other_port, None));

        let prefix = headers(&[
            ("origin", "http://desk.example.evil.example"),
            ("host", "desk.example"),
        ]);
        assert!(!same_origin(&prefix, None));
    }

    #[test]
    fn opaque_and_missing_origins_fail() {
        assert!(!same_origin(
            &headers(&[("origin", "null"), ("host", "desk.example")]),
            None
        ));
        assert!(!same_origin(&headers(&[("host", "desk.example")]), None));
        assert!(!same_origin(&headers(&[]), None));
    }

    #[test]
    fn the_request_line_authority_is_used_when_there_is_no_host_header() {
        let h = headers(&[("origin", "https://desk.example")]);
        assert!(same_origin(&h, Some("desk.example")));
        assert!(!same_origin(&h, Some("other.example")));
    }
}
