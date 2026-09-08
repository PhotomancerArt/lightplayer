//! `slice_shape` — the per-slice fixed cost the compile harness pays and the
//! emulator does not.
//!
//! `notes.md` F4: tick 15 is the smallest slice in the compile harness —
//! **silicon 5,153 cycles against `t1`'s 1,078**, so roughly **4,075 cycles
//! of fixed per-slice cost** that the emulator charges nothing for. The
//! candidates named there are an interrupt or yield around the slice and a
//! cold cache after the tick's log line.
//!
//! So this kernel is shaped like that tick and not like a benchmark: a small
//! compute body of about a thousand instructions, then **one log line**, then
//! the bracket closes. The log line is inside the bracket on purpose — it is
//! the part of a slice that the compute-only kernels leave out, and if the
//! fixed cost lives in the console path rather than in the slice boundary,
//! this is the kernel that says so.
//!
//! The line's content is fixed-width and deterministic, so the same bytes go
//! out on silicon and on every emulated configuration.

use core::hint::black_box;

/// Iterations of the compute body. Sized so the compute half lands near tick
/// 15's `t1` figure of 1,078 cycles rather than dwarfing the fixed cost the
/// kernel exists to expose.
pub const SLICE_ITERS: u32 = 256;

pub fn slice_shape(iters: u32) -> u32 {
    let mut a = 0x1234_5678u32;
    let mut n = iters;
    while n > 0 {
        a = (a ^ (a >> 3)).wrapping_mul(0x9E37_79B1);
        n -= 1;
    }
    let a = black_box(a);
    log::info!("[cycle-probe] slice-shape body done acc={a:08x} iters={iters}");
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_deterministic() {
        assert_eq!(slice_shape(SLICE_ITERS), slice_shape(SLICE_ITERS));
    }

    #[test]
    fn the_body_does_work() {
        assert_ne!(slice_shape(1), slice_shape(2));
    }
}
