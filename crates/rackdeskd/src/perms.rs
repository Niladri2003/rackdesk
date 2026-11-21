//! Owner and permission bits: read them, and change them.
//!
//! A change is tried with the rights of the server first. If the system
//! refuses it and the request has `elevate`, the server asks the root helper
//! (`helper.rs`). Without `elevate` the helper is never used, so an action
//! as root is always something the admin asked for.

use crate::audit::Actor;
use crate::error::ApiErr;
use crate::files::{blocking, PathQuery};
use crate::helper::no_helper;
use crate::ids;
use crate::state::AppState;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Extension, Query, State};
use axum::http::StatusCode;
use axum::Json;
use rd_api::{ChmodRequest, ChownRequest, EntryDetails};
use rd_helper_proto::Request;

/// `GET /api/v1/fs/properties`
pub async fn properties(
    State(state): State<AppState>,
    query: Result<Query<PathQuery>, QueryRejection>,
) -> Result<Json<EntryDetails>, ApiErr> {
    let Query(q) = query?;
    let (entry, ids) = blocking(&state, move |root| {
        Ok((root.entry(&q.path)?, root.owner(&q.path)?))
    })
    .await?;
    Ok(Json(EntryDetails {
        entry,
        uid: Some(ids.0),
        gid: Some(ids.1),
        owner: ids::user_name(ids.0),
        group: ids::group_name(ids.1),
        can_elevate: state.helper().is_some(),
    }))
}

/// Records the change, with "helper." in the action name if it was done as
/// root.
fn record(
    state: &AppState,
    actor: &Actor,
    action: &str,
    elevate: bool,
    target: &str,
    done: &Result<(), ApiErr>,
) {
    let action = if elevate {
        format!("helper.{action}")
    } else {
        format!("fs.{action}")
    };
    let result = done.as_ref().map(|_| ()).map_err(ApiErr::code);
    state.audit().record(actor, &action, target, result);
}

/// `POST /api/v1/fs/chmod`
pub async fn chmod(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<ChmodRequest>, JsonRejection>,
) -> Result<StatusCode, ApiErr> {
    let Json(req) = body?;
    let target = format!("{} mode {:o}", req.path, req.mode);
    let done = if req.elevate {
        match state.helper() {
            Some(helper) => helper
                .call(Request::Chmod {
                    path: req.path,
                    mode: req.mode,
                })
                .await
                .map(|_| ()),
            None => Err(no_helper()),
        }
    } else {
        blocking(&state, move |root| root.set_mode(&req.path, req.mode)).await
    };
    record(&state, &actor, "chmod", req.elevate, &target, &done);
    done.map(|()| StatusCode::NO_CONTENT)
}

/// `POST /api/v1/fs/chown`
pub async fn chown(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    body: Result<Json<ChownRequest>, JsonRejection>,
) -> Result<StatusCode, ApiErr> {
    let Json(req) = body?;
    let unknown = |what: &str| {
        ApiErr::new(
            StatusCode::BAD_REQUEST,
            "unknown_name",
            format!("No such {what}."),
        )
    };
    let uid = match &req.owner {
        Some(name) => Some(ids::user_id(name).ok_or_else(|| unknown("user"))?),
        None => None,
    };
    let gid = match &req.group {
        Some(name) => Some(ids::group_id(name).ok_or_else(|| unknown("group"))?),
        None => None,
    };
    if uid.is_none() && gid.is_none() {
        return Err(ApiErr::bad_request("Give an owner, a group, or both."));
    }
    let target = format!(
        "{} owner {} group {}",
        req.path,
        req.owner.as_deref().unwrap_or("-"),
        req.group.as_deref().unwrap_or("-")
    );
    let done = if req.elevate {
        match state.helper() {
            Some(helper) => helper
                .call(Request::Chown {
                    path: req.path,
                    uid,
                    gid,
                })
                .await
                .map(|_| ()),
            None => Err(no_helper()),
        }
    } else {
        blocking(&state, move |root| root.set_owner(&req.path, uid, gid)).await
    };
    record(&state, &actor, "chown", req.elevate, &target, &done);
    done.map(|()| StatusCode::NO_CONTENT)
}
