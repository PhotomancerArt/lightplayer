//! The chip emulator tests' **firmware-derived figures**, read from a
//! committed per-chip record instead of from literals in test source.
//!
//! A figure is a value the emulator reads off the **shipped firmware image**
//! that any change to that image can move: a cycle count to a boot line, the
//! main stack's size (the residual of RAM after `.data`/`.bss`), the bytes of
//! a boot chain that prints that size, a heap total. They are pinned exactly —
//! a moved figure fails its test — but they live in
//! `lp-emu/esp/figures/<chip>.json`, not in the test, so accepting a firmware
//! change is one command (`just bless-chips <chip>`) and one reviewable diff
//! rather than a hunt through three crates' tests.
//!
//! What is **not** a figure, and stays a literal in its test: anything
//! compared against silicon or a transcript, anything read off a pinned
//! reference image (a fixed commit's bytes cannot move), structural
//! constants (a `[JIT]` line of zeros, a `retry_saves=0`), and model
//! constants. Those are identity claims; a move there is a finding and a
//! bless must never be able to rewrite one.
//!
//! ```ignore
//! let mut f = Figures::new("esp32v3", "boot_idle::the_init_chain_comes_out_of_the_wire_byte_for_byte");
//! f.utf8("init_chain.prefix", &bytes);
//! f.int("init_chain.prefix.cycles", machine.cycles());
//! f.verify(); // fails naming every moved figure, old → new, and the bless command
//! ```
//!
//! With `LP_EMU_BLESS=1` in the environment, [`Figures::verify`] rewrites the
//! record instead of failing. `just bless-chips` is what sets it; see
//! `docs/chip-figures.md`.

mod figures;
mod record;

pub use figures::{BLESS_ENV, Figures, record_path};
pub use record::{Record, Value};
