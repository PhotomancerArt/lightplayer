//! Settings inputs riding the studio actor's command queue.

use lpa_agent::{ListModelsError, ModelInfo};

use crate::app::settings::StudioSettings;
use crate::app::settings::agent_provider::AgentProvider;

/// A settings mutation or layer load. Applied synchronously by the actor
/// (like console commands): pure state changes on the
/// [`SettingsStore`](super::SettingsStore), no async work — except
/// [`SettingsCommand::RequestModels`], which may spawn a model-list fetch
/// through the agent sub-controller's platform seam (the fetch reports
/// back as [`SettingsCommand::ModelsLoaded`]). Every user mutation is
/// normalized in the store (trimmed; empty ⇒ clear) and re-persists the
/// user layer through the controller's `on_user_settings` hook.
#[derive(Clone, Debug)]
pub enum SettingsCommand {
    /// The host channel's document arrived (the boot `dev-settings.json`
    /// fetch, or a future Electron preload). Absent host layers simply
    /// never send this.
    HostLayerLoaded(StudioSettings),
    /// The persisted user layer was read back (normally installed
    /// synchronously before the actor spawns; this command covers late
    /// loads and tests).
    UserLayerLoaded(StudioSettings),
    /// Set or clear the user's provider selection.
    SetAgentProvider(Option<AgentProvider>),
    /// Set or clear the user's Anthropic API-key override.
    SetAgentAnthropicApiKey(Option<String>),
    /// Set or clear the user's OpenAI API-key override.
    SetAgentOpenAiApiKey(Option<String>),
    /// Set or clear the user's custom-server base URL override.
    SetAgentCustomBaseUrl(Option<String>),
    /// Set or clear the user's custom-server API-key override.
    SetAgentCustomApiKey(Option<String>),
    /// Set or clear the OpenRouter key (written by the Connect flow's
    /// exchange; `None` = Disconnect).
    SetAgentOpenRouterApiKey(Option<String>),
    /// Set or clear the user's model override.
    SetAgentModel(Option<String>),
    /// Set or clear the $/MTok input-rate override from its text-field
    /// value (parsed in the store; junk clears).
    SetAgentPriceInputPerMtok(Option<String>),
    /// Set or clear the $/MTok output-rate override.
    SetAgentPriceOutputPerMtok(Option<String>),
    /// Fetch (or, with `force`, re-fetch) the selected provider's model
    /// list for the settings dropdown. Sent when the settings surface
    /// opens (its refresh affordance forces); credential changes trigger
    /// the same path controller-side. Debounced against the stored
    /// fingerprint: a fetch already in flight — or already resolved for
    /// the same credentials — is not repeated unless forced.
    RequestModels { force: bool },
    /// A spawned model-list fetch resolved. `fingerprint` identifies the
    /// credentials the fetch used; a response that no longer matches the
    /// store's current entry is dropped as stale.
    ModelsLoaded {
        provider: AgentProvider,
        fingerprint: String,
        result: Result<Vec<ModelInfo>, ListModelsError>,
    },
}
