//! Model-facing shader agent core.
//!
//! An edge crate (`lpa-*`): async is allowed, but the session/tool logic is
//! runtime-neutral — no spawning, no executor-flavored sleeps. Platform code
//! lives only in the transports (web-sys fetch on wasm; reqwest behind the
//! `host-transport` feature).
//!
//! Layers:
//!
//! - [`provider`]: the [`ModelProvider`] one-turn streaming contract, the
//!   shared HTTP/SSE framing layer, and two implementations — Anthropic
//!   (`/v1/messages`) and OpenAI-compatible (`/chat/completions`, which
//!   also covers Ollama/LM Studio/llama.cpp/vLLM via `base_url`). The
//!   trait is deliberately small so a future in-web local model can
//!   implement it.
//! - [`session`]: the agentic loop ([`AgentSession`]) — model turn → tool
//!   execution via the injected [`AgentHost`] → repeat — plus the transcript
//!   and UI-facing [`AgentEvent`]s.
//! - [`tool`]: the single `iterate` tool, dispatching into `lps-probe`.
//! - [`prompt`]: system prompt assembly from [`ShaderContext`] + the builtin
//!   reference generated from `lps-builtins`.
//!
//! No Studio/UI/settings code here; Studio implements [`AgentHost`] and maps
//! its settings into [`AnthropicConfig`] (P5).

pub mod prompt;
pub mod provider;
pub mod session;
pub mod tool;

pub use prompt::build_system_prompt;
pub use provider::anthropic::{AnthropicConfig, AnthropicProvider};
pub use provider::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};
pub use provider::{
    BoxStream, ChatMessage, ChatRole, ContentBlock, HttpGetTransport, HttpSseTransport,
    ListModelsError, ModelInfo, ModelPrice, ModelProvider, StopReason, TokenUsage, ToolDef,
    TurnEvent, TurnRequest, list_anthropic_models, list_openai_compat_models,
};
pub use session::{AgentError, AgentEvent, AgentSession, AgentTranscript, MAX_TURNS_PER_RUN};
pub use tool::{
    AgentHost, BindingInfo, DECLARE_SPACE_TOOL_NAME, DeclaredSpace, ENGINE_VERDICT_BUDGET_MS,
    EngineStatusKind, EngineVerdict, FixtureSummary, HostError, HostFuture, ITERATE_TOOL_NAME,
    ParamDefRecord, ParamUpsert, ProjectionShapeTag, ShaderContext, SpaceDeclaration, ToolPhase,
    UPSERT_PARAM_TOOL_NAME, declare_space_tool_def, entry_point, iterate_tool_def,
    run_declare_space, run_iterate, run_upsert_param, upsert_param_tool_def,
};
