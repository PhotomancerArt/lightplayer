//! [`AgentProvider`]: which model API the shader agent talks to, plus the
//! per-provider onboarding guidance (the no-dead-end copy).
//!
//! The guidance strings live here — core-side, in ONE place — because two
//! surfaces render them: the settings popover (under the provider selector)
//! and the Agent tab's needs-setup empty state.

use serde::{Deserialize, Serialize};

/// The selected model API family. `OpenAi` and `Custom` share the
/// OpenAI-compatible wire protocol; they differ only in defaults (base URL
/// baked vs. required, key required vs. optional).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProvider {
    /// Anthropic's `/v1/messages` API (the effective default).
    #[default]
    Anthropic,
    /// OpenAI's Chat Completions API at the official endpoint.
    #[serde(rename = "openai")]
    OpenAi,
    /// Any OpenAI-compatible server (Ollama, LM Studio, llama.cpp, vLLM)
    /// at a user-supplied base URL; API key optional.
    Custom,
    /// OpenRouter's OpenAI-compatible API; the key arrives via the OAuth
    /// PKCE Connect flow (charged to the user's own OpenRouter credits)
    /// rather than being pasted.
    #[serde(rename = "openrouter")]
    OpenRouter,
}

impl AgentProvider {
    /// Every provider, in selector order. OpenRouter leads: it's the
    /// recommended no-key-juggling path (the enum default stays Anthropic so
    /// existing installs keep their provider).
    pub const ALL: [AgentProvider; 4] = [
        AgentProvider::OpenRouter,
        AgentProvider::Anthropic,
        AgentProvider::OpenAi,
        AgentProvider::Custom,
    ];

    /// Short display label (the segmented selector's button text).
    pub fn label(self) -> &'static str {
        match self {
            AgentProvider::OpenRouter => "OpenRouter",
            AgentProvider::Anthropic => "Anthropic",
            AgentProvider::OpenAi => "OpenAI",
            AgentProvider::Custom => "Custom",
        }
    }
}

/// Onboarding copy for one provider: how to get from "selected it" to
/// "agent runs" without a dead end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentProviderGuidance {
    pub provider: AgentProvider,
    /// The provider's display label (mirrors [`AgentProvider::label`]).
    pub label: &'static str,
    /// Setup steps, one short paragraph.
    pub setup: &'static str,
    /// Links opened in a new tab: `(label, url)`.
    pub links: &'static [(&'static str, &'static str)],
    /// Extra caveat (e.g. the browser CORS note for local servers).
    pub note: Option<&'static str>,
}

/// The guidance for one provider.
pub fn provider_guidance(provider: AgentProvider) -> AgentProviderGuidance {
    match provider {
        AgentProvider::OpenRouter => AgentProviderGuidance {
            provider,
            label: provider.label(),
            setup: "Connect your OpenRouter account — one click, no key to \
                    paste. Usage is pay-as-you-go from your own OpenRouter \
                    credits, and every major model is available.",
            links: &[
                ("openrouter.ai/credits", "https://openrouter.ai/credits"),
                ("manage keys", "https://openrouter.ai/settings/keys"),
            ],
            note: Some(
                "Connect creates a key inside your OpenRouter account; you \
                 can revoke it there at any time.",
            ),
        },
        AgentProvider::Anthropic => AgentProviderGuidance {
            provider,
            label: provider.label(),
            setup: "Create an API key at platform.claude.com (Settings → API Keys) \
                    and add prepaid credits under Billing. Claude subscriptions \
                    don't include API access.",
            links: &[("platform.claude.com", "https://platform.claude.com/")],
            note: None,
        },
        AgentProvider::OpenAi => AgentProviderGuidance {
            provider,
            label: provider.label(),
            setup: "Create an API key at platform.openai.com (API keys) and set \
                    up billing. Enter the model id to use — OpenAI model ids are \
                    listed in its docs.",
            links: &[(
                "platform.openai.com",
                "https://platform.openai.com/api-keys",
            )],
            note: None,
        },
        AgentProvider::Custom => AgentProviderGuidance {
            provider,
            label: provider.label(),
            setup: "Any OpenAI-compatible server works — e.g. Ollama: run \
                    `ollama serve`, set the base URL to http://localhost:11434/v1, \
                    no key needed. LM Studio, llama.cpp, vLLM: use the base URL \
                    the server shows, and the model id it serves. Not sure? \
                    Press Find my server and Studio will look.",
            links: &[("ollama.com", "https://ollama.com/")],
            note: Some(
                "Browser note: the server must allow cross-origin requests — \
                 for Ollama set OLLAMA_ORIGINS to this app's origin (or *).",
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_names_are_stable() {
        for (provider, name) in [
            (AgentProvider::Anthropic, "\"anthropic\""),
            (AgentProvider::OpenAi, "\"openai\""),
            (AgentProvider::Custom, "\"custom\""),
            (AgentProvider::OpenRouter, "\"openrouter\""),
        ] {
            assert_eq!(serde_json::to_string(&provider).unwrap(), name);
            assert_eq!(
                serde_json::from_str::<AgentProvider>(name).unwrap(),
                provider
            );
        }
    }

    #[test]
    fn every_provider_has_nonempty_guidance() {
        for provider in AgentProvider::ALL {
            let guidance = provider_guidance(provider);
            assert_eq!(guidance.provider, provider);
            assert_eq!(guidance.label, provider.label());
            assert!(!guidance.setup.is_empty());
            assert!(!guidance.links.is_empty());
        }
    }

    #[test]
    fn custom_guidance_carries_the_cors_note() {
        let guidance = provider_guidance(AgentProvider::Custom);
        assert!(guidance.note.expect("note").contains("OLLAMA_ORIGINS"));
        assert!(guidance.setup.contains("localhost:11434/v1"));
    }
}
