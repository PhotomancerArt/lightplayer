//! The one affordance a roster card carries (identity only in M2).
//!
//! Each card grammar row names at most one affordance and what it opens
//! (direction.md state table). This enum carries the affordance IDENTITY;
//! the action wiring lands with the flows that make each state real
//! (M3 card anatomy, M6 auto-connect, M8 provisioning popup).

/// What a roster card offers the user, per the direction state table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RosterAffordance {
    /// Running, up to date: click → editor attached to this device (D29).
    OpenEditor,
    /// Running, behind: the push button IS the D11 consent click.
    /// `version` is the local head's version number for the "Push vN" label.
    PushVersion { version: Option<usize> },
    /// Edited on device, on the card FACE (§3c-2 — the D30 sheet is
    /// gone): adopt the board's copy as the new head. Non-destructive by
    /// construction — the old head stays in project history — so it
    /// dispatches without a gate.
    UseBoardCopy,
    /// Edited on device, the face's second verb: keep both (fork the
    /// board's copy into its own project).
    KeepBoth,
    /// Degraded / Not responding: troubleshooting instructions popup.
    Troubleshoot,
    /// Connected, empty: project picker popup.
    ChooseProject,
    /// Holds an OLD-format project this build can migrate: pull → migrate
    /// in the library → push (the device never upgrades in place, D14 /
    /// ADR 2026-07-05 decision 5). Non-destructive by construction — the
    /// pre-migration copy stays in project history — so, like
    /// [`Self::UseBoardCopy`], it dispatches without a gate.
    ///
    /// Offered ONLY for a format this build can actually migrate. A
    /// project below the upgrade floor, or one from a newer LightPlayer,
    /// keeps [`Self::WipeProject`] and an honest note: a button that
    /// cannot work is worse than no button.
    UpgradeProject,
    /// Holds-unreadable-data: wipe the device's project storage back to
    /// blank (model rev 2026-07-26 — the way out is BLANK, never
    /// push-over). Destructive: the web gates it with the confirm sheet.
    WipeProject,
    /// Ready to set up / Other firmware: install (provisioning) popup.
    SetUp,
    /// Needs a firmware update: confirm popup.
    UpdateFirmware,
    /// Needs a name: name popup.
    NameDevice,
    /// Offline: click → reconnect over the granted port + open.
    Reconnect,
}

impl RosterAffordance {
    /// Button/affordance label. Click-through affordances (open editor,
    /// resolve drift, reconnect) still label the card's action for
    /// accessibility even when no button renders.
    pub fn label(&self) -> String {
        match self {
            Self::OpenEditor => "Open in editor".to_string(),
            // §3a: no version jargon — the Project facts carry distance.
            Self::PushVersion { .. } => "Push the latest".to_string(),
            Self::UseBoardCopy => "Use board copy".to_string(),
            Self::KeepBoth => "Keep both".to_string(),
            Self::Troubleshoot => "Troubleshoot".to_string(),
            Self::ChooseProject => "Choose a project".to_string(),
            Self::UpgradeProject => "Upgrade project".to_string(),
            Self::WipeProject => "Wipe project…".to_string(),
            Self::SetUp => "Set up".to_string(),
            Self::UpdateFirmware => "Update".to_string(),
            Self::NameDevice => "Name it".to_string(),
            Self::Reconnect => "Reconnect".to_string(),
        }
    }
}
