//! [`engaged_byte!`]: a switch-shape seam's "is it on?" byte.
//!
//! A switch-shape seam decides at start-up whether to plug in its
//! seam-backed adapter. It asks one byte: a `u8` in flash `.rodata` that is
//! `0` in the image. An emulator that engaged the seam patches that byte to
//! `1` **in the cache window** (never in the flash chip, which a ROM-up
//! bootloader hashes), exactly the way it arms a seam function's entry, and
//! re-arms it after any refill. On silicon the check is one real load and
//! one branch; there is no hook and no call.
//!
//! The read is `read_volatile`: from the program's point of view the byte is
//! an immutable zero, and only a volatile read keeps LLVM from folding it to
//! `false`. The table entry's `engaged` field holds the byte's address
//! ([`ENGAGED_ADDRESS`](self)), and its exported name is
//! `LP_SEAM_ENGAGED_<name>` so a disassembly can find it.
//!
//! ```text
//! // seams/test_take.rs, beside its seam_fn!
//! lp_seam::engaged_byte!(test_take);
//! // at start-up: if seams::test_take::engaged() { … }
//! ```

/// Declare a seam's engaged byte and its reader. See [the module docs](self).
#[macro_export]
macro_rules! engaged_byte {
    ($name:ident) => {
        const _: () = assert!(
            matches!($crate::$name::SHAPE, $crate::SeamShape::Switch),
            "only a switch-shape seam has an engaged byte"
        );

        /// `0` in the image; `1` only when an emulator engaged this seam.
        #[used]
        #[unsafe(export_name = concat!("LP_SEAM_ENGAGED_", stringify!($name)))]
        pub static ENGAGED_BYTE: u8 = 0;

        /// Whether an emulator engaged this seam. One load and one branch on
        /// silicon, where it is always `false`.
        #[inline(always)]
        pub fn engaged() -> bool {
            // SAFETY: a byte-sized read of a live static. Volatile because an
            // emulator may change the byte behind the program's back (in the
            // cache window), which is the whole point.
            unsafe { ::core::ptr::read_volatile(&raw const ENGAGED_BYTE) != 0 }
        }

        /// The engaged byte's address, for its table entry.
        pub const ENGAGED_ADDRESS: $crate::table::Addr = $crate::table::Addr::of(&ENGAGED_BYTE);
    };
}
