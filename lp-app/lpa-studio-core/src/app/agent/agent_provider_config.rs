//! [`AgentProviderConfig`]: the resolved connection settings handed to the
//! provider factory at each run start.
//!
//! The factory (installed by the platform shell) matches on the variant and
//! constructs the corresponding `lpa-agent` provider over its transport.
//! Rebuilt from effective settings per run, so key/model/provider changes
//! apply without restarting sessions.

use lpa_agent::{AnthropicConfig, OpenAiCompatConfig};

/// Connection settings for one agent run, resolved from the settings store
/// by [`SettingsStore::agent_provider_config`]
/// (`crate::app::settings::SettingsStore`). `None` from that resolver means
/// the selected provider is not sufficiently configured (the agent shows
/// its setup guidance instead).
#[derive(Clone, Debug)]
pub enum AgentProviderConfig {
    /// Anthropic `/v1/messages`.
    Anthropic(AnthropicConfig),
    /// OpenAI or any OpenAI-compatible server (`OpenAi`/`Custom`
    /// providers — same wire protocol, different base URL/key rules).
    OpenAiCompat(OpenAiCompatConfig),
}
