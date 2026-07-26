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

use lpa_studio_core::StudioSettings;

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
