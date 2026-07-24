//! The keyed collection of runtime sessions plus the editor lens.
//!
//! P2 scope (coexistence): capacity is a POLICY — a number per kind, not a
//! shape. The pool admits [`SIM_SESSION_CAPACITY`] sim session(s) and
//! [`DEVICE_SESSION_CAPACITY`] device session(s) simultaneously;
//! [`RuntimePool::install`] no longer evicts the other KIND, a same-kind
//! attach replaces (refused while an operation is in flight on the session
//! being replaced — the DQ-A swap record), and install preserves a held
//! lens (P3: attaching observes; only a lens-less pool — or an evicted
//! lens session — hands the lens to the newcomer). The two named
//! resolution seams every network op goes through:
//!
//! - **Lens-bound** ops (the editor mirror) resolve
//!   [`RuntimePool::lens_session_mut`] — the session the editor is a lens
//!   on.
//! - **Session-targeted** ops (device flows, deploy, reconcile) resolve
//!   [`RuntimePool::device_session_mut`] — the ≤1 DEVICE-kind session,
//!   regardless of where the lens is.

use std::collections::BTreeMap;

use crate::{RuntimeId, RuntimeSession, UiError};

use super::runtime_session::{RuntimeKind, RuntimePayload};

/// How many SIM sessions the pool admits at once (MVP policy; "+ new
/// simulator" raises this number, not the shape).
pub const SIM_SESSION_CAPACITY: usize = 1;
/// How many DEVICE sessions the pool admits at once (MVP policy).
pub const DEVICE_SESSION_CAPACITY: usize = 1;

/// [`RuntimePool::install`] refused to replace a same-kind session because
/// an operation is in flight on it. Carries the payload back so the caller
/// can close it instead of leaking the fresh session.
pub struct InstallRefusal {
    pub payload: RuntimePayload,
    pub message: String,
}

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
    /// Kind-aware capacity (P2): sessions of the OTHER kind stay attached;
    /// same-kind sessions beyond the kind's capacity are replaced, oldest
    /// first — refused (payload handed back) while an operation is in
    /// flight on a session that would be replaced.
    ///
    /// Lens rule (P3): install preserves the lens unless none — attaching
    /// a runtime observes; it never steals the editor from a session the
    /// lens is on. The newcomer only claims the lens when nothing holds it
    /// (an empty pool, a detached editor, or a same-kind replace that just
    /// evicted the lens session — the replacement inherits the lens).
    /// Flows that deliberately move the editor (opening a project on the
    /// sim, the D29 device click) call [`RuntimePool::set_lens`].
    pub fn install(&mut self, payload: RuntimePayload) -> Result<RuntimeId, InstallRefusal> {
        let kind = payload.kind();
        let mut same_kind: Vec<RuntimeId> = self
            .sessions
            .iter()
            .filter(|(_, session)| session.kind() == kind)
            .map(|(id, _)| *id)
            .collect();
        // Evict oldest-first until the newcomer fits under the capacity.
        while same_kind.len() + 1 > kind_capacity(kind) {
            let oldest = same_kind.remove(0);
            if self
                .sessions
                .get(&oldest)
                .is_some_and(RuntimeSession::op_in_flight)
            {
                return Err(InstallRefusal {
                    payload,
                    message: "A device operation is still running — let it finish before \
                              connecting another runtime"
                        .to_string(),
                });
            }
            self.remove(oldest);
        }
        let id = self.mint_id();
        self.sessions.insert(id, RuntimeSession::new(id, payload));
        if self.lens.is_none() {
            self.lens = Some(id);
        }
        Ok(id)
    }

    /// Detach the editor lens (P3): the mirror's session binding drops,
    /// every session stays — worker running, wire client attached, device
    /// reconcile state intact. The caller (`StudioController`) owns the
    /// mirror teardown (`project.reset()`); this only releases the id.
    pub(crate) fn detach_lens(&mut self) {
        self.lens = None;
    }

    /// Drop every session (and the lens) without closing payloads — the
    /// `RefreshConnections` recovery semantics. Absence = not in the pool.
    pub fn clear(&mut self) {
        self.sessions.clear();
        self.lens = None;
    }

    /// Take every session out of the pool (full attachment teardown — the
    /// P2-interim `DisconnectDevice` semantics); the caller closes the
    /// payloads.
    pub fn take_all_sessions(&mut self) -> Vec<RuntimeSession> {
        self.lens = None;
        core::mem::take(&mut self.sessions).into_values().collect()
    }

    /// Remove the ≤1 session of `kind` (a failed/cancelled connect of that
    /// kind clears the slot it was aimed at; the other kind stays). The
    /// lens clears if it was on the removed session.
    pub fn remove_kind(&mut self, kind: RuntimeKind) -> Option<RuntimeSession> {
        let id = self
            .sessions
            .iter()
            .find(|(_, session)| session.kind() == kind)
            .map(|(id, _)| *id)?;
        self.remove(id)
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

    /// The ≤1 DEVICE-kind session's evidence view (roster/deploy
    /// derivations): kind-filtered, like the retired
    /// `RuntimeAttachment::is_device`-guarded reads.
    pub fn device_session(&self) -> Option<&RuntimeSession> {
        self.sessions.values().find(|session| session.is_device())
    }

    /// The session-targeted resolution seam: device-session, deploy, and
    /// reconcile ops resolve their client through here. P2: kind-filtered
    /// to the ≤1 DEVICE session — device flows never land on the sim, no
    /// matter where the lens is.
    pub fn device_session_mut(&mut self) -> Result<&mut RuntimeSession, UiError> {
        self.sessions
            .values_mut()
            .find(|session| session.is_device())
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

/// The capacity policy, per kind — numbers, not shapes.
fn kind_capacity(kind: RuntimeKind) -> usize {
    match kind {
        RuntimeKind::Sim => SIM_SESSION_CAPACITY,
        RuntimeKind::Device => DEVICE_SESSION_CAPACITY,
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

    fn install(pool: &mut RuntimePool, payload: RuntimePayload) -> RuntimeId {
        pool.install(payload).unwrap_or_else(|refusal| {
            panic!("install refused: {}", refusal.message);
        })
    }

    #[test]
    fn a_sim_and_a_device_session_coexist() {
        let mut pool = RuntimePool::new();
        let device = install(
            &mut pool,
            RuntimePayload::stub_device_for_test(ready_state_for_test()),
        );
        let sim = install(&mut pool, RuntimePayload::stub_sim_for_test());

        // Installing the sim did NOT evict the device (P2 capacity policy).
        assert!(pool.session(device).is_some());
        assert!(pool.session(sim).is_some());
        assert_eq!(pool.sessions().count(), 2);
        // Install observes (P3): the lens stays where it was — flows that
        // deliberately move the editor call `set_lens`.
        assert_eq!(pool.lens(), Some(device));
        // Kind-filtered views resolve their own kinds.
        assert_eq!(pool.device_session().map(RuntimeSession::id), Some(device));
        assert_eq!(pool.sim_session().map(RuntimeSession::id), Some(sim));
        assert_eq!(pool.device_session_mut().expect("device seam").id(), device);
    }

    #[test]
    fn same_kind_install_replaces_and_only_an_evicted_lens_moves() {
        let mut pool = RuntimePool::new();
        let first_sim = install(&mut pool, RuntimePayload::stub_sim_for_test());
        assert_eq!(
            pool.lens(),
            Some(first_sim),
            "an empty pool's first session claims the lens"
        );
        let device = install(
            &mut pool,
            RuntimePayload::stub_device_for_test(DeviceState::Gone),
        );
        assert_eq!(pool.lens(), Some(first_sim), "a held lens is never stolen");
        let second_sim = install(&mut pool, RuntimePayload::stub_sim_for_test());

        assert_ne!(first_sim, second_sim, "ids are never reused");
        assert!(
            pool.session(first_sim).is_none(),
            "same-kind attach replaces"
        );
        assert!(pool.session(device).is_some(), "the other kind stays");
        assert_eq!(
            pool.lens(),
            Some(second_sim),
            "evicting the lens session hands the lens to the replacement"
        );

        let second_device = install(
            &mut pool,
            RuntimePayload::stub_device_for_test(ready_state_for_test()),
        );
        assert!(pool.session(device).is_none());
        assert!(pool.session(second_sim).is_some());
        assert_eq!(
            pool.lens(),
            Some(second_sim),
            "replacing the OTHER kind leaves the lens alone"
        );
        assert_ne!(pool.lens(), Some(second_device));
        assert_eq!(pool.sessions().count(), 2);
    }

    #[test]
    fn detach_lens_keeps_every_session_and_reattach_resolves_again() {
        let mut pool = RuntimePool::new();
        let device = install(
            &mut pool,
            RuntimePayload::stub_device_for_test(ready_state_for_test()),
        );
        let sim = install(&mut pool, RuntimePayload::stub_sim_for_test());
        pool.set_lens(sim);

        pool.detach_lens();

        // The lens is gone; BOTH sessions stay in the pool untouched.
        assert_eq!(pool.lens(), None);
        assert!(pool.session(device).is_some(), "device session survives");
        assert!(pool.session(sim).is_some(), "sim session survives");
        assert!(matches!(
            pool.lens_session_mut(),
            Err(UiError::MissingSession(_))
        ));

        // Re-attach: the lens resolves the chosen session again.
        pool.set_lens(device);
        assert_eq!(pool.lens_session_mut().expect("lens resolves").id(), device);

        // A detached editor lets the next install claim the lens.
        pool.detach_lens();
        let second_sim = install(&mut pool, RuntimePayload::stub_sim_for_test());
        assert_eq!(pool.lens(), Some(second_sim));
    }

    #[test]
    fn install_refuses_to_replace_a_session_with_an_operation_in_flight() {
        let mut pool = RuntimePool::new();
        let device = install(
            &mut pool,
            RuntimePayload::stub_device_for_test(ready_state_for_test()),
        );
        pool.session_mut(device)
            .expect("device session")
            .set_operation(Some("Installing firmware".to_string()));

        // Replacing the busy device refuses; the payload comes back.
        let refusal = pool
            .install(RuntimePayload::stub_device_for_test(DeviceState::Gone))
            .expect_err("busy replace refuses");
        assert!(refusal.message.contains("still running"));
        assert!(matches!(refusal.payload, RuntimePayload::Device(_)));
        assert_eq!(pool.device_session().map(RuntimeSession::id), Some(device));

        // A DIFFERENT kind still installs (the busy session is not replaced).
        let sim = install(&mut pool, RuntimePayload::stub_sim_for_test());
        assert_eq!(pool.sessions().count(), 2);

        // The operation finishing re-enables the swap.
        pool.session_mut(device)
            .expect("device session")
            .set_operation(None);
        let replacement = install(
            &mut pool,
            RuntimePayload::stub_device_for_test(ready_state_for_test()),
        );
        assert!(pool.session(device).is_none());
        assert!(pool.session(sim).is_some());
        assert_eq!(pool.lens(), Some(replacement));
    }

    #[test]
    fn remove_kind_removes_only_that_kind_and_clears_a_lens_on_it() {
        let mut pool = RuntimePool::new();
        let device = install(
            &mut pool,
            RuntimePayload::stub_device_for_test(ready_state_for_test()),
        );
        let sim = install(&mut pool, RuntimePayload::stub_sim_for_test());
        assert_eq!(pool.lens(), Some(device), "install never steals the lens");

        // Removing a kind with no session is a no-op (lens untouched).
        pool.set_lens(device);
        assert!(pool.remove_kind(RuntimeKind::Sim).is_some());
        assert!(pool.session(sim).is_none(), "the sim session is gone");
        assert_eq!(pool.lens(), Some(device), "lens was not on the removed sim");
        assert!(pool.remove_kind(RuntimeKind::Sim).is_none());

        let removed = pool
            .remove_kind(RuntimeKind::Device)
            .expect("device removed");
        assert_eq!(removed.id(), device);
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

        let id = install(&mut pool, RuntimePayload::stub_sim_for_test());
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
    fn device_seam_is_kind_filtered_in_p2() {
        let mut pool = RuntimePool::new();
        assert!(matches!(
            pool.device_session_mut(),
            Err(UiError::MissingSession(_))
        ));

        // A sole SIM session no longer resolves through the device-targeted
        // seam (P2): device flows never land on the sim.
        install(&mut pool, RuntimePayload::stub_sim_for_test());
        assert!(matches!(
            pool.device_session_mut(),
            Err(UiError::MissingSession(_))
        ));
        assert!(pool.device_session().is_none());

        let device_id = install(
            &mut pool,
            RuntimePayload::stub_device_for_test(DeviceState::Gone),
        );
        assert_eq!(pool.device_session_mut().expect("device").id(), device_id);
        assert_eq!(
            pool.device_session().map(RuntimeSession::id),
            Some(device_id)
        );
        assert!(pool.sim_session().is_some(), "the sim coexists");
    }

    #[test]
    fn take_all_sessions_empties_the_pool_and_the_lens() {
        let mut pool = RuntimePool::new();
        install(
            &mut pool,
            RuntimePayload::stub_device_for_test(ready_state_for_test()),
        );
        install(&mut pool, RuntimePayload::stub_sim_for_test());

        let taken = pool.take_all_sessions();
        assert_eq!(taken.len(), 2);
        assert!(!pool.has_session());
        assert!(pool.lens().is_none());
        assert!(pool.take_all_sessions().is_empty());
    }

    #[test]
    fn device_uid_association_derives_from_the_hello() {
        let mut pool = RuntimePool::new();

        // Booting hardware: the RuntimeId exists, the dev_ uid does not yet.
        install(
            &mut pool,
            RuntimePayload::stub_device_for_test(DeviceState::Booting),
        );
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
        install(&mut pool, RuntimePayload::stub_device_for_test(hello_state));
        assert_eq!(
            pool.device_session().and_then(RuntimeSession::device_uid),
            Some("dev_aaaaaaaaaaaaaaaa".to_string())
        );

        // The sim never associates a device uid (D22).
        install(&mut pool, RuntimePayload::stub_sim_for_test());
        assert_eq!(
            pool.sim_session().and_then(RuntimeSession::device_uid),
            None
        );
    }
}
