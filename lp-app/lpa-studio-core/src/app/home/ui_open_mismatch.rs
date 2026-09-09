//! The open that stopped: an address named a device, and that device is
//! running a different project.
//!
//! **Never a silent push** (vision D50). `?on=mac:<mac>` names one device,
//! and a device is a place with something on it — a sign in a window, a
//! strip under a counter, a board somebody else set up. Opening a project
//! "on" it is a reasonable thing to ask for and a terrible thing to
//! assume: the ordinary open ends in a push, and a push replaces what is
//! running. So the open stops here instead, states both projects by name,
//! and offers the two things a person could actually mean:
//!
//! - **switch** to the project that is running (a navigation — the device
//!   is untouched), or
//! - **push this one here**, which the person has now chosen with the
//!   consequence in front of them.
//!
//! This state is the model's, not the shell's, for the same reason every
//! other refusal is: what may be pushed where is a fact about devices and
//! the library, and the e2e tests that prove no push was issued run on the
//! model. The web edge renders it and dispatches the two verbs.
//!
//! Only an address that named an INSTANCE lands here. A kind hint
//! (`?on=sim`) and a bare `/p/…` ask Studio to *resolve* a device, and
//! resolving is Studio's own choice about its own scratch devices — the
//! ordinary open reuses the tab's sim and pushes to it, which is what
//! opening a project has always meant (D33). It is naming a device that
//! makes a promise worth stopping for.

/// An open that named a device already running something else (D50).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiOpenMismatch {
    /// The project the address asked to open, and its display name.
    pub project_uid: String,
    pub project_name: String,
    /// The device the address named: its registry key, what to call it,
    /// and the base MAC that named it (so the page's verbs can spell the
    /// same `?on=` hint back).
    pub device_key: String,
    pub device_name: String,
    pub device_base_mac: String,
    /// The library project this device was last given — the one it is
    /// running. `None` when this library does not have it: then there is
    /// nothing to switch to, and nothing standing behind what a push would
    /// replace, so the page offers neither verb.
    pub running: Option<UiRunningProject>,
}

/// The project a device is running, as the mismatch page needs to name it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiRunningProject {
    pub uid: String,
    pub name: String,
}

impl UiOpenMismatch {
    /// Whether this library holds the project that is running — the gate
    /// on both verbs.
    ///
    /// "Switch" needs an address to switch TO, and a project this library
    /// does not have has no `/p/…`. "Push here" needs the same fact for a
    /// different reason: the library copy is what stands behind the push.
    /// Nothing is backed up — D50's "backing up what is on it" is vacuous
    /// in this build (Q7, DD19), because the page can only NAME a running
    /// project it found in the library, so what a push replaces is already
    /// there. Without that copy a push would destroy the only one, which
    /// is why the answer then is no verbs rather than a careful push.
    pub fn can_act(&self) -> bool {
        self.running.is_some()
    }
}
