//! What sending one channel-3 message (the over-the-air update protocol,
//! `lpc-update`) on a host link came to — the same answer on the USB link
//! (`usb_link::UsbLinkShared::send_update`) and on a radio link
//! (`radio_link::RadioLinkPort::send_update`).

/// What a link did with one channel-3 message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateSend {
    /// Queued on channel 3.
    Queued,
    /// The link has no room right now (the send ring is full, or a reply
    /// holds the frame buffer): keep it and try again.
    Later,
    /// No host has the link up: the message belongs to no session.
    NoSession,
    /// Larger than this image can send (no frame buffer without `server`,
    /// or past the link's largest message).
    TooBig,
}
