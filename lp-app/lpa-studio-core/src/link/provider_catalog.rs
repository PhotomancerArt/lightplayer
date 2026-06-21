use crate::{ProviderAvailability, ProviderCardState, ProviderIntent};
use lpa_link::link_endpoint::LinkEndpointId;
use lpa_link::link_provider::LinkProviderId;
use lpa_link::LinkEndpoint;
use serde::{Deserialize, Serialize};

/// Collection of provider profiles and the user's current provider selection.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProviderCatalog {
    pub selected_provider_id: Option<LinkProviderId>,
    pub providers: Vec<ProviderCardState>,
}

impl ProviderCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_providers(providers: Vec<ProviderCardState>) -> Self {
        Self {
            selected_provider_id: None,
            providers,
        }
    }

    pub fn selected_provider_id(&self) -> Option<&LinkProviderId> {
        self.selected_provider_id.as_ref()
    }

    pub fn select_provider(&mut self, provider_id: impl Into<LinkProviderId>) {
        let provider_id = provider_id.into();
        self.ensure_provider(provider_id.clone());
        self.selected_provider_id = Some(provider_id);
    }

    pub fn clear_selection(&mut self) {
        self.selected_provider_id = None;
    }

    pub fn set_providers(&mut self, providers: Vec<ProviderCardState>) {
        self.providers = providers;
        if let Some(selected) = &self.selected_provider_id {
            if self.provider(selected).is_none() {
                self.selected_provider_id = None;
            }
        }
    }

    pub fn upsert_provider(&mut self, provider: ProviderCardState) {
        if let Some(existing) = self
            .providers
            .iter_mut()
            .find(|entry| entry.provider_id == provider.provider_id)
        {
            *existing = provider;
        } else {
            self.providers.push(provider);
        }
    }

    pub fn provider(&self, provider_id: &LinkProviderId) -> Option<&ProviderCardState> {
        self.providers
            .iter()
            .find(|entry| entry.provider_id == *provider_id)
    }

    pub fn provider_mut(&mut self, provider_id: &LinkProviderId) -> Option<&mut ProviderCardState> {
        self.providers
            .iter_mut()
            .find(|entry| entry.provider_id == *provider_id)
    }

    pub fn selected_provider(&self) -> Option<&ProviderCardState> {
        self.selected_provider_id
            .as_ref()
            .and_then(|provider_id| self.provider(provider_id))
    }

    pub fn selected_provider_endpoints(&self) -> &[LinkEndpoint] {
        self.selected_provider()
            .map(|provider| provider.endpoints.as_slice())
            .unwrap_or(&[])
    }

    pub fn first_selected_endpoint(&self) -> Option<&LinkEndpoint> {
        self.selected_provider_endpoints().first()
    }

    pub fn set_provider_endpoints(
        &mut self,
        provider_id: LinkProviderId,
        endpoints: Vec<LinkEndpoint>,
    ) {
        self.ensure_provider(provider_id.clone());
        if let Some(provider) = self.provider_mut(&provider_id) {
            provider.endpoints = endpoints;
        }
    }

    pub fn set_provider_availability(
        &mut self,
        provider_id: LinkProviderId,
        availability: ProviderAvailability,
    ) {
        self.ensure_provider(provider_id.clone());
        if let Some(provider) = self.provider_mut(&provider_id) {
            provider.availability = availability;
        }
    }

    pub fn endpoint(&self, endpoint_id: &LinkEndpointId) -> Option<&LinkEndpoint> {
        self.providers
            .iter()
            .flat_map(|provider| provider.endpoints.iter())
            .find(|endpoint| endpoint.id == *endpoint_id)
    }

    fn ensure_provider(&mut self, provider_id: LinkProviderId) {
        if self.provider(&provider_id).is_none() {
            self.providers.push(ProviderCardState::new(
                provider_id.clone(),
                provider_id.as_str(),
                ProviderIntent::Other {
                    label: provider_id.as_str().to_string(),
                },
            ));
        }
    }
}
