//! OpenAI-compatible streaming provider: Chat Completions wire types +
//! provider. Covers OpenAI itself and local/OSS servers (Ollama, LM
//! Studio, llama.cpp, vLLM) via `base_url`; shares the HTTP/SSE framing
//! layer in [`crate::provider`] with the Anthropic provider.

pub mod openai_compat_provider;
pub mod openai_compat_wire;

pub use openai_compat_provider::{
    COMPAT_MAX_COMPLETION_TOKENS, DEFAULT_BASE_URL, OpenAiCompatConfig, OpenAiCompatProvider,
};
