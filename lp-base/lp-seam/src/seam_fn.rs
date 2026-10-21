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
//! On a target that is neither `riscv32` nor `xtensa` (host unit tests) the
//! shim calls the silicon body directly and no seam function exists.
//!
//! # Xtensa (S3 radio M5)
//!
//! The same three rules, by different means, because the windowed ABI makes
//! a hand-written call the wrong tool:
//!
//! - **the hint** is `movi {scratch}, <hint>` into a register the `asm!`
//!   declares clobbered (Xtensa has no zero register to write): a real
//!   instruction with the seam's unique immediate, so the body is neither
//!   deleted nor merged. The assembler may emit it as `movi.n` when the
//!   immediate fits; an emulator accepts either, a few instructions after the
//!   function's `entry`.
//! - **the call shim** calls the seam function through a function pointer
//!   read with `read_volatile` from a static. LLVM cannot know which
//!   function it calls, so it keeps the C ABI, materialises every argument
//!   and reads the result — the three things the RV32 `asm!` call buys —
//!   and the compiler emits the ordinary windowed call (`callx8`) for it,
//!   with the window save areas and `PS.CALLINC` exactly as any call has
//!   them. An `asm!` `callx8` would have to reproduce all of that by hand.
//!
//! An emulator answers an Xtensa seam **before** the callee's `entry`
//! rotates the window: at the patched entry, `PS.CALLINC` = *n* names the
//! caller's registers — the return address in `a[4n]`, the arguments from
//! `a[4n+2]`, the result back to `a[4n+2]` — and the answer returns to the
//! caller as `retw` would, with the window never moved.
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
//! generates `call(…)`, the shim firmware calls; on `riscv32` and `xtensa` also
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

        /// The call shim (xtensa): the seam function through a function
        /// pointer LLVM cannot see through (the module docs, "Xtensa").
        #[cfg(target_arch = "xtensa")]
        #[inline(always)]
        pub fn call($($arg: $ty),*) -> $ret {
            static FUNCTION: $crate::$name::Signature = seam_function;
            // SAFETY: reads an initialised `static` of a `Copy` type.
            let f = unsafe { ::core::ptr::read_volatile(&FUNCTION) };
            f($($arg),*)
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

        /// The call shim (xtensa): as above, with no result.
        #[cfg(target_arch = "xtensa")]
        #[inline(always)]
        pub fn call($($arg: $ty),*) {
            static FUNCTION: $crate::$name::Signature = seam_function;
            // SAFETY: reads an initialised `static` of a `Copy` type.
            let f = unsafe { ::core::ptr::read_volatile(&FUNCTION) };
            f($($arg),*)
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

        /// The seam function the emulator patches and answers (xtensa).
        #[cfg(target_arch = "xtensa")]
        #[unsafe(export_name = concat!("lp_seam_", stringify!($name)))]
        #[inline(never)]
        pub extern "C" fn seam_function($($arg: $ty),*) -> $ret {
            // SAFETY: writes a scratch register the block declares clobbered
            // and nothing else. Not `pure`, so the body and every call to it
            // stay; the immediate is unique per seam, so nothing can fold
            // two bodies.
            unsafe {
                ::core::arch::asm!(
                    "movi {scratch}, {hint}",
                    scratch = out(reg) _,
                    hint = const $crate::$name::HINT,
                    options(nomem, nostack, preserves_flags),
                );
            }
            silicon($($arg),*)
        }

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

        #[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
        const _: $crate::$name::Signature = seam_function;

        /// The seam function's address, for its table entry.
        #[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
        pub const ADDRESS: $crate::table::Addr =
            $crate::table::Addr(seam_function as *const ());

        /// The call shim (neither riscv32 nor xtensa): the silicon body,
        /// directly.
        #[cfg(not(any(target_arch = "riscv32", target_arch = "xtensa")))]
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
