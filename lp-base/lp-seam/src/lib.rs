//! Emulator seams: named places in the one firmware where the emulator may
//! answer.
//!
//! **Spike quality** (plan `lp2025/2026-10-05-1026-emulator-seams`, M0). The
//! shape is the one M1 is expected to salvage; the prose is not final.
//!
//! A seam is a named, `#[no_mangle] extern "C"` function in the shipped
//! firmware. On silicon it runs its real body. The emulator finds the
//! firmware's [`table`] by scanning the flash image for [`table::MAGIC`],
//! checks the table's identity against its own copy of [`SEAM_ABI_ID`], and,
//! when a seam is **engaged**, patches the seam function's first instruction
//! to `ebreak` and answers the call itself.
//!
//! # Identity, and why there is no compatibility
//!
//! Every seam is declared once, in the one [`declare!`] invocation below.
//! [`SEAM_ABI_ID`] is a 64-bit FNV-1a hash of that invocation's tokens with
//! every whitespace byte removed, computed by the compiler (part E, ID-4).
//! Plain `//` comments are not tokens and do not count; the `doc:` strings
//! are tokens and **do** count, on purpose: a change to what a seam means is
//! a change to the ABI.
//!
//! The firmware writes its `SEAM_ABI_ID` into its table; the emulator engages
//! seams only on an **exact** match with its own. There is **no** cross-build
//! compatibility: an image built from different declarations runs with no
//! seams engaged, and the emulator says so.

#![no_std]

pub mod identity;
pub mod table;

mod declare;

pub use declare::{SeamDecl, SeamKind, SeamShape};

declare! {
    seam 0x0001 ws281x_wait_step {
        kind: Performance,
        shape: Replace,
        signature: fn() -> (),
        doc: "The render thread's wait between two polls of the WS281x RMT \
              driver's completion flag (`send_blocking`'s spin). On silicon: \
              nothing observable, then return. Engaged: sleep exactly as `wfi` \
              would (until the next event that can raise an interrupt), then \
              return. The RMT model, the refill interrupt and the done \
              interrupt all run unchanged, so the wire time is billed by \
              emulated time passing.",
    }

    seam 0x7f01 probe_take {
        kind: Capability,
        shape: Switch,
        signature: fn(buf: *mut u8, cap: u32) -> u32,
        doc: "SPIKE ONLY (feature `spike_seam_wake_probe`, never merges): \
              copy up to `cap` pending probe events (u32 sequence numbers, \
              little-endian) into `buf` and return how many bytes were \
              written. On silicon: return 0. The adapter drains until it \
              returns 0 before it sleeps again.",
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
            }
        }
        assert_eq!(ws281x_wait_step::SYMBOL, "lp_seam_ws281x_wait_step");
        assert_eq!(ws281x_wait_step::KIND, SeamKind::Performance);
        assert_eq!(probe_take::SHAPE, SeamShape::Switch);
    }

    #[test]
    fn the_abi_id_is_the_hash_of_the_whitespace_free_declarations() {
        assert_eq!(SEAM_ABI_ID, identity::abi_id(DECLARATIONS));
        assert!(DECLARATIONS.contains("ws281x_wait_step"));
    }
}
