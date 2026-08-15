//! Pure boot-wait policy for the browser worker: when has a boot gone
//! quiet for long enough to be declared dead?
//!
//! The worker posts a status envelope at every boot phase transition
//! (`booting` → `instantiating` → `gpu-init` → `runtime-create` →
//! `ready`), so a healthy boot is never silent for long — however slow
//! the machine or the network. The timeout is therefore INACTIVITY-based:
//! a boot fails only when no status change has been observed for the
//! current phase's budget, never on total elapsed time. A genuinely hung
//! phase still surfaces in seconds; a slow one just keeps reporting.
//!
//! This module is browser-free on purpose: it lives outside the wasm32
//! gate so its unit tests run in the native suite (the wasm half,
//! `worker_handle::boot`, only feeds it observations).

/// How the worker receives the fw-browser wasm (boot protocol v2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootDelivery {
    /// The page compiled the `WebAssembly.Module` once and posted it to
    /// the worker; the `instantiating` phase is CPU-only and quick.
    SharedModule,
    /// The worker fetches + compiles from the URL itself — the fallback
    /// when page-side compile failed. Its `instantiating` phase contains
    /// an unbounded network fetch, so it gets the generous budget.
    Path,
}

impl BootDelivery {
    /// The wire value carried in the boot envelope's `module_delivery`
    /// field (mirrored by `fw_browser_worker.js`).
    pub fn wire_value(self) -> &'static str {
        match self {
            BootDelivery::SharedModule => "message",
            BootDelivery::Path => "path",
        }
    }
}

/// Inactivity budget for an ordinary boot phase. Generous on purpose:
/// activity resets it, so it only has to outlast a genuinely busy phase
/// (a cold WebGPU device request, a large instantiate) on a slow machine
/// — a dead worker posts nothing and still fails in seconds of quiet.
pub const BOOT_PHASE_INACTIVITY_MS: f64 = 20_000.0;

/// Inactivity budget for the `instantiating` phase under [`BootDelivery::Path`],
/// which contains the worker-side wasm network fetch + compile. The fetch
/// emits no intermediate statuses, so this budget must cover the whole
/// download on a slow connection.
pub const BOOT_PATH_INSTANTIATE_INACTIVITY_MS: f64 = 120_000.0;

/// One expired boot wait: which phase went quiet and for how long.
#[derive(Clone, Debug, PartialEq)]
pub struct BootWaitExpired {
    /// Last observed status, or `None` when the worker never reported one.
    pub phase: Option<String>,
    /// The budget that lapsed, in milliseconds.
    pub budget_ms: f64,
}

/// Tracks boot-phase activity and answers "has this boot gone quiet for
/// longer than the current phase allows?".
#[derive(Clone, Debug)]
pub struct BootWaitClock {
    delivery: BootDelivery,
    last_status: Option<String>,
    idle_ms: f64,
}

impl BootWaitClock {
    pub fn new(delivery: BootDelivery) -> Self {
        Self {
            delivery,
            last_status: None,
            idle_ms: 0.0,
        }
    }

    /// Record a status envelope from the worker. A CHANGED status is
    /// activity and resets the idle clock; a repeat of the current status
    /// is not (the sticky fatal re-post must not keep a boot alive).
    pub fn observe_status(&mut self, status: &str) {
        if self.last_status.as_deref() != Some(status) {
            self.last_status = Some(status.to_string());
            self.idle_ms = 0.0;
        }
    }

    /// Account for quiet time between drains.
    pub fn advance(&mut self, elapsed_ms: f64) {
        self.idle_ms += elapsed_ms;
    }

    /// The inactivity budget the current phase is entitled to.
    pub fn budget_ms(&self) -> f64 {
        if self.delivery == BootDelivery::Path && self.last_status.as_deref() == Some("instantiating")
        {
            BOOT_PATH_INSTANTIATE_INACTIVITY_MS
        } else {
            BOOT_PHASE_INACTIVITY_MS
        }
    }

    /// `Some` once the current phase has been quiet past its budget.
    pub fn expired(&self) -> Option<BootWaitExpired> {
        let budget_ms = self.budget_ms();
        (self.idle_ms >= budget_ms).then(|| BootWaitExpired {
            phase: self.last_status.clone(),
            budget_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_boot_expires_after_the_phase_budget() {
        let mut clock = BootWaitClock::new(BootDelivery::SharedModule);
        clock.advance(BOOT_PHASE_INACTIVITY_MS - 1.0);
        assert_eq!(clock.expired(), None);
        clock.advance(1.0);
        let expired = clock.expired().expect("budget lapsed");
        assert_eq!(expired.phase, None);
        assert_eq!(expired.budget_ms, BOOT_PHASE_INACTIVITY_MS);
    }

    #[test]
    fn status_change_resets_the_idle_clock() {
        let mut clock = BootWaitClock::new(BootDelivery::SharedModule);
        clock.advance(BOOT_PHASE_INACTIVITY_MS - 1.0);
        clock.observe_status("booting");
        clock.advance(BOOT_PHASE_INACTIVITY_MS - 1.0);
        assert_eq!(clock.expired(), None, "each phase gets a fresh budget");
        clock.observe_status("instantiating");
        clock.advance(BOOT_PHASE_INACTIVITY_MS - 1.0);
        assert_eq!(clock.expired(), None);
    }

    #[test]
    fn repeated_status_is_not_activity() {
        let mut clock = BootWaitClock::new(BootDelivery::SharedModule);
        clock.observe_status("booting");
        clock.advance(BOOT_PHASE_INACTIVITY_MS / 2.0);
        clock.observe_status("booting");
        clock.advance(BOOT_PHASE_INACTIVITY_MS / 2.0);
        let expired = clock.expired().expect("a re-posted status must not extend the wait");
        assert_eq!(expired.phase.as_deref(), Some("booting"));
    }

    #[test]
    fn path_delivery_gets_the_generous_fetch_budget_only_while_instantiating() {
        let mut clock = BootWaitClock::new(BootDelivery::Path);
        clock.observe_status("booting");
        assert_eq!(clock.budget_ms(), BOOT_PHASE_INACTIVITY_MS);
        clock.observe_status("instantiating");
        assert_eq!(clock.budget_ms(), BOOT_PATH_INSTANTIATE_INACTIVITY_MS);
        clock.advance(BOOT_PHASE_INACTIVITY_MS + 1.0);
        assert_eq!(clock.expired(), None, "a slow fetch is not a hung boot");
        clock.observe_status("gpu-init");
        assert_eq!(clock.budget_ms(), BOOT_PHASE_INACTIVITY_MS);
    }

    #[test]
    fn shared_module_delivery_never_uses_the_fetch_budget() {
        let mut clock = BootWaitClock::new(BootDelivery::SharedModule);
        clock.observe_status("instantiating");
        assert_eq!(clock.budget_ms(), BOOT_PHASE_INACTIVITY_MS);
    }
}
