//! What every handler shares: the service, the blob plane, the static site,
//! and the configuration.

use std::sync::{Arc, Mutex};

use lp_cloud_domain::{BlobStore, CloudService, MetaStore};
use lp_cloud_store_mem::{MemBlobStore, MemMetaStore};
use lp_cloud_store_sqlite::{FsBlobStore, SqliteMetaStore};
use lpc_cloud_api::Actor;
use lpc_history::ContentHash;

use crate::config::{BlobBackend, MetaBackend, ServerConfig};
use crate::page::static_site::StaticSite;
use crate::ports::{AnyBlobStore, AnyMetaStore, SecureMint, SystemClock};

/// The domain service plus the blob bytes it deliberately does not hold.
///
/// The two live behind **one** lock because an upload touches both: bytes to
/// the blob plane, then the hash into the meta store's blob index, which is
/// what push validation reads. Two locks would be two orders to get wrong,
/// for a service whose whole workload is a handful of requests a second.
pub struct ServiceCore {
    /// The domain service over whichever state backend was configured.
    pub service: CloudService<AnyMetaStore, SystemClock, SecureMint>,
    /// Content-addressed bytes: file blobs and the canonical tree preimages
    /// (see [`crate::content::tree_route`]).
    pub blobs: AnyBlobStore,
}

impl ServiceCore {
    /// Resolve a raw session token to a caller. No token, an unknown token,
    /// and an expired token are all [`Actor::Anonymous`] — plenty of
    /// requests are answerable without a session, and the domain decides
    /// which.
    pub fn actor_for(&self, token: Option<&[u8]>) -> Actor {
        match token {
            Some(token) => self.service.resolve_session(token),
            None => Actor::Anonymous,
        }
    }

    /// Store bytes and record them in the blob index, which is the edge's
    /// half of an upload. Returns the address the bytes actually landed at,
    /// which the caller compares against the one in the URL.
    pub fn store_blob(&mut self, bytes: &[u8]) -> ContentHash {
        let hash = self.blobs.put(bytes);
        self.service
            .store_mut()
            .record_blob(hash, bytes.len() as u64);
        hash
    }
}

/// Handler state: cloneable, cheap, and shared.
#[derive(Clone)]
pub struct AppState {
    core: Arc<Mutex<ServiceCore>>,
    site: Arc<StaticSite>,
    config: Arc<ServerConfig>,
}

impl AppState {
    /// Open the backends the configuration names and assemble the state.
    ///
    /// Blocking, deliberately: it runs once, before the listener exists.
    /// `main` still calls it inside `spawn_blocking`, because opening the
    /// S3 adapter builds that store's private tokio runtime.
    pub fn from_config(config: ServerConfig) -> Self {
        let meta = match config.meta {
            MetaBackend::Mem => AnyMetaStore::new(MemMetaStore::new()),
            MetaBackend::Sqlite => {
                let path = config.data_dir.join("cloud.sqlite");
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).expect("creating the data directory");
                }
                log::info!("meta store: sqlite at {}", path.display());
                AnyMetaStore::new(SqliteMetaStore::open(&path))
            }
        };

        let blobs = match config.blobs {
            BlobBackend::Mem => AnyBlobStore::new(MemBlobStore::new()),
            BlobBackend::Fs => {
                let root = config.data_dir.join("blobs");
                log::info!("blob store: files under {}", root.display());
                AnyBlobStore::new(FsBlobStore::open(&root))
            }
            BlobBackend::S3 => AnyBlobStore::new(open_s3(&config)),
        };

        let site = StaticSite::open(config.static_dir.as_deref());
        Self::new(config, meta, blobs, site)
    }

    /// Assemble state from parts already built — what the edge tests use to
    /// stand up a mem-backed app.
    pub fn new(
        config: ServerConfig,
        meta: AnyMetaStore,
        blobs: AnyBlobStore,
        site: StaticSite,
    ) -> Self {
        let login_providers = config.login_providers();
        Self {
            core: Arc::new(Mutex::new(ServiceCore {
                service: CloudService::new(meta, SystemClock, SecureMint)
                    .with_login_providers(login_providers),
                blobs,
            })),
            site: Arc::new(site),
            config: Arc::new(config),
        }
    }

    /// Run `work` against the service and the blob plane, off the async
    /// executor.
    ///
    /// **Every** handler reaches the stores this way, whatever backend is
    /// configured. Two reasons, and the first one alone would be enough:
    /// `S3BlobStore` owns a private current-thread tokio runtime whose
    /// `block_on` *panics* when called from a thread already running a
    /// runtime, and `SqliteMetaStore` blocks on a file, which is not
    /// something an executor thread should ever do. A handler that locks the
    /// mutex inline would work in `mem` mode and crash the first time
    /// somebody sets `LP_CLOUD_BLOBS=s3`, which is the worst possible place
    /// to find out.
    ///
    /// A panic inside `work` — the stores' fail-fast policy for a dead disk
    /// — is resumed on the handler's thread rather than swallowed: the
    /// request fails, the mutex is poisoned, and every subsequent request
    /// fails too. That is the intended shape of "this node is dead, restart
    /// it", and it is why nothing here tries to recover a poisoned lock.
    pub async fn with_service<T, F>(&self, work: F) -> T
    where
        F: FnOnce(&mut ServiceCore) -> T + Send + 'static,
        T: Send + 'static,
    {
        let core = Arc::clone(&self.core);
        match tokio::task::spawn_blocking(move || {
            let mut guard = core
                .lock()
                .expect("the store lock was poisoned by a fatal store failure");
            work(&mut guard)
        })
        .await
        {
            Ok(value) => value,
            Err(join_error) => match join_error.try_into_panic() {
                Ok(panic) => std::panic::resume_unwind(panic),
                Err(_) => panic!("lp-cloud-server: the store task was cancelled"),
            },
        }
    }

    /// The static app artifact.
    pub fn site(&self) -> &StaticSite {
        &self.site
    }

    /// The process configuration.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }
}

fn open_s3(config: &ServerConfig) -> lp_cloud_store_sqlite::S3BlobStore {
    let settings = &config.s3;
    let s3 = lp_cloud_store_sqlite::S3Config {
        bucket: settings
            .bucket
            .clone()
            .expect("LP_CLOUD_S3_BUCKET is checked while parsing the configuration"),
        endpoint: settings.endpoint.clone(),
        region: settings.region.clone(),
        access_key_id: settings.access_key_id.clone(),
        secret_access_key: settings.secret_access_key.clone(),
        allow_http: settings.allow_http,
    };
    log::info!("blob store: s3 bucket {}", s3.bucket);
    lp_cloud_store_sqlite::S3BlobStore::open(&s3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;
    use lpc_cloud_api::response::UserInfo;
    use lpc_cloud_api::{CloudRequest, CloudResponse};

    /// The wrapping is the point: a store call made from an async context
    /// has to end up on a blocking thread, and the value has to come back.
    #[tokio::test]
    async fn a_service_call_runs_off_the_executor() {
        let state = test_state();
        let answer = state
            .with_service(|core| core.service.handle(Actor::Anonymous, CloudRequest::WhoAmI))
            .await;

        assert_eq!(
            answer,
            Ok(CloudResponse::UserInfo(UserInfo {
                actor: Actor::Anonymous
            }))
        );
    }

    /// An upload has to land in the blob plane *and* the index, or the push
    /// that references it is refused as missing.
    #[tokio::test]
    async fn storing_a_blob_records_it_in_the_index() {
        let state = test_state();
        let hash = state.with_service(|core| core.store_blob(b"content")).await;

        assert_eq!(hash, ContentHash::of(b"content"));
        assert!(
            state
                .with_service(move |core| core.service.store().has_blob(hash))
                .await
        );
    }

    fn test_state() -> AppState {
        AppState::new(
            ServerConfig::from_vars(|_| None).expect("defaults parse"),
            AnyMetaStore::new(MemMetaStore::new()),
            AnyBlobStore::new(MemBlobStore::new()),
            StaticSite::open(None),
        )
    }
}
