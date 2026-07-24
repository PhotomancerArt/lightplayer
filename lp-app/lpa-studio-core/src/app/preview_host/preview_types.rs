//! Request/status vocabulary for [`super::PreviewHost`] slot leases.
//!
//! Target-neutral: consumers (and native tests) can build requests and
//! reason about statuses without the browser-worker machinery compiled in.

/// Budgets and pool shape for one [`super::PreviewHost`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewHostConfig {
    /// Preview workers to boot. Two by default — an **isolation** choice
    /// (device loss is per-worker, so one hostile project takes down at
    /// most half the previews), not a CPU-parallelism one (the measured
    /// path is GPU-bound; see the preview-host ADR).
    pub pool_size: usize,
    /// Global cap on slots holding a live runtime (deploying counts).
    /// Leasing past the cap evicts the least-recently-used live slot,
    /// preferring invisible ones.
    pub max_live_slots: usize,
    /// Present cadence for slots whose request carries no fps override.
    pub default_fps: f32,
}

impl Default for PreviewHostConfig {
    fn default() -> Self {
        Self {
            pool_size: 2,
            max_live_slots: 12,
            default_fps: 12.0,
        }
    }
}

/// What a preview slot renders.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewSource {
    /// A compiled-in example package by id (e.g. `examples/fyeah-sign` —
    /// the id [`crate::UiExampleCard`] carries); materialized via
    /// [`super::example_deploy_files`].
    Example(String),
    /// A library package by `prj_…` uid (or slug), materialized from a
    /// library catalog snapshot via [`super::catalog_deploy_files`].
    ProjectUid(String),
}

/// Reserved seam for per-project preview behavior (preview-host ADR):
/// auto input playback (button presses), audio sources for music-reactive
/// programs, and eventually preview-mode authoring. Empty today; those
/// features land by adding fields here instead of reshaping the lease API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PreviewProfile {}

/// One slot lease request.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewSlotRequest {
    /// Content to deploy into the slot's runtime.
    pub source: PreviewSource,
    /// Element id of the consumer-owned `<canvas>` the preview renders to.
    /// The consumer owns mounting (and remounting: a GPU-tier canvas is
    /// permanently consumed by `transferControlToOffscreen`, so recovery
    /// after eviction or worker recycle needs a fresh element). The host
    /// fails the lease with a clear [`PreviewSlotStatus::Error`] when the
    /// canvas never mounts or was already transferred.
    pub canvas_id: String,
    /// Present cadence override; `None` uses
    /// [`PreviewHostConfig::default_fps`].
    pub fps: Option<f32>,
    /// Per-project preview behavior (reserved seam, empty today).
    pub profile: PreviewProfile,
}

/// Shader-execution tier a slot's runtime was granted.
///
/// Mirrors `lpa-link`'s browser-worker tier vocabulary without pulling the
/// wasm-only provider into native builds; the host maps the granted wire
/// tier onto this at runtime creation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewTier {
    /// Q32 on `lpvm-wasm` (authoritative tier; pixel frames blitted by the
    /// host via `putImageData`).
    Cpu,
    /// f32 on WebGPU, presenting straight to the transferred canvas
    /// surface (zero readback).
    Gpu,
}

/// Observable per-slot state (fidelity-tiers ADR: tier selection and every
/// failure surface here, never silently).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewSlotStatus {
    /// Queued or in the lease pipeline (create → deploy → attach).
    Deploying,
    /// Presenting at the slot's cadence.
    Live {
        /// Granted tier.
        tier: PreviewTier,
        /// Why a GPU request resolved to the CPU tier (`None` when the
        /// request was granted as asked).
        tier_reason: Option<String>,
    },
    /// Not presenting: hidden (`set_visible(false)`), LRU-evicted past the
    /// live-slot cap, or parked after a worker recycle while invisible.
    /// The canvas keeps its last frame.
    Suspended,
    /// The lease failed or the slot's runtime died; `reason` is the
    /// user-facing explanation.
    Error {
        /// What went wrong (deploy failure, canvas never mounted or
        /// already transferred, present error, worker loss, …).
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_match_the_adr() {
        let config = PreviewHostConfig::default();
        assert_eq!(config.pool_size, 2);
        assert_eq!(config.max_live_slots, 12);
        assert_eq!(config.default_fps, 12.0);
    }
}
