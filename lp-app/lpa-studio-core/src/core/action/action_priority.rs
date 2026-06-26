/// Visual/action hierarchy for a `UiAction`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionPriority {
    /// The main action for the current surface or state.
    Primary,
    /// A normal supporting action.
    Secondary,
    /// A lower-emphasis action.
    Tertiary,
}
