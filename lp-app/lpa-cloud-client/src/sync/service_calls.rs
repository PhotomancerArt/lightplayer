//! The requests this engine makes, named.
//!
//! One function per request, each a thin call to
//! [`call`](crate::cloud_port::call) — which already knows, from
//! [`CloudCallSpec`](lpc_cloud_api::CloudCallSpec), which response answers
//! which request. Nothing here unwraps a `CloudResponse`; these exist so the
//! operations below read as `get_heads(port, uid)` rather than as envelope
//! construction, and so the argument lists stay positional at the call sites.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_cloud_api::request::{
    GetEvents, GetHeads, GetProject, HaveBlobs, PublishProject, PushCommit,
};
use lpc_cloud_api::response::{Events, ProjectInfo, PushResult};
use lpc_cloud_api::{Access, HeadInfo, SidecarMeta};
use lpc_history::{ContentHash, HistoryEvent, PrefixedUid};

use crate::cloud_port::{CloudPort, call};
use crate::sync_error::SyncError;

pub(crate) async fn publish_project<P: CloudPort + ?Sized>(
    port: &P,
    uid: PrefixedUid,
    access: Access,
    slug: String,
) -> Result<ProjectInfo, SyncError> {
    call(port, PublishProject { uid, access, slug }).await
}

pub(crate) async fn get_project<P: CloudPort + ?Sized>(
    port: &P,
    uid: PrefixedUid,
) -> Result<ProjectInfo, SyncError> {
    call(port, GetProject { uid }).await
}

pub(crate) async fn get_heads<P: CloudPort + ?Sized>(
    port: &P,
    uid: PrefixedUid,
) -> Result<Vec<HeadInfo>, SyncError> {
    Ok(call(port, GetHeads { uid }).await?.heads)
}

pub(crate) async fn get_events<P: CloudPort + ?Sized>(
    port: &P,
    uid: PrefixedUid,
    since: u64,
) -> Result<Events, SyncError> {
    call(port, GetEvents { uid, since }).await
}

pub(crate) async fn have_blobs<P: CloudPort + ?Sized>(
    port: &P,
    hashes: Vec<ContentHash>,
) -> Result<Vec<ContentHash>, SyncError> {
    Ok(call(port, HaveBlobs { hashes }).await?.hashes)
}

pub(crate) async fn push_commit<P: CloudPort + ?Sized>(
    port: &P,
    uid: PrefixedUid,
    parents: Vec<ContentHash>,
    tree: ContentHash,
    events: Vec<HistoryEvent>,
    sidecar: SidecarMeta,
) -> Result<PushResult, SyncError> {
    call(
        port,
        PushCommit {
            uid,
            parents,
            tree,
            events,
            sidecar,
        },
    )
    .await
}
