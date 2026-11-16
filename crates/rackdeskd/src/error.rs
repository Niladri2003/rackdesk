//! The error type of every API handler.
//!
//! Each error becomes a JSON [`rd_api::ApiError`] with a stable `code`. The UI
//! matches on the code, never on the message.

use axum::extract::rejection::JsonRejection;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rd_api::ApiError;

#[derive(Debug)]
pub struct ApiErr {
    status: StatusCode,
    code: &'static str,
    message: String,
    /// Seconds, sent as the `Retry-After` header.
    retry_after: Option<u64>,
}

impl ApiErr {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            retry_after: None,
        }
    }

    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "Sign in first.")
    }

    pub fn invalid_credentials() -> Self {
        // The same answer for a wrong username and a wrong password.
        Self::new(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "Wrong username or password.",
        )
    }

    pub fn password_change_required() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "password_change_required",
            "Change the initial password first.",
        )
    }

    pub fn rate_limited(wait: std::time::Duration) -> Self {
        // Round up, so the client never retries a moment too early.
        let secs = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
        let mut err = Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            format!("Too many failed attempts. Try again in {secs} s."),
        );
        err.retry_after = Some(secs.max(1));
        err
    }

    pub fn csrf_rejected() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "csrf_rejected",
            "This request did not come from the RackDesk page.",
        )
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    /// For failures inside the server. The detail goes to the log, not to the
    /// client.
    pub fn internal(detail: impl std::fmt::Display) -> Self {
        tracing::error!(%detail, "internal error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "The server could not complete the request.",
        )
    }
}

impl IntoResponse for ApiErr {
    fn into_response(self) -> Response {
        let mut res = (self.status, Json(ApiError::new(self.code, self.message))).into_response();
        if let Some(secs) = self.retry_after {
            res.headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(secs));
        }
        res
    }
}

impl From<JsonRejection> for ApiErr {
    fn from(rejection: JsonRejection) -> Self {
        // Keep the status (for example 413 for a body that is too large) and
        // replace the plain-text body with the JSON error shape.
        Self::new(
            rejection.status(),
            "bad_request",
            "The request body is not valid.",
        )
    }
}

pub async fn no_such_route() -> ApiErr {
    ApiErr::new(StatusCode::NOT_FOUND, "not_found", "No such API route.")
}
