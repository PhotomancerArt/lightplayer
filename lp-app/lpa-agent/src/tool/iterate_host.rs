//! [`AgentHost`]: the seam between the agent and the surrounding app.
//!
//! Studio implements this (P5); tests and evals use stubs. The host owns the
//! shader's source of truth (overlay staging) and the fixture knowledge; the
//! agent core never touches project state directly.
//!
//! The write-side methods are async (boxed, `!Send` futures — the same
//! dyn-compatible shape as [`crate::provider::BoxStream`]): staging round-trips
//! through the host's command queue, and [`AgentHost::await_engine_verdict`]
//! waits (bounded) for the LIVE engine's post-stage status. The read-side
//! accessors stay synchronous — they serve host-owned snapshots.

use core::future::Future;
use core::pin::Pin;

use lps_probe::LedPoint;

/// A boxed, runtime-neutral, `!Send` host future (dyn-compatible; the whole
/// crate runs on single-threaded executors).
pub type HostFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Injected by the embedding app. The write surface is deliberately tiny:
/// the agent can only stage this one shader's source.
pub trait AgentHost {
    /// The shader source as the user currently sees it (including any
    /// staged-but-unsaved edits).
    fn current_source(&self) -> Result<String, HostError>;

    /// Stage new source as an unsaved overlay edit (the user can Save or
    /// revert it).
    fn stage_source<'a>(&'a mut self, source: &'a str) -> HostFuture<'a, Result<(), HostError>>;

    /// The live engine's verdict on the staged source, observed within
    /// `budget_ms` (polled on the host's timer). Semantics:
    ///
    /// - `budget_ms == 0`: report the last-known engine status immediately,
    ///   without waiting (the no-stage path).
    /// - otherwise: wait until the engine status is NEWER than the last
    ///   staged edit, or the budget elapses (then
    ///   [`EngineStatusKind::Unknown`] with a note).
    /// - `None`: this host has no live engine (tests, evals) — the iterate
    ///   result then carries no `engine` section.
    fn await_engine_verdict(&mut self, budget_ms: u32) -> HostFuture<'_, Option<EngineVerdict>> {
        let _ = budget_ms;
        Box::pin(async { None })
    }

    /// The def-side shader param records (the node def's `consumed` map),
    /// as the host last saw them. `None` = this host has no def knowledge
    /// (tests, evals) — the iterate result then carries no def/orphan
    /// diff in its `params` section.
    fn shader_params(&mut self) -> HostFuture<'_, Option<Vec<ParamDefRecord>>> {
        Box::pin(async { None })
    }

    /// Create or update one f32 param def record through the host's
    /// Save-gated overlay path (the `upsert_param` tool's write seam).
    /// Only the fields present in `upsert` are written.
    fn upsert_param<'a>(
        &'a mut self,
        upsert: &'a ParamUpsert,
    ) -> HostFuture<'a, Result<(), HostError>> {
        let _ = upsert;
        Box::pin(async { Err(HostError::new("this host cannot edit param records")) })
    }

    /// The target fixture's LED sample points (normalized coordinates).
    fn led_points(&self) -> Vec<LedPoint>;

    /// Context for the system prompt: bindings table, fixture summary,
    /// node/project names.
    fn shader_context(&self) -> ShaderContext;
}

/// The engine's status for the target shader node, as the host last saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineVerdict {
    pub status: EngineStatusKind,
    /// Human-readable status detail (error message, warning text, or a
    /// timeout note for `Unknown`).
    pub message: Option<String>,
    /// 1-based `(line, col)` when the engine error carries a source
    /// location.
    pub line_col: Option<(u32, u32)>,
}

impl EngineVerdict {
    /// An `Unknown` verdict carrying an explanatory note.
    pub fn unknown(note: impl Into<String>) -> Self {
        Self {
            status: EngineStatusKind::Unknown,
            message: Some(note.into()),
            line_col: None,
        }
    }
}

/// The engine status classification the `iterate` result reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineStatusKind {
    /// The node runs (warnings included; their text rides `message`).
    Ok,
    /// The node is in an error state (compile or render failure).
    Error,
    /// No fresh verdict was observed (engine still compiling, or status
    /// unavailable).
    Unknown,
}

impl EngineStatusKind {
    /// The wire/result string (`ok` / `error` / `unknown`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }
}

/// One def-side shader param record (a `consumed` map entry), as the host
/// reports it for the iterate `params` diff.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParamDefRecord {
    /// The uniform name (the `consumed` map key).
    pub name: String,
    /// Authored display label (empty = unlabeled).
    pub label: String,
    pub default: Option<f32>,
    pub min: Option<f32>,
    pub max: Option<f32>,
    /// Knob quantization: gestures snap to whole multiples of `step`
    /// (1 = an integer knob). Absent = a continuous knob.
    pub step: Option<f32>,
    /// Whether the record is flagged for the card's front panel (knob).
    pub panel: bool,
    /// Display unit suffix (e.g. "Hz"), when authored.
    pub unit: Option<String>,
    /// Whether the uniform is bound to a bus/producer (bus-driven at
    /// runtime; the authored default is then inert).
    pub bound: bool,
}

/// One `upsert_param` write: `name` is required, every other field is
/// written only when present (f32 params only in v1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParamUpsert {
    pub name: String,
    pub label: Option<String>,
    pub default: Option<f32>,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub step: Option<f32>,
    pub unit: Option<String>,
    pub panel: Option<bool>,
}

/// A host-side failure (project unavailable, write refused, ...). These are
/// the only failures reported as `is_error` tool results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostError {
    pub message: String,
}

impl HostError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// What the prompt builder knows about the shader being edited.
#[derive(Clone, Debug, Default)]
pub struct ShaderContext {
    pub project_name: String,
    pub node_name: String,
    /// The fixture this shader drives, when one is wired.
    pub fixture: Option<FixtureSummary>,
    /// Declared uniform bindings with their current values.
    pub bindings: Vec<BindingInfo>,
}

/// Fixture summary for the prompt.
#[derive(Clone, Debug)]
pub struct FixtureSummary {
    pub name: String,
    pub led_count: u32,
    /// Human-readable mapping kind (e.g. "2D grid", "strip").
    pub mapping_kind: String,
}

/// One declared uniform binding.
#[derive(Clone, Debug)]
pub struct BindingInfo {
    /// Uniform path (e.g. `time`, `cfg.speed`).
    pub name: String,
    /// GLSL type name (e.g. `float`, `vec3`).
    pub ty: String,
    /// Current value, human-formatted.
    pub value: String,
}
