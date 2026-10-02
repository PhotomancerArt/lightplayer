//! Who may be reading the one static frame buffer (`serial::server_msg`)
//! when the link mux wants to serialize a radio frame into it.
//!
//! The mux's primary transport shares that buffer. The C6/S3 USB
//! link hands its reply to lp-link as an external message and the link task
//! keeps reading it out of the buffer after `send` returned
//! (`usb_link::UsbLinkTransport`), so the mux must ask it to let go first.

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
