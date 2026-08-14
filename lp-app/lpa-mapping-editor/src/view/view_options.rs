//! Canvas view toggles (editor defaults: wiring view on — this is the
//! editing surface, not the lit preview).

/// Canvas view toggles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorViewOptions {
    pub numbers: bool,
    pub arrows: bool,
    /// Fill lamps from host-supplied live colors (`live_feed` prop);
    /// without a feed this falls back to the object palette.
    pub live: bool,
    pub fit_preview: bool,
    /// Show the host-supplied reference image (`reference` prop); moot
    /// without one.
    pub reference: bool,
}

impl Default for EditorViewOptions {
    fn default() -> Self {
        Self {
            numbers: true,
            arrows: true,
            live: false,
            fit_preview: false,
            reference: true,
        }
    }
}
