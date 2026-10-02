/// Who may press an action: anyone driving the view (the user, the app
/// agent), or only the user's own click.
///
/// Metadata, like [`crate::ActionConfirmation`]: the renderer draws an
/// ordinary button either way. What it changes is the agent — an action
/// that is not [`Self::Anyone`] is never dispatched by the agent; it is
/// offered to the user as a card whose click dispatches it (plan D6, PD5).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ActionGesture {
    /// Any driver of the view may press it.
    #[default]
    Anyone,
    /// The browser only allows it from a real click (user activation):
    /// `navigator.serial.requestPort()`, `navigator.bluetooth.requestDevice()`.
    UserActivation,
    /// The click is the decision: it replaces or removes something on the
    /// user's board or in their library that they may want back (flashing
    /// over what a chip runs, factory reset, forget).
    UserDecision,
}
