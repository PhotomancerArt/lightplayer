//! The deploy dialog's view model: the session state plus the data the
//! renderer needs alongside it (push candidates for the picker).

use super::deploy_session::DeployState;

/// One pickable push target.
#[derive(Clone, Debug, PartialEq)]
pub struct UiDeployChoice {
    pub uid: String,
    pub slug: String,
}

/// The open deploy dialog, as the web shell renders it (a modal overlay
/// over whatever the shell shows — gallery or editor).
#[derive(Clone, Debug, PartialEq)]
pub struct UiDeployView {
    pub state: DeployState,
    /// Library projects the picker offers (from the cached gallery
    /// inputs; empty when no library mounted).
    pub choices: Vec<UiDeployChoice>,
    /// Hardware connect actions for the `NeedsDevice` state (provider /
    /// endpoint ops — never the simulator, D22).
    pub connect_actions: Vec<crate::UiAction>,
}
