//! [`UiBoardPicture`]: the board's lights. Nothing draws over it — no
//! sentence, no pill, no progress, no button; what used to be said over the
//! picture is the status corner's "Picture" line now (Q10).

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
    /// lens holds still): drawn dimmed.
    pub dim: bool,
    /// The board's own lights while an update holds them (one solid
    /// colour), drawn as the light strip instead of the frame.
    pub light: Option<UpdateLight>,
}

/// Where the picture comes from. Later, the cloud is one more case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureSource {
    /// The board's link, live or waiting for its first frame.
    Link,
    /// The editor's lens holds the board's wire; the last frame it left.
    Lens,
    /// The last picture saved before the board went away.
    Saved,
    /// No picture.
    None,
}
