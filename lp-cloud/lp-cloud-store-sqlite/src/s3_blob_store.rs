//! Blob bytes in S3-compatible object storage (Tigris).
//!
//! Same layout as [`FsBlobStore`](crate::fs_blob_store::FsBlobStore) — see
//! [`crate::blob_layout`] — so a deployment can be seeded from a directory
//! by copying it, key for key.
//!
//! # The async bridge, and why it is a tokio runtime
//!
//! [`BlobStore`] is a synchronous port and `object_store` is an async
//! crate, so something has to bridge them. The choice made here is a
//! **private current-thread tokio runtime owned by the store**, with one
//! `block_on` per operation.
//!
//! The smaller-looking options do not work:
//!
//! - A hand-rolled `block_on` that polls the future to completion drives
//!   nothing but the future itself. `object_store`'s S3 backend is built on
//!   reqwest/hyper, whose sockets and timers are registered with a tokio
//!   *reactor*; with no reactor running, the first poll returns `Pending`
//!   and is never woken. It would not be a smaller bridge, it would be a
//!   hang.
//! - There is no sync API on `object_store` to use instead — the trait is
//!   async all the way down.
//! - Growing an async variant of the `BlobStore` port would push `async fn`
//!   into a domain-facing trait for the benefit of one adapter, and the
//!   filesystem adapter (the default) would gain nothing from it.
//!
//! So the tokio runtime is the smallest honest mechanism: it is confined to
//! this file, it is created once per store rather than per call, and it
//! leaves the port synchronous for everybody else.
//!
//! ## The one rule for callers
//!
//! **Do not call these methods from inside an async task.** `block_on`
//! panics when it is called on a thread that is already running a tokio
//! runtime. A server edge that is itself async must reach this store
//! through `tokio::task::spawn_blocking`, which is what it should be doing
//! with a blocking store anyway.
//!
//! # What failure does
//!
//! Nothing here returns a `Result`: see [`crate::store_fatal`]. A missing
//! object is `None` (that is an answer, not a failure); anything else —
//! credentials, network, a bucket that is not there — is fatal, and the
//! supervisor restarts us.
//!
//! # Testing
//!
//! There is no local test of this adapter, deliberately: a hand-written
//! fake of S3 would only prove that the fake matches this code. It is
//! exercised in P11 as a smoke test against real Tigris, and the conformance
//! suite runs against the filesystem adapter, which shares the layout.

use std::sync::Arc;

use lp_cloud_domain::BlobStore;
use lpc_history::ContentHash;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload};
use tokio::runtime::{Builder as RuntimeBuilder, Runtime};

use crate::blob_layout::blob_object_path;
use crate::store_fatal::fatal;

/// What it takes to reach a bucket.
///
/// Anything left `None` falls back to the standard AWS environment
/// variables (`AWS_ACCESS_KEY_ID`, `AWS_ENDPOINT`, `AWS_REGION`, …), which
/// is how a deployment supplies credentials without them passing through
/// our own config file.
#[derive(Debug, Clone, Default)]
pub struct S3Config {
    /// Bucket name. Required.
    pub bucket: String,
    /// Endpoint URL, for S3-compatible providers such as Tigris. Leave
    /// unset for AWS itself.
    pub endpoint: Option<String>,
    /// Region. Tigris accepts `auto`.
    pub region: Option<String>,
    /// Access key id, if not coming from the environment.
    pub access_key_id: Option<String>,
    /// Secret access key, if not coming from the environment.
    pub secret_access_key: Option<String>,
    /// Allow a plain-HTTP endpoint. For a local MinIO, never for a real
    /// provider — credentials travel on every request.
    pub allow_http: bool,
}

/// Content-addressed blob storage in an S3-compatible bucket.
#[derive(Debug)]
pub struct S3BlobStore {
    store: Arc<dyn ObjectStore>,
    runtime: Runtime,
}

impl S3BlobStore {
    /// Connect to the bucket described by `config`.
    pub fn open(config: &S3Config) -> Self {
        let mut builder = AmazonS3Builder::from_env()
            .with_bucket_name(&config.bucket)
            .with_allow_http(config.allow_http);
        if let Some(endpoint) = &config.endpoint {
            builder = builder.with_endpoint(endpoint);
        }
        if let Some(region) = &config.region {
            builder = builder.with_region(region);
        }
        if let Some(key_id) = &config.access_key_id {
            builder = builder.with_access_key_id(key_id);
        }
        if let Some(secret) = &config.secret_access_key {
            builder = builder.with_secret_access_key(secret);
        }

        let store = fatal(
            &format!("connecting to the bucket {}", config.bucket),
            builder.build(),
        );
        Self::with_object_store(Arc::new(store))
    }

    /// Wrap an already-configured object store.
    ///
    /// The seam a P11 smoke test (or a local MinIO) plugs into without
    /// going through [`S3Config`].
    pub fn with_object_store(store: Arc<dyn ObjectStore>) -> Self {
        let runtime = fatal(
            "starting the blob-store runtime",
            RuntimeBuilder::new_current_thread().enable_all().build(),
        );
        Self { store, runtime }
    }

    /// Where this blob lives in the bucket.
    fn path_for(&self, hash: ContentHash) -> ObjectPath {
        ObjectPath::from(blob_object_path(hash))
    }
}

impl BlobStore for S3BlobStore {
    fn has(&self, hash: ContentHash) -> bool {
        let path = self.path_for(hash);
        let result = self.runtime.block_on(self.store.head(&path));
        match result {
            Ok(_) => true,
            Err(object_store::Error::NotFound { .. }) => false,
            Err(error) => panic!("lp-cloud-store-sqlite: heading blob {path} failed: {error}"),
        }
    }

    fn get(&self, hash: ContentHash) -> Option<Vec<u8>> {
        let path = self.path_for(hash);
        let result = self.runtime.block_on(async {
            match self.store.get(&path).await {
                Ok(response) => response.bytes().await.map(Some),
                Err(object_store::Error::NotFound { .. }) => Ok(None),
                Err(error) => Err(error),
            }
        });
        match result {
            Ok(bytes) => bytes.map(|bytes| bytes.to_vec()),
            Err(error) => panic!("lp-cloud-store-sqlite: reading blob {path} failed: {error}"),
        }
    }

    fn put(&mut self, bytes: &[u8]) -> ContentHash {
        let hash = ContentHash::of(bytes);
        let path = self.path_for(hash);
        // Idempotent without a preflight check: the key is the content, so
        // re-uploading writes the same bytes to the same place. A `head`
        // first would trade a round trip for nothing.
        let payload = PutPayload::from(bytes.to_vec());
        fatal(
            &format!("storing blob {path}"),
            self.runtime.block_on(self.store.put(&path, payload)),
        );
        hash
    }
}
