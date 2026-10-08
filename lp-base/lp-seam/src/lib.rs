//! Emulator seams: named places in the one shipped firmware where the
//! emulator may answer.
//!
//! A seam is a named `extern "C"` function in the firmware, `lp_seam_<name>`.
//! On silicon its real body runs. The emulator finds the firmware's
//! descriptor [`table`] by scanning the flash image for [`table::MAGIC`],
//! checks the table's identity against its own copy of [`SEAM_ABI_ID`] and,
//! only when a run asked for that seam (it is **engaged**), patches the seam
//! function's first instruction to `ebreak` and answers the call itself.
//! See `README.md` beside this crate, and the ADR
//! `docs/adr/2026-10-05-emulator-seams.md`.
//!
//! What lives here, one concept per file:
//!
//! - [`declare!`] and the declaration types ([`SeamDecl`], [`SeamKind`],
//!   [`SeamShape`]) — the one invocation below is the whole seam ABI;
//! - [`identity`] — [`SEAM_ABI_ID`], a hash of that invocation;
//! - [`table`] — the descriptor table, both the firmware's layout and the
//!   emulator's reader;
//! - [`seam_fn!`] — generates a seam function and its call shim, so firmware
//!   never hand-writes either (the LTO rule);
//! - [`engaged_byte!`] — a switch-shape seam's "is it on?" byte;
//! - [`net`] — the network seam's calls and the shapes they exchange;
//! - [`wake`] — the wake line and the pending word's rules;
//! - [`test_seams`] — the reserved test id range.
//!
//! # Identity, and why there is no compatibility
//!
//! [`SEAM_ABI_ID`] is a 64-bit FNV-1a hash of the [`declare!`] invocation's
//! tokens with every whitespace byte removed, computed by the compiler.
//! Plain `//` comments are not tokens and do not count; the `doc:` strings
//! are tokens and **do** count, on purpose: a change to what a seam means is
//! a change to the ABI.
//!
//! The firmware writes its `SEAM_ABI_ID` into its table; the emulator engages
//! seams only on an **exact** match with its own. There is **no**
//! cross-build compatibility: an image built from different declarations
//! runs with no seams engaged, and the emulator says so.
//!
//! # Licence
//!
//! MIT, and outside `lp-emu/` on purpose: the firmware (AGPL) and the
//! emulator (MIT) both depend on it, and the emulator fence admits a
//! workspace crate that declares MIT.

#![no_std]

pub mod identity;
pub mod net;
pub mod table;
pub mod test_seams;
pub mod wake;

mod declare;
mod engaged_byte;
mod seam_fn;

#[doc(hidden)]
pub use declare::check as __check_declarations;
pub use declare::{SeamDecl, SeamKind, SeamShape};

declare! {
    seam 0x0001 ws281x_wait_step {
        kind: Performance,
        shape: Replace,
        signature: fn() -> (),
        doc: "The render thread's wait between two polls of the WS281x RMT \
              driver's completion flag (`send_blocking`'s spin). On silicon: \
              nothing observable, then return. Engaged: return, then sleep \
              until the next interrupt the hart would wake for (`wfi`'s wake \
              condition: asserted and enabled in `mie`). The RMT model, the \
              refill interrupt and the done interrupt all run unchanged, so \
              the wire time is billed by emulated time passing.",
    }

    seam 0x0101 net_mac {
        kind: Capability,
        shape: Switch,
        signature: fn(out: *mut u8) -> u32,
        doc: "The network seam (`net=lan`): the station's MAC. Carries the \
              network seam's engaged byte, which the firmware reads once at \
              network bring-up: 0 runs the radio, 1 plugs in the seam-backed \
              station and frame device instead. On silicon: return 0 and \
              write nothing. Engaged: write the board's 6-byte station MAC to \
              `out` and return 1.",
    }

    seam 0x0102 net_take_frame {
        kind: Capability,
        shape: Switch,
        signature: fn(buf: *mut u8, cap: u32) -> u32,
        doc: "The network seam: take one received Ethernet II frame. On \
              silicon: return 0. Engaged: copy the oldest frame waiting for \
              this board into `buf` when it fits `cap` and return its \
              length; return 0 when none is waiting (or the oldest does not \
              fit, which stays queued). One whole frame per take, never two.",
    }

    seam 0x0103 net_give_frame {
        kind: Capability,
        shape: Switch,
        signature: fn(buf: *const u8, len: u32) -> u32,
        doc: "The network seam: send one Ethernet II frame of `len` bytes \
              from `buf`, the station's MAC as its source. On silicon: \
              return 0. Engaged: return 1 when the frame was taken for the \
              LAN (it is carried only while the link is up), 0 when refused.",
    }

    seam 0x0104 net_link {
        kind: Capability,
        shape: Switch,
        signature: fn() -> u32,
        doc: "The network seam: whether the station's link is up (it is \
              associated with a network). On silicon: return 0. Engaged: \
              return 1 while associated, else 0.",
    }

    seam 0x0105 net_scan_start {
        kind: Capability,
        shape: Switch,
        signature: fn() -> u32,
        doc: "The network seam: start listening for networks. On silicon: \
              return 0. Engaged: return 1 when a scan started; a `scan done` \
              event follows a stated time later, and `net_scan_take` then \
              answers what was heard.",
    }

    seam 0x0106 net_scan_take {
        kind: Capability,
        shape: Switch,
        signature: fn(buf: *mut u8, cap: u32) -> u32,
        doc: "The network seam: the last finished scan's networks, hidden \
              ones omitted, strongest first. On silicon: return 0. Engaged: \
              write whole records into `buf` up to `cap` bytes and return how \
              many records were written. A record is: name length (u8), the \
              name's bytes, signal in dBm (i8), secure (u8, 1 = asks a \
              password).",
    }

    seam 0x0107 net_connect {
        kind: Capability,
        shape: Switch,
        signature: fn(ssid: *const u8, ssid_len: u32, password: *const u8, password_len: u32) -> u32,
        doc: "The network seam: join the network named by `ssid` with \
              `password` (empty for an open network). On silicon: return 0. \
              Engaged: return 1 when the attempt started; its outcome comes \
              as an `associated`, `auth failed` or `not found` event a stated \
              time later. The password is read from the call's buffer into \
              emulator memory and goes nowhere else.",
    }

    seam 0x0108 net_disconnect {
        kind: Capability,
        shape: Switch,
        signature: fn() -> u32,
        doc: "The network seam: leave the network the station is on. On \
              silicon: return 0. Engaged: the link goes down at once, with \
              no event, and return 1.",
    }

    seam 0x0109 net_event_take {
        kind: Capability,
        shape: Switch,
        signature: fn() -> u32,
        doc: "The network seam: the station's next event, oldest first. On \
              silicon: return 0. Engaged: 0 none waiting, 1 associated, 2 \
              auth failed, 3 not found, 4 link lost (the network left \
              without the station asking), 5 scan done.",
    }

    seam 0x7f01 test_echo {
        kind: Performance,
        shape: Replace,
        signature: fn(a: u32, b: u32, c: u32) -> u32,
        doc: "TEST ONLY: never in a shipped table. Proves arguments and a \
              result survive the real release build. On silicon: return \
              `a ^ b ^ c`. Engaged (emulator feature `test-seams`): return a \
              different value, so a test sees whose answer ran.",
    }

    seam 0x7f02 test_take {
        kind: Capability,
        shape: Switch,
        signature: fn(endpoint: u32, buf: *mut u8, cap: u32) -> u32,
        doc: "TEST ONLY: never in a shipped table. The wake consumer's shape: \
              copy up to `cap` bytes the host queued for `endpoint` into \
              `buf` and return how many were written. On silicon: return 0. \
              Its engaged byte reads 1 only when an emulator engaged it.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_declared_seams_have_unique_ids_and_prefixed_symbols() {
        for (i, a) in ALL.iter().enumerate() {
            assert!(a.symbol.starts_with("lp_seam_"), "{}", a.symbol);
            assert!(a.symbol.ends_with(a.name));
            for b in &ALL[i + 1..] {
                assert_ne!(a.id, b.id, "{} and {}", a.name, b.name);
                assert_ne!(a.hint(), b.hint(), "{} and {}", a.name, b.name);
            }
        }
        assert_eq!(ws281x_wait_step::SYMBOL, "lp_seam_ws281x_wait_step");
        assert_eq!(ws281x_wait_step::KIND, SeamKind::Performance);
        assert_eq!(test_take::SHAPE, SeamShape::Switch);
        assert_eq!(test_take::ENGAGED_SYMBOL, "LP_SEAM_ENGAGED_test_take");
    }

    #[test]
    fn the_network_seam_is_a_capability_switch_with_its_byte_on_net_mac() {
        for d in net::CALLS {
            assert_eq!(d.kind, SeamKind::Capability, "{}", d.name);
            assert_eq!(d.shape, SeamShape::Switch, "{}", d.name);
            assert!(d.name.starts_with("net_"), "{}", d.name);
            assert!(!test_seams::is_test_id(d.id), "{}", d.name);
        }
        let declared = ALL.iter().filter(|d| d.name.starts_with("net_")).count();
        assert_eq!(
            declared,
            net::CALLS.len(),
            "net::CALLS lists every net_ seam"
        );
        assert_eq!(net_mac::ENGAGED_SYMBOL, "LP_SEAM_ENGAGED_net_mac");
    }

    #[test]
    fn test_seams_and_only_test_seams_are_in_the_reserved_range() {
        for d in ALL {
            assert_eq!(
                test_seams::is_test_id(d.id),
                d.name.starts_with("test_"),
                "{}",
                d.name
            );
            if test_seams::is_test_id(d.id) {
                assert!(d.doc.starts_with(test_seams::DOC_PREFIX), "{}", d.name);
            }
        }
        assert!(!test_seams::is_test_id(ws281x_wait_step::ID));
        assert!(test_seams::is_test_id(test_echo::ID));
    }

    #[test]
    fn the_abi_id_is_the_hash_of_the_whitespace_free_declarations() {
        assert_eq!(SEAM_ABI_ID, identity::abi_id(DECLARATIONS));
        assert!(DECLARATIONS.contains("ws281x_wait_step"));
    }

    #[test]
    fn a_hint_is_the_id_in_twelve_signed_bits() {
        assert_eq!(ws281x_wait_step::HINT, 1);
        for d in ALL {
            assert!((0..=0x7ff).contains(&d.hint()), "{}", d.name);
        }
    }
}
