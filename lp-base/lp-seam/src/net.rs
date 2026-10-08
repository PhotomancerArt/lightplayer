//! The network seam (`net=lan`): the calls, and the shapes they exchange.
//!
//! A **capability** seam in the **switch** shape (ADR
//! `docs/adr/2026-10-05-emulator-seams.md` §3): at network bring-up the
//! firmware reads [`crate::net_mac`]'s engaged byte. On silicon it reads 0
//! and the radio runs; on an emulated board that engaged `net=lan` it reads
//! 1, and a seam-backed station and frame device plug in under the same
//! IP stack. Every call is pull-only: the guest asks, the emulator answers
//! and writes only memory the call handed it. A frame or an event arriving
//! for an idle guest is announced by the one wake (`crate::wake`).
//!
//! The declarations themselves are in the crate's one `declare!`
//! invocation (`lib.rs`); this file holds the numbers both sides read.

use crate::SeamDecl;

/// Every network seam call, in declaration order. An emulator that engages
/// `net=lan` arms all of them, and the image's table carries all of them.
pub const CALLS: &[SeamDecl] = &[
    crate::net_mac::DECL,
    crate::net_take_frame::DECL,
    crate::net_give_frame::DECL,
    crate::net_link::DECL,
    crate::net_scan_start::DECL,
    crate::net_scan_take::DECL,
    crate::net_connect::DECL,
    crate::net_disconnect::DECL,
    crate::net_event_take::DECL,
];

/// A station MAC's length, as `net_mac` writes it.
pub const MAC_LEN: usize = 6;

/// The largest frame either side hands over: an Ethernet II frame with a
/// 1500-byte payload and no FCS.
pub const MAX_FRAME_LEN: usize = 1514;

/// The longest network name (an SSID is at most 32 bytes).
pub const MAX_SSID_LEN: usize = 32;

/// The longest password `net_connect` passes (WPA2's 63-character
/// passphrase, or a 64-digit hex key).
pub const MAX_PASSWORD_LEN: usize = 64;

/// `net_event_take`: nothing waiting.
pub const EVENT_NONE: u32 = 0;
/// `net_event_take`: joined; the link is up.
pub const EVENT_ASSOCIATED: u32 = 1;
/// `net_event_take`: the network refused the password.
pub const EVENT_AUTH_FAILED: u32 = 2;
/// `net_event_take`: nothing by that name is in range.
pub const EVENT_NOT_FOUND: u32 = 3;
/// `net_event_take`: the link went down without the station asking.
pub const EVENT_LINK_LOST: u32 = 4;
/// `net_event_take`: a scan finished; `net_scan_take` has its networks.
pub const EVENT_SCAN_DONE: u32 = 5;

/// One `net_scan_take` record's length for a name of `name_len` bytes:
/// the length byte, the name, the signal and the secure flag.
pub const fn scan_record_len(name_len: usize) -> usize {
    1 + name_len + 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_event_codes_are_distinct_and_none_is_zero() {
        let codes = [
            EVENT_NONE,
            EVENT_ASSOCIATED,
            EVENT_AUTH_FAILED,
            EVENT_NOT_FOUND,
            EVENT_LINK_LOST,
            EVENT_SCAN_DONE,
        ];
        for (i, a) in codes.iter().enumerate() {
            assert_eq!(*a, i as u32);
        }
    }

    #[test]
    fn a_scan_record_is_its_name_plus_three_bytes() {
        assert_eq!(scan_record_len(0), 3);
        assert_eq!(scan_record_len(MAX_SSID_LEN), 35);
    }
}
