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
    /// Edited on device: click → editor + the D30 diverged popup.
    ResolveDrift,
    /// Degraded / Not responding: troubleshooting instructions popup.
    Troubleshoot,
    /// Connected, empty: project picker popup.
    ChooseProject,
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
            Self::OpenEditor => "Open".to_string(),
            Self::PushVersion { version: Some(n) } => format!("Push v{n}"),
            Self::PushVersion { version: None } => "Push".to_string(),
            Self::ResolveDrift => "Review".to_string(),
            Self::Troubleshoot => "Troubleshoot".to_string(),
            Self::ChooseProject => "Choose a project".to_string(),
            Self::SetUp => "Set up".to_string(),
            Self::UpdateFirmware => "Update".to_string(),
            Self::NameDevice => "Name it".to_string(),
            Self::Reconnect => "Reconnect".to_string(),
        }
    }
}
