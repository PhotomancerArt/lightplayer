//! Memory pressure: the engine-wide state-shedding contract.
//!
//! ## When pressure fires
//!
//! The engine broadcasts
//! [`NodeRuntime::handle_memory_pressure`](crate::node::NodeRuntime::handle_memory_pressure)
//! to every alive node at **safe points only** — moments where no render
//! borrow into any node-owned buffer is live. Today that is the top of a
//! tick, immediately before a **compile window** opens (a shader node
//! deferred a compile and requested the window; see
//! [`NodeRuntime::wants_compile_window`](crate::node::NodeRuntime::wants_compile_window)).
//! Embedders may also broadcast between ticks via
//! [`Engine::broadcast_memory_pressure`](crate::engine::Engine::broadcast_memory_pressure)
//! — for example from an allocation-failure retry hook, which runs OUTSIDE
//! the allocator lock. Pressure is never delivered from inside the allocator
//! or mid-render; see the ADR below for why those are hazards, not options.
//!
//! ## What a node may drop at each level
//!
//! - [`Low`](PressureLevel::Low) / [`Medium`](PressureLevel::Medium) —
//!   reserved; currently never broadcast. Treat as advisory.
//! - [`High`](PressureLevel::High) — a large allocation transient (a shader
//!   compile) runs this frame. Drop state that your own render path rebuilds
//!   **lazily and to identical core-path output** on the next demand — the
//!   `ensure_*` pattern in the fixture node is the model. After a High
//!   broadcast, drop → tick → output must be bit-identical to never having
//!   dropped on the core mapping/sample/render path (gravy features — dither,
//!   interpolation — are exempt per
//!   `docs/adr/2026-08-03-gravy-features-out-of-core-correctness-tests.md`).
//! - [`Critical`](PressureLevel::Critical) — survival: the device is about
//!   to OOM. A node may additionally drop **resettable** state whose loss is
//!   a visible discontinuity but not a correctness failure (the fluid
//!   solver's simulation grid). Never broadcast by the routine compile
//!   window.
//!
//! Never drop source-of-truth state at any level: authored/synced slot data,
//! resolved mappings, compiled shader programs (keep-last-good), or asset
//! text are inputs, not caches.
//!
//! ## The rebuild guarantee
//!
//! Anything dropped must be rebuilt on demand by the dropping node itself —
//! the engine schedules nothing. A node that drops state and then renders
//! stale or wrong output has broken the contract; silent invalidation is the
//! recurring failure class in this subsystem
//! (`docs/debt/s3-frame-cost-scales-per-fixture.md`), so rebuild seams must
//! fail loudly, not fall back quietly.
//!
//! Full decision record:
//! `docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md`.

/// Coarse memory pressure tier for runtime shedding decisions.
///
/// See the module docs for the per-level contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PressureLevel {
    /// Reserved; currently never broadcast.
    Low,
    /// Reserved; currently never broadcast.
    Medium,
    /// A large allocation transient runs this frame — drop lazily-rebuildable
    /// state (identical core-path output required on rebuild).
    High,
    /// Survival — may additionally drop resettable simulation state; visual
    /// discontinuity permitted.
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_level_total_ordering() {
        assert!(PressureLevel::Low < PressureLevel::Medium);
        assert!(PressureLevel::Medium < PressureLevel::High);
        assert!(PressureLevel::High < PressureLevel::Critical);
    }
}
