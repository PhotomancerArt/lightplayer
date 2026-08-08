//! The transport that is the service: no network, no serialization, no mock.

use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use lp_cloud_domain::{
    BlobStore as _, Caller, Clock as _, CloudService, DevPickerConnection, LoginProviders,
    MetaStore as _, SESSION_TOKEN_LEN, session_token_hash,
};
use lp_cloud_store_mem::{MemBlobStore, MemClock, MemIdMint, MemMetaStore};
use lpc_cloud_api::{Actor, CLOUD_API_VERSION, CloudCall, CloudReply, check_version};
use lpc_history::{ContentHash, PrefixedUid, TreeManifest};

use crate::cloud_port::{CloudPort, TransportError};

/// How long a session minted here lasts. Long enough that no test has to
/// think about expiry; short enough that it is obviously a value, not
/// "forever".
pub const SESSION_TTL_SECONDS: f64 = 30.0 * 24.0 * 60.0 * 60.0;

/// The service, in this process.
///
/// This is the real [`CloudService`] over the real in-memory adapters (D13),
/// plus the two things a server edge owns and the domain deliberately does
/// not: the blob bytes, and the session/OAuth dance (here reduced to
/// [`sign_in`](Self::sign_in), which is what the Google callback does once it
/// has an identity).
///
/// Shared through an [`Rc`], because the interesting scenarios are two
/// clients talking to one service.
pub struct InProcessServer {
    service: RefCell<CloudService<MemMetaStore, MemClock, MemIdMint>>,
    blobs: RefCell<MemBlobStore>,
    /// Tree manifests, addressed by **package** hash — see [`CloudPort`] for
    /// why that is a different address from the manifest JSON's own hash.
    trees: RefCell<BTreeMap<ContentHash, TreeManifest>>,
}

impl InProcessServer {
    /// A service whose clock starts at `start_time` (f64 epoch seconds).
    ///
    /// The dev picker is on unconditionally: this transport stands in for a
    /// local dev server (never production, which speaks over `FetchCloudPort`
    /// instead), so a client test exercising `LoginOptions`'s dev-picker
    /// branch should not have to opt in separately.
    pub fn new(start_time: f64) -> Rc<Self> {
        Rc::new(Self {
            service: RefCell::new(
                CloudService::new(
                    MemMetaStore::new(),
                    MemClock::new(start_time),
                    MemIdMint::new(),
                )
                .with_login_providers(LoginProviders {
                    oidc: Vec::new(),
                    dev_picker: Some(DevPickerConnection {
                        start_path: "/auth/dev".to_string(),
                    }),
                }),
            ),
            blobs: RefCell::new(MemBlobStore::new()),
            trees: RefCell::new(BTreeMap::new()),
        })
    }

    /// Log somebody in, minting the account on first sight.
    ///
    /// The edge's job, not the domain's: everything an OAuth callback does
    /// after the provider hands back an identity. Always `"google"` —
    /// this stands in for the Google callback specifically (see the doc
    /// above), never the dev picker.
    pub fn sign_in(&self, google_sub: &str, email: &str, display_name: &str) -> SignedIn {
        let mut service = self.service.borrow_mut();
        let user = service.upsert_user(google_sub, email, display_name, "google", None, None, None);
        let token = service.open_session(user.uid, SESSION_TTL_SECONDS, None);
        SignedIn {
            user: user.uid,
            token,
        }
    }

    /// The service's clock reading.
    pub fn now(&self) -> f64 {
        self.service.borrow().clock().now()
    }

    /// Move the service's clock (session expiry is otherwise unreachable
    /// without waiting for real time).
    pub fn advance_clock(&self, seconds: f64) {
        self.service.borrow().clock().advance(seconds);
    }

    /// How many file blobs the content plane holds.
    pub fn blob_count(&self) -> usize {
        self.blobs.borrow().len()
    }

    /// How many tree manifests the content plane holds.
    pub fn tree_count(&self) -> usize {
        self.trees.borrow().len()
    }

    /// The service itself, for a test that wants to assert on state no
    /// request exposes.
    pub fn service(&self) -> &RefCell<CloudService<MemMetaStore, MemClock, MemIdMint>> {
        &self.service
    }
}

/// The result of a login: who, and the bearer token that proves it.
#[derive(Debug, Clone, Copy)]
pub struct SignedIn {
    /// The account's uid.
    pub user: PrefixedUid,
    /// The raw session token. Handed out exactly once; only its hash is
    /// stored.
    pub token: [u8; SESSION_TOKEN_LEN],
}

/// One client's connection to an [`InProcessServer`].
///
/// Carries **one session**, which is what makes it a client rather than a
/// handle on the server: two `InProcessCloud`s over the same `Rc` are two
/// different people, and an anonymous one is a person with no account. Every
/// call resolves its own [`Actor`] from its own token, exactly as the HTTP
/// edge will resolve one from a cookie.
///
/// # The offline switch
///
/// [`go_offline`](Self::go_offline) makes every call fail with
/// [`TransportError::Offline`] — the service is untouched, as if the network
/// were gone. That is the only fake thing about this transport, and it exists
/// because "push is never blocked" (D5) is a claim about the *client's* queue
/// as much as the server's frontier: the way to prove local work continues
/// while pushes fail is to make pushes fail.
pub struct InProcessCloud {
    server: Rc<InProcessServer>,
    session: Option<[u8; SESSION_TOKEN_LEN]>,
    offline: Cell<bool>,
}

impl InProcessCloud {
    /// A client with no account.
    pub fn anonymous(server: Rc<InProcessServer>) -> Self {
        Self {
            server,
            session: None,
            offline: Cell::new(false),
        }
    }

    /// A client holding an existing session.
    pub fn with_session(server: Rc<InProcessServer>, session: SignedIn) -> Self {
        Self {
            server,
            session: Some(session.token),
            offline: Cell::new(false),
        }
    }

    /// Log in and return the client already holding the session.
    pub fn sign_in(
        server: Rc<InProcessServer>,
        google_sub: &str,
        email: &str,
        display_name: &str,
    ) -> (Self, SignedIn) {
        let session = server.sign_in(google_sub, email, display_name);
        (Self::with_session(server, session), session)
    }

    /// The service this client talks to.
    pub fn server(&self) -> &Rc<InProcessServer> {
        &self.server
    }

    /// Who the service resolves this client as right now.
    pub fn actor(&self) -> Actor {
        match &self.session {
            Some(token) => self.server.service.borrow().resolve_session(token),
            None => Actor::Anonymous,
        }
    }

    /// Cut the client off from the service. Nothing on the service changes.
    pub fn go_offline(&self) {
        self.offline.set(true);
    }

    /// Reconnect.
    pub fn go_online(&self) {
        self.offline.set(false);
    }

    /// Whether this client is currently cut off.
    pub fn is_offline(&self) -> bool {
        self.offline.get()
    }

    fn reachable(&self) -> Result<(), TransportError> {
        if self.offline.get() {
            Err(TransportError::Offline)
        } else {
            Ok(())
        }
    }

    /// Record a blob in the service's index. The edge's half of an upload:
    /// bytes to the blob plane, hash to the meta store, which is what push
    /// validation reads.
    fn record_blob(&self, hash: ContentHash, size: usize) {
        self.server
            .service
            .borrow_mut()
            .store_mut()
            .record_blob(hash, size as u64);
    }

    fn blob_bytes(&self, hash: ContentHash) -> Result<Vec<u8>, TransportError> {
        self.server
            .blobs
            .borrow()
            .get(hash)
            .ok_or(TransportError::MissingBlob(hash))
    }
}

impl CloudPort for InProcessCloud {
    async fn call(&self, call: CloudCall) -> Result<CloudReply, TransportError> {
        self.reachable()?;
        // The version check is the edge's, not the domain's: a call from a
        // client speaking another vocabulary is refused before the request
        // is looked at.
        let result = match check_version(call.version) {
            Ok(()) => {
                let actor = self.actor();
                // This client knows its own raw token, unlike a browser's
                // HttpOnly cookie — hashing it here is exactly what the HTTP
                // edge does from the cookie header (`api_route.rs`).
                let session = self.session.as_ref().map(|token| session_token_hash(token));
                self.server
                    .service
                    .borrow_mut()
                    .handle(Caller { actor, session }, call.request)
            }
            Err(mismatch) => Err(mismatch),
        };
        Ok(CloudReply {
            version: CLOUD_API_VERSION,
            result,
        })
    }

    async fn get_blob(&self, hash: ContentHash) -> Result<Vec<u8>, TransportError> {
        self.reachable()?;
        self.blob_bytes(hash)
    }

    async fn put_blob(&self, bytes: &[u8]) -> Result<ContentHash, TransportError> {
        self.reachable()?;
        let hash = self.server.blobs.borrow_mut().put(bytes);
        self.record_blob(hash, bytes.len());
        Ok(hash)
    }

    async fn get_tree(&self, package_hash: ContentHash) -> Result<TreeManifest, TransportError> {
        self.reachable()?;
        self.server
            .trees
            .borrow()
            .get(&package_hash)
            .cloned()
            .ok_or(TransportError::MissingBlob(package_hash))
    }

    async fn put_tree(&self, manifest: &TreeManifest) -> Result<ContentHash, TransportError> {
        self.reachable()?;
        // Content-addressed means recomputed, never taken on trust: the
        // address is derived here, from the manifest itself.
        let hash = manifest.package_hash();
        let size = serde_json::to_vec(manifest)
            .map_err(|e| TransportError::Protocol(e.to_string()))?
            .len();
        self.server
            .trees
            .borrow_mut()
            .insert(hash, manifest.clone());
        self.record_blob(hash, size);
        Ok(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on::block_on;
    use crate::cloud_port::call;
    use crate::sync_error::SyncError;
    use lpc_cloud_api::request::{GetProject, HaveBlobs, PublishProject, WhoAmI};
    use lpc_cloud_api::{CloudError, CloudRequest, Visibility};
    use lpc_history::{TreeEntry, UidPrefix};
    use lpfs::LpPathBuf;

    #[test]
    fn each_client_is_its_own_caller() {
        let server = InProcessServer::new(1000.0);
        let (yona, signed_in) = InProcessCloud::sign_in(server.clone(), "sub-1", "y@x.dev", "Yona");
        let stranger = InProcessCloud::anonymous(server.clone());

        assert_eq!(yona.actor(), Actor::User(signed_in.user));
        assert_eq!(stranger.actor(), Actor::Anonymous);

        let who = block_on(call(&yona, WhoAmI)).unwrap();
        assert_eq!(who.actor, Actor::User(signed_in.user));
    }

    /// The service is not a mock: an anonymous caller hits the real access
    /// rules, and a private project is `NotFound` rather than `NotAuthorized`.
    #[test]
    fn the_real_access_rules_apply() {
        let server = InProcessServer::new(1000.0);
        let (yona, _) = InProcessCloud::sign_in(server.clone(), "sub-1", "y@x.dev", "Yona");
        let stranger = InProcessCloud::anonymous(server.clone());
        let uid = PrefixedUid::mint(UidPrefix::Project, &[3u8; 16]);

        block_on(call(
            &yona,
            PublishProject {
                uid,
                visibility: Visibility::Private,
                slug: "zook-dome".into(),
            },
        ))
        .unwrap();

        let refused = block_on(call(&stranger, GetProject { uid })).unwrap_err();
        assert!(matches!(refused, SyncError::Cloud(CloudError::NotFound)));
    }

    #[test]
    fn going_offline_fails_every_plane_and_touches_nothing() {
        let server = InProcessServer::new(1000.0);
        let (yona, _) = InProcessCloud::sign_in(server.clone(), "sub-1", "y@x.dev", "Yona");

        let hash = block_on(yona.put_blob(b"content")).unwrap();
        yona.go_offline();

        assert!(matches!(
            block_on(call(&yona, WhoAmI)),
            Err(SyncError::Transport(TransportError::Offline))
        ));
        assert_eq!(
            block_on(yona.put_blob(b"more")),
            Err(TransportError::Offline)
        );
        assert_eq!(block_on(yona.get_blob(hash)), Err(TransportError::Offline));
        // the service kept everything it had
        assert_eq!(server.blob_count(), 1);

        yona.go_online();
        assert_eq!(block_on(yona.get_blob(hash)).unwrap(), b"content".to_vec());
    }

    /// A tree is addressed by its package hash, which is *not* the hash of
    /// the JSON that carries it — the whole reason `put_tree` exists.
    #[test]
    fn a_tree_is_addressed_by_its_package_hash() {
        let server = InProcessServer::new(1000.0);
        let (yona, _) = InProcessCloud::sign_in(server.clone(), "sub-1", "y@x.dev", "Yona");

        let manifest = TreeManifest::from_entries(alloc::vec![TreeEntry {
            path: LpPathBuf::from("/project.json"),
            hash: ContentHash::of(b"{}"),
        }])
        .unwrap();
        let json_hash = ContentHash::of(&serde_json::to_vec(&manifest).unwrap());

        let stored = block_on(yona.put_tree(&manifest)).unwrap();
        assert_eq!(stored, manifest.package_hash());
        assert_ne!(stored, json_hash);
        assert_eq!(block_on(yona.get_tree(stored)).unwrap(), manifest);
    }

    /// An uploaded blob has to reach the *index* too, or a push that
    /// references it is refused as missing.
    #[test]
    fn uploading_records_the_blob_in_the_index() {
        let server = InProcessServer::new(1000.0);
        let (yona, _) = InProcessCloud::sign_in(server.clone(), "sub-1", "y@x.dev", "Yona");
        let hash = block_on(yona.put_blob(b"content")).unwrap();

        let missing = block_on(call(
            &yona,
            HaveBlobs {
                hashes: alloc::vec![hash, ContentHash::of(b"absent")],
            },
        ))
        .unwrap();
        assert_eq!(missing.hashes, alloc::vec![ContentHash::of(b"absent")]);
    }

    #[test]
    fn a_client_speaking_another_vocabulary_is_refused() {
        let server = InProcessServer::new(1000.0);
        let (yona, _) = InProcessCloud::sign_in(server.clone(), "sub-1", "y@x.dev", "Yona");
        let reply = block_on(yona.call(CloudCall {
            version: CLOUD_API_VERSION + 1,
            request: CloudRequest::WhoAmI,
        }))
        .unwrap();
        assert!(matches!(
            reply.result,
            Err(CloudError::VersionMismatch { .. })
        ));
    }
}
