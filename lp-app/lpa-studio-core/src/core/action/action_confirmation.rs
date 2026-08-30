/// Confirmation copy for an action that needs explicit user approval.
///
/// Use this for destructive, expensive, or surprising actions. The web renderer
/// decides how to present the confirmation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionConfirmation {
    /// Confirmation dialog title.
    pub title: String,
    /// Confirmation body copy.
    pub message: String,
    /// Label for the confirmation button.
    pub confirm_label: String,
    /// Present the confirmation as a two-click ARMED button (first click
    /// arms — the button itself asks; second click within the window
    /// dispatches) instead of the native dialog. For destructive-but-light
    /// verbs where a modal reads heavier than the act (G1 feedback,
    /// 2026-08-31: Forget device).
    pub inline: bool,
}

impl ActionConfirmation {
    /// Create confirmation copy for an action.
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        confirm_label: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            confirm_label: confirm_label.into(),
            inline: false,
        }
    }

    /// Two-click armed-button presentation (see [`Self::inline`]).
    pub fn inline(mut self) -> Self {
        self.inline = true;
        self
    }
}
