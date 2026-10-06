//! [`seam_fn!`]: generate a seam function and its call shim.
//!
//! # The LTO rule
//!
//! A seam function must survive the firmware's release build (`lto = true`,
//! `opt-level = "z"`) as a real call with real arguments and a real result.
//! Left to itself, LLVM can do three things to an ordinary function that
//! break the emulator's answer, and the M0 spike watched it do them:
//!
//! 1. **delete the call** to a side-effect-free body;
//! 2. **merge two identical bodies** into one address (rustc's default
//!    `MergeFunctions`, or a linker's identical-code folding);
//! 3. **internalise the ABI**: once LTO sees every caller of a function it
//!    may drop the argument set-up it can prove the body ignores, and fold a
//!    result it can compute — so the emulator's answer in `a0` is never read.
//!
//! So firmware never writes either half by hand. The generated **seam
//! function** is `#[inline(never)] extern "C"`, exported as
//! `lp_seam_<name>`, and its first instruction is a non-`pure` `asm!` hint,
//! `addi zero, zero, <hint>`: an architectural no-op whose immediate is
//! unique per seam ([`crate::SeamDecl::hint`]), so the body cannot be deleted
//! or merged. The generated **call shim** reaches it only from an `asm!`
//! block — `call {f}` with the arguments bound to `a0..a7` and
//! `clobber_abi("C")` — so LLVM cannot see a Rust call at all, and must
//! materialise every argument and read `a0` afterwards.
//!
//! On a target that is not `riscv32` (host unit tests, the Xtensa firmwares'
//! builds of shared code) the shim calls the silicon body directly and no
//! seam function exists. Xtensa seams are the emulator seams roadmap's M7.
//!
//! # Usage
//!
//! One seam per module (one concept per file); the items are generated in
//! place, with fixed names:
//!
//! ```text
//! // seams/test_echo.rs
//! lp_seam::seam_fn! {
//!     test_echo => fn(a: u32, b: u32, c: u32) -> u32 { a ^ b ^ c }
//! }
//! // elsewhere: seams::test_echo::call(1, 2, 4)
//! ```
//!
//! generates `call(…)`, the shim firmware calls; on `riscv32` also
//! `seam_function` (exported `lp_seam_test_echo`) and `ADDRESS`, its
//! [`crate::table::Addr`] for the table entry; and `DECL`, the seam's
//! declaration.

/// Generate a seam function and its call shim. See [the module docs](self).
#[macro_export]
macro_rules! seam_fn {
    ($name:ident => fn($($arg:ident : $ty:ty),* $(,)?) -> $ret:ty $body:block) => {
        $crate::__seam_fn_common!($name => ($($arg : $ty),*) -> $ret $body);

        /// The call shim (riscv32): the seam function, reached from `asm!`.
        #[cfg(target_arch = "riscv32")]
        #[inline(always)]
        pub fn call($($arg: $ty),*) -> $ret {
            let result: $ret;
            // SAFETY: calls `seam_function`, an `extern "C"` function of
            // exactly this signature (checked against the declaration
            // above), with its arguments in `a0..` per the C ABI and every
            // caller-saved register declared clobbered.
            unsafe {
                $crate::__seam_call!(
                    f = seam_function,
                    out = [lateout("a0") result,],
                    regs = ["a0" "a1" "a2" "a3" "a4" "a5" "a6" "a7"],
                    ops = [],
                    args = [$($arg)*]
                );
            }
            result
        }
    };
    ($name:ident => fn($($arg:ident : $ty:ty),* $(,)?) $body:block) => {
        $crate::__seam_fn_common!($name => ($($arg : $ty),*) -> () $body);

        /// The call shim (riscv32): the seam function, reached from `asm!`.
        #[cfg(target_arch = "riscv32")]
        #[inline(always)]
        pub fn call($($arg: $ty),*) {
            // SAFETY: as above, with no result.
            unsafe {
                $crate::__seam_call!(
                    f = seam_function,
                    out = [],
                    regs = ["a0" "a1" "a2" "a3" "a4" "a5" "a6" "a7"],
                    ops = [],
                    args = [$($arg)*]
                );
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __seam_fn_common {
    ($name:ident => ($($arg:ident : $ty:ty),*) -> $ret:ty $body:block) => {
        /// This seam's declaration.
        #[allow(dead_code, reason = "not every seam module names its own declaration")]
        pub const DECL: $crate::SeamDecl = $crate::$name::DECL;

        /// What the seam does on silicon (and on a host build, directly).
        #[inline(always)]
        #[allow(clippy::unused_unit, reason = "a seam with no result is declared `-> ()`")]
        fn silicon($($arg: $ty),*) -> $ret $body

        /// The seam function the emulator patches and answers.
        #[cfg(target_arch = "riscv32")]
        #[unsafe(export_name = concat!("lp_seam_", stringify!($name)))]
        #[inline(never)]
        pub extern "C" fn seam_function($($arg: $ty),*) -> $ret {
            // SAFETY: writes x0 only — an architectural no-op. Not `pure`,
            // so the compiler keeps the body and every call to it; the
            // immediate is unique per seam, so nothing can fold two bodies.
            unsafe {
                ::core::arch::asm!(
                    "addi zero, zero, {hint}",
                    hint = const $crate::$name::HINT,
                    options(nomem, nostack, preserves_flags),
                );
            }
            silicon($($arg),*)
        }

        #[cfg(target_arch = "riscv32")]
        const _: $crate::$name::Signature = seam_function;

        /// The seam function's address, for its table entry.
        #[cfg(target_arch = "riscv32")]
        pub const ADDRESS: $crate::table::Addr =
            $crate::table::Addr(seam_function as *const ());

        /// The call shim (not riscv32): the silicon body, directly.
        #[cfg(not(target_arch = "riscv32"))]
        #[inline(always)]
        #[allow(clippy::unused_unit, reason = "a seam with no result is declared `-> ()`")]
        pub fn call($($arg: $ty),*) -> $ret {
            silicon($($arg),*)
        }
    };
}

/// Bind each argument to the next `a` register, then emit the one `asm!`
/// call. A ninth argument runs out of registers and fails to match: the C
/// ABI's stack arguments are not supported, on purpose.
#[doc(hidden)]
#[macro_export]
macro_rules! __seam_call {
    (
        f = $f:ident,
        out = [$($out:tt)*],
        regs = [$reg:tt $($regs:tt)*],
        ops = [$($ops:tt)*],
        args = [$arg:ident $($rest:ident)*]
    ) => {
        $crate::__seam_call!(
            f = $f,
            out = [$($out)*],
            regs = [$($regs)*],
            ops = [$($ops)* in($reg) $arg,],
            args = [$($rest)*]
        )
    };
    (
        f = $f:ident,
        out = [$($out:tt)*],
        regs = [$($regs:tt)*],
        ops = [$($ops:tt)*],
        args = []
    ) => {
        ::core::arch::asm!(
            "call {f}",
            f = sym $f,
            $($ops)*
            $($out)*
            clobber_abi("C"),
        )
    };
}
