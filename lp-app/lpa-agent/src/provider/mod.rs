//! Model providers: the provider-neutral turn contract, the shared
//! HTTP/SSE framing layer, and the provider implementations.
//!
//! The framing layer ([`HttpSseTransport`], [`sse_parser`], the platform
//! transports) is provider-neutral: both the Anthropic and the
//! OpenAI-compatible providers stream `data:`-framed SSE over the same
//! transport seam; only the JSON payloads differ.

pub mod anthropic;
pub mod http_transport;
pub mod model_discovery;
pub mod model_provider;
pub mod openai_compat;
pub mod sse_parser;
#[cfg(all(not(target_arch = "wasm32"), feature = "host-transport"))]
pub mod transport_host;
#[cfg(target_arch = "wasm32")]
pub mod transport_web;

pub use http_transport::{
    HttpGetTransport, HttpRequest, HttpResponse, HttpSseTransport, TransportError,
};
pub use model_discovery::{
    ListModelsError, ModelInfo, list_anthropic_models, list_openai_compat_models,
};
pub use model_provider::{
    BoxStream, ChatMessage, ChatRole, ContentBlock, ModelProvider, StopReason, TokenUsage, ToolDef,
    TurnEvent, TurnRequest,
};
#[cfg(all(not(target_arch = "wasm32"), feature = "host-transport"))]
pub use transport_host::ReqwestTransport;
#[cfg(target_arch = "wasm32")]
pub use transport_web::WebFetchTransport;
