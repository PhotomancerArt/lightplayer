//! [`UiDrawSurface`]: which live picture a stroke is on.

use lpa_devices::DeviceId;

/// A live picture a finger can draw on: the web reports it back in
/// [`crate::UiStroke`] exactly as core published it.
///
/// Every drawable picture is the open session's own (the editor's lens), so
/// all of them convert through the same lamp layout; the surface says which
/// one the user touched, so core can check it is still drawable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum UiDrawSurface {
    /// The connected board's card picture on the home page.
    Board(DeviceId),
    /// The open project's picture: the root module's hero in the editor
    /// and Play's banner (both draw the root module face's preview).
    Project,
}
