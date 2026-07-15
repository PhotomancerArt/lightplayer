//! Runtime tier selection (fidelity-tiers ADR).
//!
//! Tier is chosen exactly once, at runtime creation, and recorded: a `gpu`
//! request while the worker device is unavailable yields a CPU-tier runtime
//! with the reason attached — surfaced in the `runtime_created` worker
//! message, one structured log line, and the preview card badge. There is no
//! mid-flight tier switching and no retry loop.

/// Which shader-execution tier a runtime was created on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeTier {
    /// f32 on WebGPU via `lp-gfx-wgpu` (preview tier).
    Gpu,
    /// Q32 on `lpvm-wasm` (authoritative tier; the browser default).
    Cpu,
}

impl RuntimeTier {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Gpu => "gpu",
            Self::Cpu => "cpu",
        }
    }
}

/// The recorded outcome of tier selection at runtime creation.
#[derive(Clone, Debug)]
pub(crate) struct TierSelection {
    pub(crate) tier: RuntimeTier,
    /// Why a `gpu` request resolved to the CPU tier (`None` when the
    /// requested tier was granted).
    pub(crate) reason: Option<String>,
}

impl TierSelection {
    pub(crate) fn granted(tier: RuntimeTier) -> Self {
        Self { tier, reason: None }
    }

    pub(crate) fn cpu_because(reason: String) -> Self {
        Self {
            tier: RuntimeTier::Cpu,
            reason: Some(reason),
        }
    }
}
