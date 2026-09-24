//! Which link, of a server transport's links, a message came in on.

/// A server-side link: one client connection a transport carries (the USB
/// serial line, one BLE connection, one websocket).
///
/// Chosen by the TRANSPORT, never by a message. A single-link transport
/// (USB today, the host, the browser worker, the emulator) has exactly one,
/// [`LinkId::PRIMARY`]. A multi-link transport mints ids monotonically and
/// **never reuses one**, so a session keyed on a closed link can never be
/// inherited by the connection that replaced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LinkId(u32);

impl LinkId {
    /// The one link of a single-link transport.
    pub const PRIMARY: LinkId = LinkId(0);

    /// A link id from its raw number (multi-link transports mint these).
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw number, for logs.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl core::fmt::Display for LinkId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "link{}", self.0)
    }
}
