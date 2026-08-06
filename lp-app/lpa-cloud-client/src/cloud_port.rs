//! The transport trait: everything the client needs from a cloud service.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use lpc_cloud_api::{CLOUD_API_VERSION, CloudCall, CloudReply, CloudRequest, CloudResponse};
use lpc_history::{ContentHash, TreeManifest};

use crate::sync_error::SyncError;

/// A cloud service, as the client sees it.
///
/// Two planes, mirroring the deployed API (D11):
///
/// - **Control plane** — [`call`](CloudPort::call): one
///   [`CloudCall`] in, one [`CloudReply`] out, the whole
///   [`CloudRequest`]/[`CloudResponse`] vocabulary.
/// - **Content plane** — blobs and trees, addressed by content hash and
///   transferred outside the message envelope. `PUT`/`GET /b/<hash>` in the
///   HTTP transport.
///
/// A **tree** is a blob like any other, with one twist worth stating: its
/// address is the *package* hash (the canonical `lph1` preimage over the
/// manifest's entries), not the hash of the manifest JSON that carries it.
/// That is what makes a commit's `tree` hash directly fetchable, and it stays
/// verifiable: a receiver recomputes [`TreeManifest::package_hash`] and
/// rejects a mismatch, exactly as it recomputes a file blob's hash. Hence
/// the separate [`get_tree`](CloudPort::get_tree)/[`put_tree`](CloudPort::put_tree)
/// pair rather than a raw-bytes call the caller could address wrongly.
///
/// # Runtime neutrality
///
/// Implementations return plain futures: no spawning, no executor-flavored
/// sleeps, no `Send` bound (the wasm edge that drives them is
/// single-threaded). Any edge must be able to drive them — see the sans-IO
/// rule in AGENTS.md and [`crate::block_on`] for the test driver.
#[allow(
    async_fn_in_trait,
    reason = "runtime-neutral futures by design: a Send bound would exclude the single-threaded wasm edge this is written for"
)]
pub trait CloudPort {
    /// Make one control-plane call.
    ///
    /// The `Err` side is transport failure — the service was not reached, or
    /// answered unintelligibly. A service that answered *with a refusal* is
    /// `Ok(reply)` carrying `reply.result == Err(CloudError)`; see
    /// [`request`] for the helper that folds both into [`SyncError`].
    async fn call(&self, call: CloudCall) -> Result<CloudReply, TransportError>;

    /// Fetch a file blob's bytes by content hash.
    async fn get_blob(&self, hash: ContentHash) -> Result<Vec<u8>, TransportError>;

    /// Upload bytes; the service stores them under their own hash and
    /// returns it. Idempotent.
    async fn put_blob(&self, bytes: &[u8]) -> Result<ContentHash, TransportError>;

    /// Fetch the tree manifest stored at a package hash.
    async fn get_tree(&self, package_hash: ContentHash) -> Result<TreeManifest, TransportError>;

    /// Upload a tree manifest; the service stores it under its package hash
    /// and returns it. Idempotent.
    async fn put_tree(&self, manifest: &TreeManifest) -> Result<ContentHash, TransportError>;
}

/// Why a call did not produce an answer.
///
/// Distinct from [`lpc_cloud_api::CloudError`] on purpose: this is "the
/// conversation did not happen", which is the retryable family. A
/// `CloudError` is the service having considered the request and said no.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// The service could not be reached. The offline case — queue and retry
    /// is the caller's loop, not this crate's.
    Offline,
    /// The service was reached but the answer could not be read (a truncated
    /// body, an unparseable frame, a blob that did not hash to its address).
    Protocol(String),
    /// The content plane does not hold this hash.
    MissingBlob(ContentHash),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Offline => f.write_str("offline"),
            TransportError::Protocol(detail) => write!(f, "protocol: {detail}"),
            TransportError::MissingBlob(hash) => {
                write!(f, "service has no blob {}", hash.short())
            }
        }
    }
}

/// Make one call and unwrap it to a typed response.
///
/// Folds the three failure shapes into [`SyncError`]: the transport not
/// reaching the service, the service running a different
/// [`CLOUD_API_VERSION`], and the service refusing the request. Version
/// mismatch is checked on the *reply* too, because a browser tab can sit
/// open across a service redeploy — the refusal is named, never guessed
/// around (see `lpc_cloud_api::version`).
pub async fn request<P: CloudPort + ?Sized>(
    port: &P,
    request: CloudRequest,
) -> Result<CloudResponse, SyncError> {
    let reply = port
        .call(CloudCall {
            version: CLOUD_API_VERSION,
            request,
        })
        .await?;
    lpc_cloud_api::check_version(reply.version)?;
    Ok(reply.result?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on::block_on;
    use lpc_cloud_api::CloudError;

    #[test]
    fn a_refusal_becomes_a_cloud_error() {
        let port = FakePort {
            reply: Ok(CloudReply {
                version: CLOUD_API_VERSION,
                result: Err(CloudError::NotFound),
            }),
        };
        let error = block_on(request(&port, CloudRequest::WhoAmI)).unwrap_err();
        assert!(matches!(error, SyncError::Cloud(CloudError::NotFound)));
    }

    #[test]
    fn an_unreachable_service_becomes_a_transport_error() {
        let port = FakePort {
            reply: Err(TransportError::Offline),
        };
        let error = block_on(request(&port, CloudRequest::WhoAmI)).unwrap_err();
        assert!(matches!(
            error,
            SyncError::Transport(TransportError::Offline)
        ));
    }

    /// A reply from a service running a different vocabulary is refused
    /// before its body is read.
    #[test]
    fn a_mismatched_reply_version_is_refused() {
        let port = FakePort {
            reply: Ok(CloudReply {
                version: CLOUD_API_VERSION + 1,
                result: Ok(CloudResponse::UserInfo {
                    actor: lpc_cloud_api::Actor::Anonymous,
                }),
            }),
        };
        let error = block_on(request(&port, CloudRequest::WhoAmI)).unwrap_err();
        assert!(matches!(
            error,
            SyncError::Cloud(CloudError::VersionMismatch { .. })
        ));
    }

    struct FakePort {
        reply: Result<CloudReply, TransportError>,
    }

    impl CloudPort for FakePort {
        async fn call(&self, _call: CloudCall) -> Result<CloudReply, TransportError> {
            self.reply.clone()
        }

        async fn get_blob(&self, hash: ContentHash) -> Result<Vec<u8>, TransportError> {
            Err(TransportError::MissingBlob(hash))
        }

        async fn put_blob(&self, bytes: &[u8]) -> Result<ContentHash, TransportError> {
            Ok(ContentHash::of(bytes))
        }

        async fn get_tree(
            &self,
            package_hash: ContentHash,
        ) -> Result<TreeManifest, TransportError> {
            Err(TransportError::MissingBlob(package_hash))
        }

        async fn put_tree(&self, manifest: &TreeManifest) -> Result<ContentHash, TransportError> {
            Ok(manifest.package_hash())
        }
    }
}
