use std::collections::BTreeMap;

use crate::LinkProvider;
use crate::providers::{LinkEnv, LinkProviderDescriptor, LinkProviderInstance, LinkProviderKind};

/// Runtime collection of provider implementations compiled into `lpa-link`.
///
/// The registry owns provider instances keyed by `LinkProviderKind`. It is the
/// high-level entry point for applications that want to enumerate available
/// providers from the same feature/target matrix that compiled `lpa-link`,
/// without duplicating provider availability logic in the application crate.
#[derive(Default)]
pub struct LinkProviderRegistry {
    providers: BTreeMap<LinkProviderKind, LinkProviderInstance>,
}

impl LinkProviderRegistry {
    /// Create an empty registry for manual provider insertion.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct the default registry for the current build and target.
    ///
    /// Providers are inserted only when their crate feature and target
    /// conditions are satisfied. App-owned configuration is read from `env`
    /// through feature-gated fields.
    pub fn from_env(env: LinkEnv) -> Self {
        let mut registry = Self::new();
        let _ = &env;

        registry.insert(crate::providers::fake::FakeProvider::new());

        #[cfg(feature = "host-process")]
        {
            let mut provider = crate::providers::host_process::HostProcessProvider::new();
            provider.create_memory_endpoint("Host process runtime");
            registry.insert(provider);
        }

        #[cfg(feature = "host-serial-esp32")]
        registry.insert(
            crate::providers::host_serial_esp32::HostSerialEsp32Provider::with_options(
                env.host_serial_esp32,
            ),
        );

        #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
        {
            let mut provider =
                crate::providers::browser_worker::BrowserWorkerProvider::with_options(
                    env.browser_worker,
                );
            provider.create_worker_endpoint("Browser firmware runtime");
            registry.insert(provider);
        }

        #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
        registry.insert(
            crate::providers::browser_serial_esp32::BrowserSerialEsp32Provider::with_options(
                env.browser_serial_esp32,
            ),
        );

        registry
    }

    /// Insert or replace a provider by its `LinkProviderKind`.
    pub fn insert(&mut self, provider: impl Into<LinkProviderInstance>) {
        let provider = provider.into();
        self.providers.insert(provider.kind(), provider);
    }

    /// Iterate over provider instances in key order.
    pub fn providers(&self) -> impl Iterator<Item = &LinkProviderInstance> {
        self.providers.values()
    }

    /// Mutably iterate over provider instances in key order.
    pub fn providers_mut(&mut self) -> impl Iterator<Item = &mut LinkProviderInstance> {
        self.providers.values_mut()
    }

    /// Return the provider for a kind, if it is available in this registry.
    pub fn provider(&self, kind: LinkProviderKind) -> Option<&LinkProviderInstance> {
        self.providers.get(&kind)
    }

    /// Return the mutable provider for a kind, if it is available in this registry.
    pub fn provider_mut(&mut self, kind: LinkProviderKind) -> Option<&mut LinkProviderInstance> {
        self.providers.get_mut(&kind)
    }

    /// Return descriptors for all providers currently owned by the registry.
    pub fn descriptors(&self) -> Vec<LinkProviderDescriptor> {
        self.providers()
            .map(LinkProviderInstance::descriptor)
            .collect()
    }

    /// Return all provider kinds currently owned by the registry.
    pub fn kinds(&self) -> Vec<LinkProviderKind> {
        self.providers.keys().copied().collect()
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

        assert!(registry.provider(LinkProviderKind::Fake).is_some());
    }

    #[cfg(feature = "host-process")]
    #[test]
    fn host_process_feature_adds_host_process_provider() {
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());

        assert!(registry.provider(LinkProviderKind::HostProcess).is_some());
    }

    #[cfg(feature = "host-serial-esp32")]
    #[test]
    fn host_serial_feature_adds_host_serial_provider() {
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());

        assert!(
            registry
                .provider(LinkProviderKind::HostSerialEsp32)
                .is_some()
        );
    }

    #[cfg(all(feature = "browser-worker", not(target_arch = "wasm32")))]
    #[test]
    fn browser_worker_feature_does_not_add_host_provider() {
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());

        assert!(registry.provider(LinkProviderKind::BrowserWorker).is_none());
    }

    #[cfg(all(feature = "browser-serial-esp32", not(target_arch = "wasm32")))]
    #[test]
    fn browser_serial_feature_does_not_add_host_provider() {
        let registry = LinkProviderRegistry::from_env(LinkEnv::default());

        assert!(
            registry
                .provider(LinkProviderKind::BrowserSerialEsp32)
                .is_none()
        );
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
