//! Whether a link is trusted by virtue of what it physically is.

/// A property of the LINK, set by the firmware (the transport), never by a
/// message.
///
/// - **Trusted** — physical possession: the USB cable, the host process,
///   the browser worker, the emulator's own console. A trusted link holds
///   the edit tier without logging in, because the cable is the recovery
///   path for a board whose passwords are lost.
/// - **Untrusted** — a radio link (BLE, later WiFi). It holds nothing until
///   it logs in, except play when the device is explicitly `open`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkTrust {
    Trusted,
    Untrusted,
}
