#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionEnablement {
    Enabled,
    Disabled { reason: String },
}

impl ActionEnablement {
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled)
    }
}
