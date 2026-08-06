//! Typed wrappers over the request/response vocabulary.
//!
//! One function per request the engine makes, each unwrapping the one
//! response shape that request can legally produce. Written once here so no
//! operation below has to carry a `match` for a response that cannot happen —
//! and so that when one does happen, exactly one place says
//! [`SyncError::UnexpectedResponse`].

use alloc::string::String;
use alloc::vec::Vec;

use lpc_cloud_api::{
    CloudRequest, CloudResponse, HeadInfo, ProjectMeta, PushOutcome, SidecarMeta, Visibility,
};
use lpc_history::{ContentHash, HistoryEvent, PrefixedUid};

use crate::cloud_port::{CloudPort, request};
use crate::sync_error::SyncError;

/// A project as the service currently holds it.
pub(crate) struct RemoteProject {
    pub meta: ProjectMeta,
    pub heads: Vec<HeadInfo>,
    pub sidecar: SidecarMeta,
}

/// A stretch of the service's event log.
pub(crate) struct RemoteEvents {
    pub events: Vec<HistoryEvent>,
    pub next_since: u64,
}

pub(crate) async fn publish_project<P: CloudPort + ?Sized>(
    port: &P,
    uid: PrefixedUid,
    visibility: Visibility,
    slug: String,
) -> Result<RemoteProject, SyncError> {
    project_info(
        request(
            port,
            CloudRequest::PublishProject {
                uid,
                visibility,
                slug,
            },
        )
        .await?,
        "publishProject",
    )
}

pub(crate) async fn get_project<P: CloudPort + ?Sized>(
    port: &P,
    uid: PrefixedUid,
) -> Result<RemoteProject, SyncError> {
    project_info(
        request(port, CloudRequest::GetProject { uid }).await?,
        "getProject",
    )
}

pub(crate) async fn get_heads<P: CloudPort + ?Sized>(
    port: &P,
    uid: PrefixedUid,
) -> Result<Vec<HeadInfo>, SyncError> {
    match request(port, CloudRequest::GetHeads { uid }).await? {
        CloudResponse::Heads { heads } => Ok(heads),
        _ => Err(SyncError::UnexpectedResponse("getHeads")),
    }
}

pub(crate) async fn get_events<P: CloudPort + ?Sized>(
    port: &P,
    uid: PrefixedUid,
    since: u64,
) -> Result<RemoteEvents, SyncError> {
    match request(port, CloudRequest::GetEvents { uid, since }).await? {
        CloudResponse::Events { events, next_since } => Ok(RemoteEvents { events, next_since }),
        _ => Err(SyncError::UnexpectedResponse("getEvents")),
    }
}

pub(crate) async fn have_blobs<P: CloudPort + ?Sized>(
    port: &P,
    hashes: Vec<ContentHash>,
) -> Result<Vec<ContentHash>, SyncError> {
    match request(port, CloudRequest::HaveBlobs { hashes }).await? {
        CloudResponse::MissingBlobs { hashes } => Ok(hashes),
        _ => Err(SyncError::UnexpectedResponse("haveBlobs")),
    }
}

pub(crate) async fn push_commit<P: CloudPort + ?Sized>(
    port: &P,
    uid: PrefixedUid,
    parents: Vec<ContentHash>,
    tree: ContentHash,
    events: Vec<HistoryEvent>,
    sidecar: SidecarMeta,
) -> Result<(PushOutcome, Vec<HeadInfo>), SyncError> {
    match request(
        port,
        CloudRequest::PushCommit {
            uid,
            parents,
            tree,
            events,
            sidecar,
        },
    )
    .await?
    {
        CloudResponse::PushResult { outcome, heads } => Ok((outcome, heads)),
        _ => Err(SyncError::UnexpectedResponse("pushCommit")),
    }
}

fn project_info(response: CloudResponse, what: &'static str) -> Result<RemoteProject, SyncError> {
    match response {
        CloudResponse::ProjectInfo {
            meta,
            heads,
            sidecar,
        } => Ok(RemoteProject {
            meta,
            heads,
            sidecar,
        }),
        _ => Err(SyncError::UnexpectedResponse(what)),
    }
}
