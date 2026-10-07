//! How Studio writes its lp-link frames to a board over Web Bluetooth: one
//! policy per link, chosen when the link is made.
//!
//! A policy is two numbers that only make sense together:
//!
//! - **The write kind for data frames** ([`DataWrites`]). Every SYN and every
//!   ACK-only frame is written WITH response, whatever the policy; the
//!   policy decides the data frames (reliable data and datagrams), which are
//!   nearly every byte of an update or an upload.
//! - **The cap on frames in flight** ([`BleWritePolicy::in_flight`]): this
//!   end's lp-link transmit window. The board's own receive window (8 while
//!   its engine runs, 32 core-only) lowers it further on that link.
//!
//! **Why without response, and why a cap (the OTA speed pass, 2026-10-06).**
//! A write with response costs a radio round trip before the next one may
//! start: Studio over Bluetooth moved 1.3–3.8 KiB/s in the M7 silicon
//! pre-walk, a 25-minute first update, where the desk pipe — the same board,
//! the same Mac Chrome, writes without response — moved 10–20 KiB/s. Unpaced
//! writes without response are not a transport: Chrome resolves each at
//! once and macOS drops what overflows its queue (two thirds lost in the BLE
//! M2 spike's Run B). Paced by the link's window they are: the OTA spike
//! (S5c) lost little up to ~16 in flight and 20–36 % above it, and lp-link
//! resends what is lost. So the cap IS the pacing.
//!
//! **What stays with response, and why** (#880's reasons, kept):
//!
//! - **SYN and ACK-only frames.** A handshake and a pure acknowledgement are
//!   rare and small; with response they are never lost to the Mac's queue,
//!   and a SYN that fails is how a dead connection shows itself early
//!   (`browser_ble.js` rule 5).
//! - **Every frame while the link is stalled** (nothing heard from the board
//!   for the link's stall time). A write without response can "succeed"
//!   into a link only the page still believes in — Bluefy's phantom drop
//!   (G4, 2026-09-25) — and only a write with response fails there, which
//!   tears the connection down and reconnects (rule 5). Data alone, the
//!   phantom would cost lp-link's whole retry budget before its reset's SYN
//!   found it.
//! - **The one-at-a-time write chain.** Web Bluetooth refuses a second GATT
//!   operation while one is in flight, of either kind.
//!
//! **Per browser** ([`BleWritePolicy::default_for_browser`]): a desktop
//! browser gets the spike's measured best ([`BleWritePolicy::DESKTOP`]); a
//! browser on iOS (Bluefy — nothing else there has Web Bluetooth) gets data
//! without response at half that cap ([`BleWritePolicy::IOS`]): CoreBluetooth
//! drops a write without response when its queue is full, the same failure
//! the Mac has, and no iOS central has been measured yet, so the first
//! measurement starts where the spike's loss was lowest (window 8: 0.6–2.1 %
//! resent). Studio's dev-only `?ble-writes=` flag replaces the default for a
//! page ([`BleWritePolicy::parse`]), so a browser that loses too much drops
//! to a lower cap or back to [`BleWritePolicy::WITH_RESPONSE`] — #880's
//! behaviour exactly — without a build.

use std::cell::Cell;

use lpc_wire::lp_link::frame::{FrameKind, Header};

/// How data frames are written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataWrites {
    /// `writeValueWithResponse`: one radio round trip per frame (#880).
    WithResponse,
    /// `writeValueWithoutResponse`, paced by the link's window.
    WithoutResponse,
}

/// One link's write policy. See the module docs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BleWritePolicy {
    pub data_writes: DataWrites,
    /// Frames this end keeps unacknowledged: its lp-link transmit window.
    pub in_flight: u8,
}

/// The largest cap a policy may ask for: the board's own largest receive
/// window (core-only's), past which nothing more is ever in flight.
pub const MAX_IN_FLIGHT: u8 = 32;

thread_local! {
    /// The page's `?ble-writes=` choice, if any; links made after it take it.
    static OVERRIDE: Cell<Option<BleWritePolicy>> = const { Cell::new(None) };
}

impl BleWritePolicy {
    /// A desktop browser: the OTA spike's fastest measured setting on Mac
    /// Chrome (S5c, window 16, unpaced data without response).
    pub const DESKTOP: Self = Self {
        data_writes: DataWrites::WithoutResponse,
        in_flight: 16,
    };

    /// A browser on iOS (Bluefy): data without response at the spike's
    /// lowest-loss window. Unmeasured on iOS; see the module docs.
    pub const IOS: Self = Self {
        data_writes: DataWrites::WithoutResponse,
        in_flight: 8,
    };

    /// #880's policy: every frame with response, window 16 (M7 DS11).
    pub const WITH_RESPONSE: Self = Self {
        data_writes: DataWrites::WithResponse,
        in_flight: 16,
    };

    /// The default for a browser family, as `browser_ble.js`'s
    /// `browserKind()` names it (`"ios"`, `"brave"`, `"other"`, …).
    pub fn default_for_browser(family: &str) -> Self {
        match family {
            "ios" => Self::IOS,
            _ => Self::DESKTOP,
        }
    }

    /// Read a `?ble-writes=` value: `without-response` or `with-response`,
    /// optionally `:<in flight>` (1–[`MAX_IN_FLIGHT`]); the cap defaults to
    /// [`Self::DESKTOP`]'s. `None` for anything else.
    pub fn parse(value: &str) -> Option<Self> {
        let (kind, cap) = match value.trim().split_once(':') {
            Some((kind, cap)) => (kind, Some(cap)),
            None => (value.trim(), None),
        };
        let data_writes = match kind {
            "without-response" => DataWrites::WithoutResponse,
            "with-response" => DataWrites::WithResponse,
            _ => return None,
        };
        let in_flight = match cap {
            None => Self::DESKTOP.in_flight,
            Some(cap) => match cap.trim().parse::<u8>() {
                Ok(n) if (1..=MAX_IN_FLIGHT).contains(&n) => n,
                _ => return None,
            },
        };
        Some(Self {
            data_writes,
            in_flight,
        })
    }

    /// Whether `frame` (one whole lp-link frame) goes out with response:
    /// every SYN and ACK-only frame, every frame while the link is
    /// `stalled`, and data frames only under [`DataWrites::WithResponse`].
    pub fn with_response(&self, frame: &[u8], stalled: bool) -> bool {
        if stalled || self.data_writes == DataWrites::WithResponse {
            return true;
        }
        !matches!(
            Header::parse(frame).map(|header| header.kind),
            Some(FrameKind::Data | FrameKind::Datagram)
        )
    }

    /// One line for the console: what this link does.
    pub fn describe(&self) -> String {
        let kind = match self.data_writes {
            DataWrites::WithResponse => "with response",
            DataWrites::WithoutResponse => "without response",
        };
        format!("data frames {kind}, at most {} in flight", self.in_flight)
    }
}

/// Replace every later link's default policy with `policy` (`None`: back to
/// the browser's default). Studio's `?ble-writes=` calls it at page load.
pub fn set_ble_write_policy_override(policy: Option<BleWritePolicy>) {
    OVERRIDE.with(|cell| cell.set(policy));
}

/// The page's override, if one was set.
pub fn ble_write_policy_override() -> Option<BleWritePolicy> {
    OVERRIDE.with(Cell::get)
}

/// The policy a link made now takes in a browser of `family`.
pub fn ble_write_policy_for(family: &str) -> BleWritePolicy {
    ble_write_policy_override().unwrap_or_else(|| BleWritePolicy::default_for_browser(family))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::lp_link::frame::Header;

    #[test]
    fn data_goes_without_response_and_syn_and_ack_with_it() {
        let policy = BleWritePolicy::DESKTOP;
        assert!(!policy.with_response(&frame(FrameKind::Data), false));
        assert!(!policy.with_response(&frame(FrameKind::Datagram), false));
        assert!(policy.with_response(&frame(FrameKind::Ack), false));
        assert!(policy.with_response(&frame(FrameKind::Syn), false));
        assert!(
            policy.with_response(&[], false),
            "unreadable: the safe kind"
        );
    }

    #[test]
    fn a_stalled_link_writes_everything_with_response() {
        let policy = BleWritePolicy::DESKTOP;
        assert!(policy.with_response(&frame(FrameKind::Data), true));
    }

    #[test]
    fn the_with_response_policy_is_880s() {
        let policy = BleWritePolicy::WITH_RESPONSE;
        assert!(policy.with_response(&frame(FrameKind::Data), false));
        assert_eq!(policy.in_flight, 16);
    }

    #[test]
    fn ios_starts_at_half_the_desktop_cap() {
        assert_eq!(
            BleWritePolicy::default_for_browser("ios"),
            BleWritePolicy::IOS
        );
        assert_eq!(BleWritePolicy::IOS.in_flight, 8);
        for family in ["other", "brave", "firefox", "safari", ""] {
            assert_eq!(
                BleWritePolicy::default_for_browser(family),
                BleWritePolicy::DESKTOP
            );
        }
    }

    #[test]
    fn the_flag_reads_a_kind_and_an_optional_cap() {
        assert_eq!(
            BleWritePolicy::parse("with-response"),
            Some(BleWritePolicy::WITH_RESPONSE)
        );
        assert_eq!(
            BleWritePolicy::parse("without-response"),
            Some(BleWritePolicy::DESKTOP)
        );
        assert_eq!(
            BleWritePolicy::parse("without-response:4"),
            Some(BleWritePolicy {
                data_writes: DataWrites::WithoutResponse,
                in_flight: 4
            })
        );
        assert_eq!(
            BleWritePolicy::parse("with-response:32").map(|p| p.in_flight),
            Some(32)
        );
        for bad in [
            "",
            "fast",
            "without-response:0",
            "without-response:33",
            "with-response:x",
        ] {
            assert_eq!(BleWritePolicy::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_override_replaces_every_browsers_default() {
        set_ble_write_policy_override(Some(BleWritePolicy::WITH_RESPONSE));
        assert_eq!(ble_write_policy_for("other"), BleWritePolicy::WITH_RESPONSE);
        assert_eq!(ble_write_policy_for("ios"), BleWritePolicy::WITH_RESPONSE);
        set_ble_write_policy_override(None);
        assert_eq!(ble_write_policy_for("ios"), BleWritePolicy::IOS);
    }

    fn frame(kind: FrameKind) -> Vec<u8> {
        let header = Header {
            kind,
            fin: true,
            first: true,
            chan: 3,
            seq: 1,
            ack: 0,
            win: 8,
        };
        let mut frame = header.to_bytes().to_vec();
        frame.extend_from_slice(&[0; 12]);
        frame
    }
}
