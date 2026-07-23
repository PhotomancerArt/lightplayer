//! The keyed collection of runtime sessions plus the editor lens.
//!
//! P1 scope (session extraction): the pool holds AT MOST ONE session and
//! [`RuntimePool::install`] still replaces — the capacity POLICY (1 sim +
//! 1 device coexisting) arrives with P2. What P1 establishes is the shape
//! and the two named resolution seams every network op goes through:
//!
//! - **Lens-bound** ops (the editor mirror) resolve
//!   [`RuntimePool::lens_session_mut`] — the session the editor is a lens
//!   on.
//! - **Session-targeted** ops (device flows, deploy, reconcile) resolve
//!   [`RuntimePool::device_session_mut`] — named separately so P2+ can
//!   target a specific session among several.
//!
//! In P1 both seams resolve the same sole session, exactly matching the
//! retired single-slot behavior where every op used the one
//! `ServerController` client regardless of attachment kind.

use std::collections::BTreeMap;

use crate::{RuntimeId, RuntimeSession, UiError};

use super::runtime_session::RuntimePayload;

/// The studio's runtime sessions, keyed by [`RuntimeId`], plus the lens.
pub struct RuntimePool {
    sessions: BTreeMap<RuntimeId, RuntimeSession>,
    /// The session the editor is currently a lens on (D35: the editor is
    /// a lens on exactly one session). In P1 this is always the sole
    /// session; detach semantics arrive with P3.
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

    /// Install a session around `payload` and put the lens on it.
    ///
    /// P1: attach still replaces — any existing session is dropped
    /// (matching the retired single-slot eviction). The capacity policy
    /// that lets a sim and a device coexist is P2.
    pub fn install(&mut self, payload: RuntimePayload) -> RuntimeId {
        self.sessions.clear();
        let id = self.mint_id();
        self.sessions.insert(id, RuntimeSession::new(id, payload));
        self.lens = Some(id);
        id
    }

    /// Drop every session (and the lens). The retired behavior of
    /// `attachment = RuntimeAttachment::None`: absence = not in the pool.
    pub fn clear(&mut self) {
        self.sessions.clear();
        self.lens = None;
    }

    /// Take the sole session out of the pool (attachment teardown). P1:
    /// there is at most one; the caller closes its payload.
    pub fn take_sole_session(&mut self) -> Option<RuntimeSession> {
        let id = self.sessions.keys().next().copied()?;
        self.lens = None;
        self.sessions.remove(&id)
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

    /// The ≤1 DEVICE-kind session's evidence view (roster/deploy
    /// derivations): kind-filtered, like the retired
    /// `RuntimeAttachment::is_device`-guarded reads.
    pub fn device_session(&self) -> Option<&RuntimeSession> {
        self.sessions.values().find(|session| session.is_device())
    }

    /// The session-targeted resolution seam: device-session, deploy, and
    /// reconcile ops resolve their client through here — named separately
    /// from the lens seam so P2+ can target a specific session among
    /// several.
    ///
    /// P1: resolves the sole session REGARDLESS of kind, because the pool
    /// holds at most one and the retired single slot served device flows
    /// with its one client the same way. P2 narrows this to the ≤1
    /// device-kind session once coexistence lands.
    pub fn device_session_mut(&mut self) -> Result<&mut RuntimeSession, UiError> {
        self.sessions
            .values_mut()
            .next()
            .ok_or_else(missing_session)
    }

    /// The ≤1 SIM-kind session.
    pub fn sim_session(&self) -> Option<&RuntimeSession> {
        self.sessions.values().find(|session| session.is_sim())
    }

    /// The ≤1 SIM-kind session, mutably.
    pub fn sim_session_mut(&mut self) -> Option<&mut RuntimeSession> {
        self.sessions.values_mut().find(|session| session.is_sim())
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
    use lpa_link::DeviceState;

    use super::super::runtime_session::ready_state_for_test;
    use super::*;

    #[test]
    fn install_replaces_the_sole_session_and_moves_the_lens() {
        let mut pool = RuntimePool::new();
        assert!(!pool.has_session());
        assert!(pool.lens().is_none());
        assert!(pool.lens_session().is_none());

        let first = pool.install(RuntimePayload::stub_sim_for_test());
        assert_eq!(pool.lens(), Some(first));
        assert!(pool.session(first).is_some());

        // P1: attach still replaces — the old session is gone, the lens
        // follows the new one, and ids are never reused.
        let second = pool.install(RuntimePayload::stub_device_for_test(DeviceState::Gone));
        assert_ne!(first, second);
        assert_eq!(pool.lens(), Some(second));
        assert!(pool.session(first).is_none());
        assert!(pool.has_session());
    }

    #[test]
    fn lens_seam_resolves_the_lens_session_or_reports_missing_session() {
        let mut pool = RuntimePool::new();
        assert!(matches!(
            pool.lens_session_mut(),
            Err(UiError::MissingSession(message))
                if message == "server client is not connected"
        ));

        let id = pool.install(RuntimePayload::stub_sim_for_test());
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
    fn device_seam_resolves_the_sole_session_regardless_of_kind_in_p1() {
        let mut pool = RuntimePool::new();
        assert!(matches!(
            pool.device_session_mut(),
            Err(UiError::MissingSession(_))
        ));

        // A sole SIM session still resolves through the device-targeted
        // seam — the retired single slot served device flows with its one
        // client the same way (P2 narrows this to kind-filtered targeting).
        let sim_id = pool.install(RuntimePayload::stub_sim_for_test());
        assert_eq!(
            pool.device_session_mut().expect("sole session").id(),
            sim_id
        );
        // The kind-filtered EVIDENCE views stay honest either way.
        assert!(pool.device_session().is_none());
        assert_eq!(pool.sim_session().map(RuntimeSession::id), Some(sim_id));
        assert_eq!(pool.sim_session_mut().map(|s| s.id()), Some(sim_id));

        let device_id = pool.install(RuntimePayload::stub_device_for_test(DeviceState::Gone));
        assert_eq!(
            pool.device_session_mut().expect("sole session").id(),
            device_id
        );
        assert_eq!(
            pool.device_session().map(RuntimeSession::id),
            Some(device_id)
        );
        assert!(pool.sim_session().is_none());
        assert!(pool.sim_session_mut().is_none());
    }

    #[test]
    fn session_by_id_and_take_sole_session_round_trip() {
        let mut pool = RuntimePool::new();
        let id = pool.install(RuntimePayload::stub_device_for_test(ready_state_for_test()));

        assert!(pool.session_mut(id).is_some());
        let taken = pool.take_sole_session().expect("sole session taken");
        assert_eq!(taken.id(), id);
        assert!(!pool.has_session());
        assert!(pool.lens().is_none());
        assert!(pool.take_sole_session().is_none());
    }

    #[test]
    fn device_uid_association_derives_from_the_hello() {
        let mut pool = RuntimePool::new();

        // Booting hardware: the RuntimeId exists, the dev_ uid does not yet.
        pool.install(RuntimePayload::stub_device_for_test(DeviceState::Booting));
        assert_eq!(
            pool.device_session().and_then(RuntimeSession::device_uid),
            None
        );

        // The hello lands (Ready) carrying the uid: the association exists,
        // keyed under the same minted RuntimeId scheme.
        let mut hello_state = ready_state_for_test();
        if let DeviceState::Ready { hello } = &mut hello_state {
            hello.device_uid = Some("dev_aaaaaaaaaaaaaaaa".to_string());
        }
        pool.install(RuntimePayload::stub_device_for_test(hello_state));
        assert_eq!(
            pool.device_session().and_then(RuntimeSession::device_uid),
            Some("dev_aaaaaaaaaaaaaaaa".to_string())
        );

        // The sim never associates a device uid (D22).
        pool.install(RuntimePayload::stub_sim_for_test());
        assert_eq!(
            pool.lens_session().and_then(RuntimeSession::device_uid),
            None
        );
    }
}
