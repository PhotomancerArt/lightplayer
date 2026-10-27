//! [`UiBoardPicture`]: the board's lights. While the board is well nothing
//! draws over it; otherwise the card's surface ([`super::UiBoardSurface`])
//! covers it with its notice and dims it, with one dim — the cover's, never
//! the picture's own stale dim too.

use crate::{UiControlProductPreview, UpdateLight};

/// The picture.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBoardPicture {
    /// Where the picture comes from.
    pub source: PictureSource,
    /// The frame, when it has geometry to draw; `None` keeps the picture
    /// dark.
    pub frame: Option<UiControlProductPreview>,
    /// Last known rather than current (a saved picture, or one the editor's
    /// lens holds still): drawn dimmed, unless the surface's notice covers
    /// it (its dim is the one dim then).
    pub dim: bool,
    /// The board's own lights while an update holds them (one solid
    /// colour), drawn as the light strip instead of the frame.
    pub light: Option<UpdateLight>,
    /// The picture is drawable (plan `drawing-on-the-picture`, D12): only on
    /// the connected board's card (its panel is up), while a `touches`
    /// reader is loaded. A watched card never is. A fact, not a verb: a
    /// stroke is reported as `StudioCommand::Stroke`.
    pub drawable: Option<crate::UiDrawable>,
}

/// Where the picture comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureSource {
    /// The board's link, live or waiting for its first frame.
    Link,
    /// The editor's lens holds the board's wire; the last frame it left.
    Lens,
    /// The last picture saved before the board went away.
    Saved,
    /// lightplayer.app's last picture of a board of the account that no
    /// link here reaches (`BoardPictures`): current, drawn one row of lamps
    /// per output.
    Cloud,
    /// No picture.
    None,
}
