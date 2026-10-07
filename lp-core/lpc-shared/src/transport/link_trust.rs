//! Whether a link is trusted by virtue of what it physically is.

/// A property of the LINK, set by the firmware (the transport), never by a
/// message.
///
/// - **Trusted** — physical possession: the USB cable, the host process,
///   the browser worker, the emulator's own console. A trusted link holds
///   the edit tier without logging in, because the cable is the recovery
///   path for a board whose passwords are lost.
/// - **Untrusted** — a radio link (BLE). It holds nothing until it logs in,
///   except play when the device is explicitly `open`.
/// - **Keyed** — an untrusted network link nearby secured by lp-link's
///   secure channel (a LAN WebSocket). Its tier is the tier of the access
///   entry its handshake matched (none for the anonymous key), else what
///   the device is `open` to. It logs in by handshake only: the HMAC
///   `LoginAnswer` is refused on it, so nothing in the middle of a session
///   can pass a login through it.
/// - **Relayed** — the same secure link, arriving through the cloud relay
///   from anywhere on the internet. Keyed in every way but one: the
///   device's `open` ("Anyone nearby") **never applies** to it, because a
///   board's relay id is not a secret. Its tier is only what its handshake's
///   key grants; the anonymous key completes a handshake (so a visitor can
///   read the login offers and come back with a password's key) but holds
///   nothing. Decision record: `docs/adr/2026-10-06-cloud-relay.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkTrust {
    Trusted,
    Untrusted,
    Keyed,
    Relayed,
}

impl LinkTrust {
    /// Whether the link logs in by its secure handshake (and never by an
    /// HMAC `LoginAnswer`): [`Self::Keyed`] and [`Self::Relayed`].
    #[must_use]
    pub const fn is_secure(self) -> bool {
        matches!(self, Self::Keyed | Self::Relayed)
    }
}
