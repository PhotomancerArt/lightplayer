//! What the board's pictures are doing (relay protocol 2), in the
//! heartbeat's words: `pictures N idle|watched|off`.

/// See the module doc. Read off the client with the time
/// ([`RelayDriver::picture_mode`](super::RelayDriver::picture_mode)), so a
/// watch that ran out reads `idle` (or `off`) at once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RelayPictureMode {
    /// No pictures: the hub has not asked (no rate since the board
    /// registered, or no leg), or it asked for none while idle.
    #[default]
    Off,
    /// The idle cadence (one a minute by default).
    Idle,
    /// Someone is watching: the fast cadence, until the watch runs out.
    Watched,
}

impl RelayPictureMode {
    /// The heartbeat's word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Idle => "idle",
            Self::Watched => "watched",
        }
    }
}
