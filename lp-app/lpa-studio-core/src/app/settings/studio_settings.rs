//! The serialized settings shape shared by every layer.
//!
//! One JSON document (`{ "agent": { … } }`) is used verbatim by all three
//! sources: `~/.lightplayer/settings.json` (emitted as `/dev-settings.json`
//! by the dev server, or supplied by a future Electron host), the browser's
//! localStorage user layer, and the in-memory overlays inside
//! [`SettingsStore`](super::SettingsStore). Every field is optional so a
//! layer only speaks for the fields it sets.

use serde::{Deserialize, Serialize};

use crate::app::settings::agent_provider::AgentProvider;

/// The effective agent model when no layer overrides it — Anthropic only;
/// the OpenAI/Custom providers have no default model (ids are theirs to
/// name, never guessed here).
pub const DEFAULT_AGENT_MODEL: &str = "claude-sonnet-5";

/// OpenRouter's API origin (browser-CORS-open, OpenAI-compatible).
pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// The effective model for OpenRouter when no layer overrides it, so the
/// Connect flow alone makes the agent ready. Slug verified live against
/// `GET https://openrouter.ai/api/v1/models` on 2026-07-26.
pub const DEFAULT_OPENROUTER_MODEL: &str = "anthropic/claude-sonnet-5";

/// One settings overlay: every field optional, absent fields defer to the
/// layer below.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StudioSettings {
    pub agent: AgentSettings,
}

/// Agent (shader-assistant) settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSettings {
    /// Which provider the agent talks to. Effective default: Anthropic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    /// Anthropic API key. No baked default exists; effective `None` means
    /// the Anthropic provider cannot run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anthropic_api_key: Option<String>,
    /// OpenAI API key (the `OpenAi` provider).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    /// Base URL of an OpenAI-compatible server (the `Custom` provider),
    /// e.g. `http://localhost:11434/v1` for Ollama.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_base_url: Option<String>,
    /// Optional API key for the `Custom` provider (local servers usually
    /// need none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_api_key: Option<String>,
    /// OpenRouter key, normally written by the OAuth PKCE Connect flow
    /// (it lives in the user's OpenRouter account and is revocable there).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openrouter_api_key: Option<String>,
    /// Model id. Effective default is [`DEFAULT_AGENT_MODEL`] for
    /// Anthropic; REQUIRED (no default) for OpenAI/Custom.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Cost-estimate rate override, $ per million input tokens. Wins over
    /// the built-in pricing table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_input_per_mtok: Option<f64>,
    /// Cost-estimate rate override, $ per million output tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_output_per_mtok: Option<f64>,
}

impl StudioSettings {
    /// Parse one layer from its JSON document. Callers (the platform IO
    /// edges) decide how to surface a parse error; the shape itself is
    /// lenient — unknown fields are ignored, missing fields default.
    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize this layer for persistence (the localStorage user layer).
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_document() {
        let settings = StudioSettings::from_json_str(
            r#"{"agent":{
                "provider":"custom",
                "anthropic_api_key":"sk-ant-x",
                "openai_api_key":"sk-oai-x",
                "custom_base_url":"http://localhost:11434/v1",
                "custom_api_key":"local-key",
                "model":"m",
                "price_input_per_mtok":3.0,
                "price_output_per_mtok":15.0
            }}"#,
        )
        .unwrap();
        assert_eq!(settings.agent.provider, Some(AgentProvider::Custom));
        assert_eq!(
            settings.agent.anthropic_api_key.as_deref(),
            Some("sk-ant-x")
        );
        assert_eq!(settings.agent.openai_api_key.as_deref(), Some("sk-oai-x"));
        assert_eq!(
            settings.agent.custom_base_url.as_deref(),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(settings.agent.custom_api_key.as_deref(), Some("local-key"));
        assert_eq!(settings.agent.model.as_deref(), Some("m"));
        assert_eq!(settings.agent.price_input_per_mtok, Some(3.0));
        assert_eq!(settings.agent.price_output_per_mtok, Some(15.0));
    }

    #[test]
    fn pre_provider_documents_still_parse() {
        // The P4-era shape (key + model only) keeps working unchanged.
        let settings = StudioSettings::from_json_str(
            r#"{"agent":{"anthropic_api_key":"sk-ant-x","model":"m"}}"#,
        )
        .unwrap();
        assert_eq!(settings.agent.provider, None);
        assert_eq!(
            settings.agent.anthropic_api_key.as_deref(),
            Some("sk-ant-x")
        );
        assert_eq!(settings.agent.model.as_deref(), Some("m"));
    }

    #[test]
    fn missing_and_unknown_fields_are_tolerated() {
        let settings = StudioSettings::from_json_str(r#"{"agent":{"future":1}}"#).unwrap();
        assert_eq!(settings, StudioSettings::default());
        let settings = StudioSettings::from_json_str("{}").unwrap();
        assert_eq!(settings, StudioSettings::default());
    }

    #[test]
    fn round_trips_and_skips_absent_fields() {
        let settings = StudioSettings {
            agent: AgentSettings {
                provider: Some(AgentProvider::OpenAi),
                model: Some("claude-sonnet-5".to_string()),
                ..AgentSettings::default()
            },
        };
        let json = settings.to_json_string();
        assert!(!json.contains("anthropic_api_key"));
        assert!(!json.contains("custom_base_url"));
        assert!(json.contains("\"provider\":\"openai\""));
        assert_eq!(StudioSettings::from_json_str(&json).unwrap(), settings);
    }
}
