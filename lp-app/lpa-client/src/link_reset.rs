//! How a device link's reset reaches a request that was waiting on it.
//!
//! Since `WIRE_PROTO_VERSION` 30 a board's USB serial link is an lp-link
//! (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D9). When that link
//! resets (the board rebooted, the host restarted it, or it gave up
//! resending), every request in flight on the old session is lost, and the
//! board will never answer it. Rather than wait out an idle budget, the
//! transport fails the waiting `receive` at once with the error this module
//! names.
//!
//! It is an ordinary [`TransportError::Other`], so every existing error path
//! (a failed request, a retry) handles it unchanged; [`is_link_reset`] lets
//! the few places that must tell it apart (a readiness gate that expects the
//! board to reboot under it) do so without matching on text elsewhere.

use lpc_wire::TransportError;
use lpc_wire::lp_link::ResetReason;

/// The text every link-reset error starts with.
pub const LINK_RESET_PREFIX: &str = "link reset";

/// The error a request in flight gets when its link resets. `detail` says
/// why (`board restarted`, `gave up resending`, …).
pub fn link_reset_error(detail: &str) -> TransportError {
    TransportError::Other(format!(
        "{LINK_RESET_PREFIX}: {detail}; the request was lost with the old session"
    ))
}

/// Whether `error` is a link reset ([`link_reset_error`]).
pub fn is_link_reset(error: &TransportError) -> bool {
    matches!(error, TransportError::Other(message) if message.starts_with(LINK_RESET_PREFIX))
}

/// Why a link reset, in a few plain words (for errors and journal lines).
pub fn reset_reason_words(reason: ResetReason) -> &'static str {
    match reason {
        ResetReason::PeerRestarted => "the board restarted",
        ResetReason::RetryLimit => "the board stopped answering and resends gave up",
        ResetReason::ProtocolError => "the link hit a protocol error",
        ResetReason::Requested => "the host restarted the link",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_reset_is_recognized_and_nothing_else_is() {
        assert!(is_link_reset(&link_reset_error("board restarted")));
        assert!(!is_link_reset(&TransportError::ConnectionLost));
        assert!(!is_link_reset(&TransportError::Other(
            "device did not respond".to_string()
        )));
    }
}
