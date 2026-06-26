/// Whether an action can currently be invoked.
///
/// Disabled actions keep their metadata visible while explaining why the user
/// cannot run them yet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionEnablement {
    /// The action can be invoked.
    Enabled,
    /// The action is visible but blocked for the given reason.
    Disabled { reason: String },
}

impl ActionEnablement {
    /// Return whether the action is currently invokable.
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled)
    }
}
