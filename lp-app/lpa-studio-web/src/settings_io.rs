//! Browser IO edges for the layered settings store.
//!
//! The store itself is pure core state (`lpa_studio_core::SettingsStore`);
//! this module is where settings touch the platform:
//!
//! - **User layer** — JSON in `localStorage` under [`SETTINGS_STORAGE_KEY`],
//!   read synchronously at boot (before the actor spawns) and written by the
//!   controller's `on_user_settings` hook. Stored as plain text — the
//!   accepted v1 posture for the API key.
//! - **Host layer** — a same-origin `dev-settings.json` fetch. The dev
//!   server emits it from `~/.lightplayer/settings.json`; deployed builds
//!   404 (⇒ no host layer). The URL is relative so any port/path works. An
//!   Electron shell would replace this fetch with IPC/preload carrying the
//!   same JSON shape.

use lpa_studio_core::{AgentProvider, StudioSettings};

/// localStorage key holding the user settings layer (a `StudioSettings`
/// JSON document).
pub const SETTINGS_STORAGE_KEY: &str = "lp.settings.v1";

/// Host-layer URL, relative to the app's own origin/path.
const DEV_SETTINGS_URL: &str = "dev-settings.json";

/// Read the persisted user layer's JSON, if any. Any storage failure
/// (blocked storage, private mode) reads as "no stored settings".
pub fn load_user_settings_json() -> Option<String> {
    let storage = web_sys::window()?.local_storage().ok()??;
    storage.get_item(SETTINGS_STORAGE_KEY).ok()?
}

/// Persist the user layer's JSON (the controller `on_user_settings` hook).
/// Failures only warn: settings still apply for this session.
pub fn store_user_settings_json(json: &str) {
    let storage = web_sys::window().and_then(|window| window.local_storage().ok().flatten());
    let Some(storage) = storage else {
        log::warn!("settings not saved: localStorage is unavailable");
        return;
    };
    if let Err(error) = storage.set_item(SETTINGS_STORAGE_KEY, json) {
        log::warn!("settings not saved to localStorage: {error:?}");
    }
}

/// The effective API key for one provider (user layer > host layer), for
/// the connection test and the model picker.
///
/// The view DTO carries only a masked preview — deliberately, so no raw key
/// rides on a snapshot — so these two features read the value from the two
/// layers this edge already owns instead. The user layer is re-read from
/// localStorage on each call, which is exactly current: the controller
/// persists it synchronously as the user changes it.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, reason = "only the wasm probe/picker read a credential")
)]
pub fn effective_api_key(provider: AgentProvider) -> Option<String> {
    let field = |settings: &StudioSettings| match provider {
        AgentProvider::Anthropic => settings.agent.anthropic_api_key.clone(),
        AgentProvider::OpenAi => settings.agent.openai_api_key.clone(),
        AgentProvider::OpenRouter => settings.agent.openrouter_api_key.clone(),
        AgentProvider::Custom => settings.agent.custom_api_key.clone(),
    };
    let user = load_user_settings_json()
        .and_then(|json| StudioSettings::from_json_str(&json).ok())
        .as_ref()
        .and_then(field);
    user.or_else(|| HOST_LAYER.with_borrow(|host| host.as_ref().and_then(field)))
}

/// The Custom provider's key, for the local-server connection test.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, reason = "only the wasm probe reads a credential")
)]
pub fn effective_custom_api_key() -> Option<String> {
    effective_api_key(AgentProvider::Custom)
}

thread_local! {
    /// The host layer, kept for the credential lookup above (the store owns
    /// the authoritative copy; this is a read-only echo of the same fetch).
    static HOST_LAYER: std::cell::RefCell<Option<StudioSettings>> =
        const { std::cell::RefCell::new(None) };
}

/// Remember the fetched host layer for [`effective_custom_api_key`].
pub fn remember_host_layer(settings: &StudioSettings) {
    HOST_LAYER.with_borrow_mut(|host| *host = Some(settings.clone()));
}

/// Fetch the host-provided settings layer. A 404 or network error means the
/// host supplies no layer (the deployed-site case) and resolves to `None`
/// silently; an unreadable document also resolves to `None` but logs one
/// warning, since a present-but-broken `~/.lightplayer/settings.json` is
/// worth surfacing.
pub async fn fetch_dev_settings() -> Option<StudioSettings> {
    let response = gloo_net::http::Request::get(DEV_SETTINGS_URL)
        .send()
        .await
        .ok()?;
    if !response.ok() {
        return None;
    }
    let text = response.text().await.ok()?;
    match StudioSettings::from_json_str(&text) {
        Ok(settings) => Some(settings),
        Err(error) => {
            log::warn!("dev-settings.json ignored (unreadable): {error}");
            None
        }
    }
}
