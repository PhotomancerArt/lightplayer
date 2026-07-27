//! Anthropic streaming provider: `/v1/messages` wire types + provider.
//!
//! The HTTP/SSE framing layer (transport trait, SSE parser, platform
//! transports) lives one level up in [`crate::provider`] — it is shared
//! with the OpenAI-compatible provider.

pub mod anthropic_provider;
pub mod anthropic_wire;

pub use anthropic_provider::{
    ANTHROPIC_MAX_OUTPUT_TOKENS, ANTHROPIC_VERSION, AnthropicConfig, AnthropicProvider,
    DEFAULT_BASE_URL, DEFAULT_MODEL,
};
