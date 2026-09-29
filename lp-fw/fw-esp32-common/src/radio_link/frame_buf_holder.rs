//! Who may be reading the one static frame buffer (`serial::server_msg`)
//! when the link mux wants to serialize a reply into it.
//!
//! Every transport serializes its replies into that one buffer. The classic's
//! `M!` transport is done with it by the time its `send` returns (its io task
//! writes and reports back), so it holds nothing between sends. An lp-link
//! transport hands a long reply to its link as an external message, and the
//! link keeps reading it out of the buffer after `send` returned — the C6/S3
//! USB link (`usb_link::UsbLinkTransport`) and each radio link alike — so
//! before the mux serializes anything, every other holder must let go. The
//! radio links are the mux's own (`LinkMuxTransport` waits for them); the
//! primary transport lets go through this trait.

/// A transport that may still be reading the frame buffer between sends.
#[allow(
    async_fn_in_trait,
    reason = "single-executor firmware; no Send bound needed"
)]
pub trait FrameBufHolder {
    /// Return once this transport no longer reads the frame buffer (bounded:
    /// an implementation gives up what it holds rather than wait forever).
    async fn release_frame_buf(&mut self) {}
}
