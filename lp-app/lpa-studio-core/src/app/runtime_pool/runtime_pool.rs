//! The keyed collection of runtime sessions plus the editor lens.
//!
//! ⚠️ Reduced to the SIM's needs by M2 of the device-model rebuild: the
//! DEVICE arm (four device sessions, the per-endpoint replace, the
//! kind-aware capacity policy, the id-targeted device seam and the
//! in-flight-operation refusal) went with the old device flows. The
//! rebuilt device model owns its own session collection; the pool keeps
//! the shape the sim uses — a keyed map plus the editor lens — so the sim
//! path is untouched (vision R1).

use std::collections::BTreeMap;

use crate::{RuntimeId, RuntimeSession, UiError};

use super::runtime_session::SimAttachment;

/// How many SIM sessions the pool admits at once (MVP policy; "+ new
/// simulator" raises this number, not the shape).
pub const SIM_SESSION_CAPACITY: usize = 1;

/// The studio's runtime sessions, keyed by [`RuntimeId`], plus the lens.
pub struct RuntimePool {
    sessions: BTreeMap<RuntimeId, RuntimeSession>,
    /// The session the editor is currently a lens on (D35: the editor is
    /// a lens on exactly one session). P3 lens semantics: `None` means the
    /// editor is detached — sessions keep running (worker + wire client)
    /// without a mirror. [`RuntimePool::install`] only claims the lens
    /// when nothing holds it (install observes; it never steals the lens
    /// from another session), and [`RuntimePool::detach_lens`] releases it
    /// without touching the sessions.
    lens: Option<RuntimeId>,
    next_id: u64,
}

impl RuntimePool {
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            lens: None,
            next_id: 0,
        }
    }

    /// Install a session around `payload`.
    ///
    /// Capacity (P2) is a POLICY — a number, not a shape: sessions beyond
    /// [`SIM_SESSION_CAPACITY`] are replaced, oldest first.
    ///
    /// Lens rule (P3): install preserves the lens unless none — attaching
    /// a runtime observes; it never steals the editor from a session the
    /// lens is on. The newcomer only claims the lens when nothing holds it
    /// (an empty pool, a detached editor, or a replace that just evicted
    /// the lens session — the replacement inherits the lens). Flows that
    /// deliberately move the editor call [`RuntimePool::set_lens`].
    pub fn install(&mut self, payload: SimAttachment) -> RuntimeId {
        let mut existing: Vec<RuntimeId> = self.sessions.keys().copied().collect();
        // Evict oldest-first until the newcomer fits under the capacity.
        while existing.len() + 1 > SIM_SESSION_CAPACITY {
            let oldest = existing.remove(0);
            self.remove(oldest);
        }
        let id = self.mint_id();
        self.sessions.insert(id, RuntimeSession::new(id, payload));
        if self.lens.is_none() {
            self.lens = Some(id);
        }
        id
    }

    /// Detach the editor lens (P3): the mirror's session binding drops,
    /// every session stays — worker running, wire client attached. The
    /// caller (`StudioController`) owns the mirror teardown
    /// (`project.reset()`); this only releases the id.
    pub(crate) fn detach_lens(&mut self) {
        self.lens = None;
    }

    /// Drop every session (and the lens) without closing payloads — the
    /// `RefreshConnections` recovery semantics. Absence = not in the pool.
    pub fn clear(&mut self) {
        self.sessions.clear();
        self.lens = None;
    }

    /// Take every session out of the pool (full attachment teardown); the
    /// caller closes the payloads.
    pub fn take_all_sessions(&mut self) -> Vec<RuntimeSession> {
        self.lens = None;
        core::mem::take(&mut self.sessions).into_values().collect()
    }

    /// Move the lens onto an existing session (P2: opening a project puts
    /// the lens on the reused sim session).
    pub(crate) fn set_lens(&mut self, id: RuntimeId) {
        if self.sessions.contains_key(&id) {
            self.lens = Some(id);
        }
    }

    pub fn has_session(&self) -> bool {
        !self.sessions.is_empty()
    }

    pub fn lens(&self) -> Option<RuntimeId> {
        self.lens
    }

    pub fn session(&self, id: RuntimeId) -> Option<&RuntimeSession> {
        self.sessions.get(&id)
    }

    pub fn session_mut(&mut self, id: RuntimeId) -> Option<&mut RuntimeSession> {
        self.sessions.get_mut(&id)
    }

    /// Every session in the pool, in id (installation) order.
    pub fn sessions(&self) -> impl Iterator<Item = &RuntimeSession> {
        self.sessions.values()
    }

    pub(crate) fn sessions_mut(&mut self) -> impl Iterator<Item = &mut RuntimeSession> {
        self.sessions.values_mut()
    }

    /// The session the editor lens is on, when there is one.
    pub fn lens_session(&self) -> Option<&RuntimeSession> {
        self.session(self.lens?)
    }

    /// The lens-bound resolution seam: every editor-mirror network op
    /// resolves its client through here. Errors with the same
    /// `MissingSession` surface the retired `ServerController::client_mut`
    /// reported, so call sites keep their error behavior unchanged.
    pub fn lens_session_mut(&mut self) -> Result<&mut RuntimeSession, UiError> {
        let id = self.lens.ok_or_else(missing_session)?;
        self.sessions.get_mut(&id).ok_or_else(missing_session)
    }

    /// The ≤1 SIM session.
    pub fn sim_session(&self) -> Option<&RuntimeSession> {
        self.sessions.values().next()
    }

    /// The ≤1 SIM session, mutably.
    pub fn sim_session_mut(&mut self) -> Option<&mut RuntimeSession> {
        self.sessions.values_mut().next()
    }

    /// Remove the sim session, if one is attached (stop-sim). The lens
    /// clears with it; the caller closes the payload.
    pub fn remove_sim(&mut self) -> Option<RuntimeSession> {
        let id = *self.sessions.keys().next()?;
        self.remove(id)
    }

    fn remove(&mut self, id: RuntimeId) -> Option<RuntimeSession> {
        if self.lens == Some(id) {
            self.lens = None;
        }
        self.sessions.remove(&id)
    }

    fn mint_id(&mut self) -> RuntimeId {
        self.next_id += 1;
        RuntimeId::new(self.next_id)
    }
}

impl Default for RuntimePool {
    fn default() -> Self {
        Self::new()
    }
}

fn missing_session() -> UiError {
    // The exact surface the retired `ServerController::client_mut` used for
    // "no client": a missing session means no client either way.
    UiError::MissingSession("server client is not connected".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_replaces_and_only_an_evicted_lens_moves() {
        let mut pool = RuntimePool::new();
        let first = pool.install(SimAttachment::stub_for_test());
        assert_eq!(
            pool.lens(),
            Some(first),
            "an empty pool's first session claims the lens"
        );
        let second = pool.install(SimAttachment::stub_for_test());

        assert_ne!(first, second, "ids are never reused");
        assert!(
            pool.session(first).is_none(),
            "attaching beyond capacity replaces (sim capacity is 1)"
        );
        assert_eq!(
            pool.lens(),
            Some(second),
            "evicting the lens session hands the lens to the replacement"
        );
        assert_eq!(pool.sessions().count(), 1);
        assert_eq!(pool.sim_session().map(RuntimeSession::id), Some(second));
    }

    #[test]
    fn detach_lens_keeps_the_session_and_reattach_resolves_again() {
        let mut pool = RuntimePool::new();
        let sim = pool.install(SimAttachment::stub_for_test());

        pool.detach_lens();

        // The lens is gone; the session stays in the pool untouched.
        assert_eq!(pool.lens(), None);
        assert!(pool.session(sim).is_some(), "the session survives");
        assert!(matches!(
            pool.lens_session_mut(),
            Err(UiError::MissingSession(_))
        ));

        // Re-attach: the lens resolves the chosen session again.
        pool.set_lens(sim);
        assert_eq!(pool.lens_session_mut().expect("lens resolves").id(), sim);

        // A detached editor lets the next install claim the lens.
        pool.detach_lens();
        let replacement = pool.install(SimAttachment::stub_for_test());
        assert_eq!(pool.lens(), Some(replacement));
    }

    #[test]
    fn remove_sim_clears_the_lens_with_its_session() {
        let mut pool = RuntimePool::new();
        assert!(pool.remove_sim().is_none());

        let sim = pool.install(SimAttachment::stub_for_test());
        let removed = pool.remove_sim().expect("the sim session");
        assert_eq!(removed.id(), sim);
        assert!(pool.lens().is_none(), "lens cleared with its session");
        assert!(!pool.has_session());
    }

    #[test]
    fn lens_seam_resolves_the_lens_session_or_reports_missing_session() {
        let mut pool = RuntimePool::new();
        assert!(matches!(
            pool.lens_session_mut(),
            Err(UiError::MissingSession(message))
                if message == "server client is not connected"
        ));

        let id = pool.install(SimAttachment::stub_for_test());
        let session = pool.lens_session_mut().expect("lens resolves");
        assert_eq!(session.id(), id);
        // No client attached yet: the client surface still reports the
        // retired MissingSession error.
        assert!(matches!(
            session.client_mut(),
            Err(UiError::MissingSession(message))
                if message == "server client is not connected"
        ));
    }

    #[test]
    fn take_all_sessions_empties_the_pool_and_the_lens() {
        let mut pool = RuntimePool::new();
        pool.install(SimAttachment::stub_for_test());

        let taken = pool.take_all_sessions();
        assert_eq!(taken.len(), 1);
        assert!(!pool.has_session());
        assert!(pool.lens().is_none());
        assert!(pool.take_all_sessions().is_empty());
    }
}
