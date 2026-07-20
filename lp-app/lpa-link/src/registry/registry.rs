use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::providers::{LinkConnector, LinkEnv, LinkProviderDescriptor, LinkProviderKind};
use crate::{LinkError, LinkProvider};

/// Catalog + factory for the link providers compiled into `lpa-link`.
///
/// The registry answers "which provider kinds exist in this build?" (the
/// catalog: [`Self::descriptors`], [`Self::kinds`], for picker UI) and hands
/// out an owned [`LinkConnector`] on demand ([`Self::create_connector`]) from
/// the per-kind options stored at construction ([`LinkEnv`]).
///
/// Connectors are SHARED per kind: the first `create_connector` call for a
/// kind constructs the provider, every later call returns the same `Rc`.
/// Provider-held endpoint state must survive across flows — the browser
/// serial provider mints an endpoint in `request_access` (one call) that the
/// subsequent connect flow (another call) has to find; a fresh instance per
/// call loses it ("link endpoint not found").
#[derive(Default)]
pub struct LinkProviderRegistry {
    env: LinkEnv,
    catalog: Vec<LinkProviderKind>,
    /// Preconfigured connectors, keyed by kind. Tests insert record-level or
    /// device-backed fakes here; `create_connector` returns the SAME shared
    /// instance for the kind so a test's scripted state survives re-opens.
    prebuilt: BTreeMap<LinkProviderKind, Rc<LinkConnector>>,
    /// Factory-built connectors, memoized per kind on first request so every
    /// flow sees one shared instance (interior mutability: the registry is
    /// owned by value and served through `&self`).
    built: RefCell<BTreeMap<LinkProviderKind, Rc<LinkConnector>>>,
}

impl LinkProviderRegistry {
    /// Create an empty registry for manual connector insertion.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct the default registry for the current build and target.
    ///
    /// Kinds are cataloged only when their crate feature and target
    /// conditions are satisfied. App-owned configuration is stored from `env`
    /// and consumed when `create_connector` builds a provider.
    pub fn from_env(env: LinkEnv) -> Self {
        let mut catalog = vec![LinkProviderKind::Fake];

        #[cfg(feature = "host-process")]
        catalog.push(LinkProviderKind::HostProcess);

        #[cfg(feature = "host-serial-esp32")]
        catalog.push(LinkProviderKind::HostSerialEsp32);

        #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
        catalog.push(LinkProviderKind::BrowserWorker);

        #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
        catalog.push(LinkProviderKind::BrowserSerialEsp32);

        catalog.sort();
        Self {
            env,
            catalog,
            prebuilt: BTreeMap::new(),
            built: RefCell::new(BTreeMap::new()),
        }
    }

    /// Insert or replace a preconfigured connector by its `LinkProviderKind`.
    ///
    /// This is the test-construction seam: record-level `FakeProvider`
    /// endpoints and scripted fake devices are built by the test and handed
    /// to the registry, which then serves them from `create_connector`.
    pub fn insert(&mut self, provider: impl Into<LinkConnector>) {
        let connector = provider.into();
        let kind = connector.kind();
        if !self.catalog.contains(&kind) {
            self.catalog.push(kind);
            self.catalog.sort();
        }
        self.prebuilt.insert(kind, Rc::new(connector));
    }

    /// Return the shared connector for a kind, building it on first request.
    ///
    /// A preconfigured kind returns its inserted instance; a factory-built
    /// kind is constructed ONCE from the stored `LinkEnv` and memoized, so
    /// endpoint state minted through one flow (e.g. browser serial
    /// `request_access`) is visible to every later flow. Kinds absent from
    /// this build's catalog are an error.
    pub fn create_connector(&self, kind: LinkProviderKind) -> Result<Rc<LinkConnector>, LinkError> {
        if let Some(connector) = self.prebuilt.get(&kind) {
            return Ok(Rc::clone(connector));
        }
        if !self.catalog.contains(&kind) {
            return Err(LinkError::other(format!(
                "provider {} is not available",
                kind.key()
            )));
        }
        if let Some(connector) = self.built.borrow().get(&kind) {
            return Ok(Rc::clone(connector));
        }
        let _ = &self.env;
        let connector = match kind {
            LinkProviderKind::Fake => {
                LinkConnector::Fake(crate::providers::fake::FakeProvider::new())
            }
            #[cfg(feature = "host-process")]
            LinkProviderKind::HostProcess => {
                let provider = crate::providers::host_process::HostProcessProvider::new();
                provider.create_memory_endpoint("Host process runtime");
                LinkConnector::HostProcess(provider)
            }
            #[cfg(feature = "host-serial-esp32")]
            LinkProviderKind::HostSerialEsp32 => LinkConnector::HostSerialEsp32(
                crate::providers::host_serial_esp32::HostSerialEsp32Provider::with_options(
                    self.env.host_serial_esp32.clone(),
                ),
            ),
            #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
            LinkProviderKind::BrowserWorker => {
                let provider =
                    crate::providers::browser_worker::BrowserWorkerProvider::with_options(
                        self.env.browser_worker.clone(),
                    );
                provider.create_worker_endpoint("Browser firmware runtime");
                LinkConnector::BrowserWorker(provider)
            }
            #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
            LinkProviderKind::BrowserSerialEsp32 => LinkConnector::BrowserSerialEsp32(
                crate::providers::browser_serial_esp32::BrowserSerialEsp32Provider::with_options(
                    self.env.browser_serial_esp32.clone(),
                ),
            ),
            #[allow(
                unreachable_patterns,
                reason = "kinds outside the feature/target matrix are caught by the catalog check above"
            )]
            _ => {
                return Err(LinkError::other(format!(
                    "provider {} is not available",
                    kind.key()
                )));
            }
        };
        let connector = Rc::new(connector);
        self.built.borrow_mut().insert(kind, Rc::clone(&connector));
        Ok(connector)
    }

    /// Return descriptors for all provider kinds in this registry's catalog.
    pub fn descriptors(&self) -> Vec<LinkProviderDescriptor> {
        self.catalog.iter().map(|kind| kind.descriptor()).collect()
    }

    /// Return all provider kinds in this registry's catalog.
    pub fn kinds(&self) -> Vec<LinkProviderKind> {
        self.catalog.clone()
    }
}

impl From<LinkEnv> for LinkProviderRegistry {
    fn from(env: LinkEnv) -> Self {
        Self::from_env(env)
    }
}

/// Convenience descriptor list for apps that only need provider metadata.
pub fn available_provider_descriptors() -> Vec<LinkProviderDescriptor> {
    LinkProviderRegistry::from_env(LinkEnv::default()).descriptors()
}

#[cfg(test)]
mod tests {
    use crate::providers::{LinkEnv, LinkProviderKind, LinkProviderRegistry};

    #[test]
    fn default_registry_includes_fake_provider() {
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());

        assert!(registry.kinds().contains(&LinkProviderKind::Fake));
        assert!(registry.create_connector(LinkProviderKind::Fake).is_ok());
    }

    #[cfg(feature = "host-process")]
    #[test]
    fn host_process_feature_adds_host_process_provider() {
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());

        assert!(registry.kinds().contains(&LinkProviderKind::HostProcess));
        assert!(
            registry
                .create_connector(LinkProviderKind::HostProcess)
                .is_ok()
        );
    }

    #[cfg(feature = "host-serial-esp32")]
    #[test]
    fn host_serial_feature_adds_host_serial_provider() {
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());

        assert!(
            registry
                .kinds()
                .contains(&LinkProviderKind::HostSerialEsp32)
        );
    }

    #[cfg(all(feature = "browser-worker", not(target_arch = "wasm32")))]
    #[test]
    fn browser_worker_feature_does_not_add_host_provider() {
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());

        assert!(!registry.kinds().contains(&LinkProviderKind::BrowserWorker));
        assert!(
            registry
                .create_connector(LinkProviderKind::BrowserWorker)
                .is_err()
        );
    }

    #[cfg(all(feature = "browser-serial-esp32", not(target_arch = "wasm32")))]
    #[test]
    fn browser_serial_feature_does_not_add_host_provider() {
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());

        assert!(
            !registry
                .kinds()
                .contains(&LinkProviderKind::BrowserSerialEsp32)
        );
    }

    #[test]
    fn preconfigured_connector_is_returned_shared() {
        use crate::providers::fake::FakeProvider;
        use crate::{LinkEndpoint, LinkProvider};

        let mut registry = LinkProviderRegistry::new();
        registry.insert(FakeProvider::new().with_endpoint(LinkEndpoint::new(
            "fake-runtime",
            LinkProviderKind::Fake,
            "Fake runtime",
        )));

        let first = registry.create_connector(LinkProviderKind::Fake).unwrap();
        let second = registry.create_connector(LinkProviderKind::Fake).unwrap();

        assert_eq!(first.kind(), LinkProviderKind::Fake);
        assert!(std::rc::Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn factory_built_connector_is_memoized_and_shared() {
        use crate::LinkProvider;

        // Regression: `request_access` and `connect_endpoint` run through
        // separate `create_connector` calls; both must land on ONE provider
        // instance or endpoints minted by the first call vanish.
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());

        let first = registry.create_connector(LinkProviderKind::Fake).unwrap();
        let second = registry.create_connector(LinkProviderKind::Fake).unwrap();

        assert_eq!(first.kind(), LinkProviderKind::Fake);
        assert!(std::rc::Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn provider_kind_uses_kebab_case_keys() {
        assert_eq!(
            LinkProviderKind::BrowserSerialEsp32.key(),
            "browser-serial-esp32"
        );
        assert_eq!(
            LinkProviderKind::from_key("host-process"),
            Some(LinkProviderKind::HostProcess)
        );
    }
}
