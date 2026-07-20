//! Errors from the lp-shader compilation and rendering pipeline.

use alloc::string::String;
use core::fmt;

/// Errors from the lp-shader compilation and rendering pipeline.
#[derive(Debug)]
pub enum LpsError {
    /// GLSL parse failure (naga frontend).
    Parse(String),
    /// LPIR lowering failure.
    Lower(String),
    /// Backend compilation failure.
    Compile(String),
    /// Render-time failure (trap, type mismatch, etc.).
    Render(String),
    /// The guest exhausted its per-invocation fuel tank (out-of-fuel trap).
    /// Structured so the engine can route it typed — no substring matching
    /// (see the lpvm-native fuel ADR).
    FuelExhausted(ShaderFuelTrap),
    /// Pixel shader contract validation failure (e.g. missing `render`,
    /// wrong signature, return type mismatch with output format).
    Validation(String),
}

impl fmt::Display for LpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LpsError::Parse(msg) => write!(f, "parse: {msg}"),
            LpsError::Lower(msg) => write!(f, "lower: {msg}"),
            LpsError::Compile(msg) => write!(f, "compile: {msg}"),
            LpsError::Render(msg) => write!(f, "render: {msg}"),
            LpsError::FuelExhausted(trap) => write!(f, "render: {trap}"),
            LpsError::Validation(msg) => write!(f, "validation: {msg}"),
        }
    }
}

impl core::error::Error for LpsError {}

/// Structured out-of-fuel diagnostic: which synthesised entry trapped and
/// where. Built in `px_shader` (the layer that knows the entry kind and the
/// texture width for deriving pixel coordinates from the linear invocation
/// index), threaded typed through `GfxError` to the engine's shader node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShaderFuelTrap {
    /// Entry point whose invocation exhausted its tank.
    pub entry: ShaderFuelTrapEntry,
    /// The per-invocation fuel budget that was exceeded, in loop back-edge
    /// units ([`lpvm::DEFAULT_INVOCATION_FUEL`]).
    pub budget: u32,
}

/// The trapping entry point, with host-derived invocation coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderFuelTrapEntry {
    /// Pixel `(x, y)` derived from the linear invocation index and the
    /// render-target width.
    RenderTexture { x: u32, y: u32 },
    /// Sample index of the trapping invocation.
    RenderSamples { sample: u32 },
}

impl fmt::Display for ShaderFuelTrap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.entry {
            ShaderFuelTrapEntry::RenderTexture { x, y } => write!(
                f,
                "shader fuel exhausted: render_texture pixel ({x}, {y}) exceeded {} iterations",
                self.budget
            ),
            ShaderFuelTrapEntry::RenderSamples { sample } => write!(
                f,
                "shader fuel exhausted: render_samples sample {sample} exceeded {} iterations",
                self.budget
            ),
        }
    }
}
