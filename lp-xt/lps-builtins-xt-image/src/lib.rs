//! The Xtensa builtins image, embedded at build time.
//!
//! The image is a linked guest executable carrying every `__lps_*` builtin at
//! the addresses `lp-xt-emu` models — the base image a host emulation engine
//! links compiled shader code against. It is produced by the DEVICE-target crate
//! `lp-xt/lps-builtins-xt-app` (esp toolchain) via
//! `scripts/build-builtins-xt.sh`, and is **not** checked in.
//!
//! When it has not been built, [`image`] returns an empty slice. That is a
//! first-class state, not an error: the workspace must build and test on a
//! machine with no esp toolchain, so consumers check [`is_available`] and skip
//! the Xtensa host path with a loud note rather than failing.
//!
//! ## Why this is its own crate
//!
//! Two reasons, both deliberate:
//!
//! - `lp-shader/*` crates are **sans-IO** (`docs/adr/2026-07-06-sans-io-core.md`),
//!   so the consumer cannot read the ELF from a path at runtime. Embedding at
//!   build time is the compliant answer, mirroring how `lpvm-cranelift/build.rs`
//!   embeds the rv32 image.
//! - The consumer, `lpvm-native`, is also compiled for device firmware. A build
//!   script there would run on every firmware build to do nothing.

include!(concat!(env!("OUT_DIR"), "/xt_builtins_image.rs"));

/// The embedded image bytes, or an empty slice when it was not built.
pub fn image() -> &'static [u8] {
    IMAGE_BYTES
}

/// Whether the image was available at build time.
pub fn is_available() -> bool {
    !IMAGE_BYTES.is_empty()
}

/// The command that produces the image, for skip messages.
pub const BUILD_COMMAND: &str = "scripts/build-builtins-xt.sh";

#[cfg(test)]
mod tests {
    use super::*;

    /// The image is optional, so this cannot assert it is present — but when it
    /// *is* present it must be a plausible Xtensa ELF32, since a truncated or
    /// wrong-arch embed would otherwise surface far from here.
    #[test]
    fn embedded_image_is_either_absent_or_a_little_endian_elf32() {
        if !is_available() {
            eprintln!("SKIP: image not built — run {BUILD_COMMAND}");
            return;
        }
        let b = image();
        assert_eq!(&b[..4], b"\x7fELF", "not an ELF magic");
        assert_eq!(b[4], 1, "expected ELF32 (EI_CLASS=1)");
        assert_eq!(b[5], 1, "expected little-endian (EI_DATA=1)");
        // e_machine at offset 18: EM_XTENSA = 94.
        assert_eq!(
            u16::from_le_bytes([b[18], b[19]]),
            94,
            "expected EM_XTENSA (94)"
        );
    }
}
