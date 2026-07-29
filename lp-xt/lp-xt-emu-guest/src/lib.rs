//! Guest runtime for programs running inside the `lp-xt-emu` Xtensa emulator.
//!
//! Mirrors `lp2025`'s `lp-riscv-emu-guest` module set — `entry` / `syscall` /
//! `print` / `panic` / `allocator` — adapted to the Xtensa windowed ABI and
//! the `SYSCALL`-instruction trap hosted by `lp-xt-elf` (see that crate's
//! `abi` module for the host-side contract; the two must stay in sync).
//!
//! A fixture uses it like:
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//! use lp_xt_emu_guest::{emu_main, println};
//!
//! fn main(_arg: u32) -> u32 {
//!     println!("hello");
//!     0
//! }
//! emu_main!(main);
//! ```
//!
//! There is deliberately no startup assembly: the emulator's run harness
//! synthesizes the windowed CALL8 into `_start` with SP already staged, and
//! the ELF loader materializes `.data`/`.bss` directly — so `_start` is a
//! plain windowed `extern "C"` function.

#![no_std]
#![feature(asm_experimental_arch)]

extern crate alloc;

pub mod allocator;
pub mod entry;
pub mod panic;
pub mod print;
pub mod syscall;

pub use print::_print;
pub use syscall::{exit, sys_write, syscall3, SYS_EXIT, SYS_PANIC, SYS_WRITE};
