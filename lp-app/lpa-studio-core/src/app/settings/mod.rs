//! Layered Studio settings.
//!
//! Small per-field overlays merged **user > host > baked default**:
//!
//! - **User layer** — the browser's `localStorage` (key `lp.settings.v1`),
//!   written whenever the user changes a setting in the UI.
//! - **Host layer** — a JSON [`StudioSettings`] document supplied by whatever
//!   hosts the app. This is a *channel contract*, not a dev-server hack: the
//!   dev server copies `~/.lightplayer/settings.json` to
//!   `/dev-settings.json`; an Electron shell later reads the same file and
//!   supplies the same JSON shape via IPC/preload; lightplayer.app simply has
//!   no host layer (the fetch 404s ⇒ absent).
//! - **Baked defaults** — compile-time fallbacks (e.g.
//!   [`DEFAULT_AGENT_MODEL`]).
//!
//! The store itself ([`SettingsStore`]) is pure state — sans-IO. The two IO
//! edges (the boot `dev-settings.json` fetch and the localStorage read/write)
//! live in the platform shell, which feeds the store through
//! [`SettingsCommand`]s and persists the user layer through the controller's
//! `on_user_settings` hook.

pub mod agent_models;
pub mod agent_provider;
pub mod settings_command;
pub mod settings_layer;
pub mod settings_store;
pub mod studio_settings;
pub mod ui_settings_view;

pub use agent_models::{AgentModelsFetch, AgentModelsState, discovery_fingerprint};
pub use agent_provider::{AgentProvider, AgentProviderGuidance, provider_guidance};
pub use settings_command::SettingsCommand;
pub use settings_layer::SettingsLayer;
pub use settings_store::SettingsStore;
pub use studio_settings::{AgentSettings, DEFAULT_AGENT_MODEL, StudioSettings};
pub use ui_settings_view::{UiAgentSettingsView, UiModelOption, UiSettingsView};
