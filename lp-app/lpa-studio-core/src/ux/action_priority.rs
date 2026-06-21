/// Presentation priority for an action that is currently available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionPriority {
    Primary,
    Secondary,
    Tertiary,
    Danger,
}
