//! Who may be reading the one static frame buffer (`serial::server_msg`)
//! when the link mux wants to serialize a reply into it.
//!
//! Every transport serializes its replies into that one buffer. An lp-link
//! transport hands a long reply to its link as an external message, and the
//! link keeps reading it out of the buffer after `send` returned — the C6/S3
//! USB link (`usb_link::UsbLinkTransport`), the classic's UART link
//! (`uart_link::UartLinkTransport`, which has no radio link beside it and so
//! never meets the mux) and each radio link alike — so before the mux
//! serializes anything, every other holder must let go. The radio links are
//! the mux's own (`LinkMuxTransport` waits for them); the primary transport
//! lets go through this trait.

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
