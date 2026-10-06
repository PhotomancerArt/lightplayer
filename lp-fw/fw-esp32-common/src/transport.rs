//! The `M!`-line decoder the remaining `M!` host links share.
//!
//! The classic's UART spoke `M!` lines through a transport here
//! (`StreamingMessageRouterTransport`) until it moved onto lp-link (plan
//! `classic-uart-on-lp-link`: `crate::uart_link`, as the C6/S3's USB did
//! before it, `crate::usb_link`). What is left is the line decoder the BLE
//! links' mux still reads with (`crate::radio_link`), until their own
//! milestone moves them too.

use lpc_wire::{ClientMessage, json};

/// One received line → the client message it carries, or `None` for a line
/// that is not a wire frame (skipped quietly) or does not parse (logged and
/// counted: a torn or spliced frame is protocol loss, not chatter —
/// 2026-08-26 inbound-loss defect: this drop sat at DEBUG and made losses
/// invisible). Radio lines, today; every `M!` link parsed alike.
///
/// `#[inline(always)]` is load-bearing. As an ordinary function it is
/// codegen'd once, out of line, in this crate, and that standalone
/// `json::from_str::<ClientMessage>` changed how the deserializer was inlined
/// into the USB receive path: the main task's `poll` frame grew from
/// `entry a1, 1024` to `4448` on the S3 and from `976` to `4112` on the
/// classic, on images with no radio link at all (and the S3's `.data` gained
/// a duplicated 12 B serde_json constant table, which cost the stack 16 B).
/// Inlined into each caller, both frames are back to what they were before
/// the radio links existed.
#[inline(always)]
pub fn parse_wire_line(msg_line: &str) -> Option<ClientMessage> {
    let Some(json_str) = msg_line.strip_prefix("M!") else {
        log::trace!("transport: skipping non-message line");
        return None;
    };
    let json_str = json_str.trim_end_matches('\n');
    match json::from_str::<ClientMessage>(json_str) {
        Ok(msg) => {
            log::debug!("transport: received message id={}", msg.id);
            Some(msg)
        }
        Err(e) => {
            // A radio line is arbitrary UTF-8: cut the preview on a char
            // boundary, never mid-character.
            // A line that may carry a Wi-Fi password gets no preview at all.
            let mut preview_len = if lpc_wire::may_carry_secret(json_str) {
                0
            } else {
                json_str.len().min(48)
            };
            while !json_str.is_char_boundary(preview_len) {
                preview_len -= 1;
            }
            crate::serial::link_counters::bump_parse_failure();
            log::warn!(
                "transport: dropping unparseable {} B M! line ({e}); prefix: {:?}",
                json_str.len(),
                &json_str[..preview_len]
            );
            None
        }
    }
}
