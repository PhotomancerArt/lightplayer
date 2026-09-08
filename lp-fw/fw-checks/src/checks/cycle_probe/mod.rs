//! The `cycle-probe` payload: one kernel per term of a cycle model.
//!
//! Every other timing payload in this crate measures a *workload*. This one
//! measures a model's **terms**: a kernel here exists to move exactly one
//! cost and nothing else, so that the difference between silicon and an
//! emulated configuration can be attributed rather than admired. A kernel
//! that measures two things at once is worse than no kernel, and the table
//! below is written to that rule.
//!
//! | kernel | isolates |
//! |---|---|
//! | `bracket_overhead` | what a measurement costs before it measures |
//! | `iram_loop` | the instruction-cost floor, no flash in the path |
//! | `flash_loop` | the same loop, flash-resident: the cost of *fetch* |
//! | `code_walk/{cold,warm}` | fetch misses over code larger than any cache |
//! | `rodata_stride/<n>` | the *load* side of the cache, per stride |
//! | `mmio_poll/<block>` | the APB access cost, per block |
//! | `muldiv/{mul,div}` | the per-class costs the `t2` model already claims |
//! | `slice_shape` | the ≈4,075-cycle per-slice term (`notes.md` F4) |
//!
//! ## Two clocks, always
//!
//! Every bracket reads **both** clocks: the chip's cycle counter (`mpccr` on
//! the C6 — `fw-esp32c6`'s `board::esp32c6::cycle_counter`) and a microsecond
//! clock that does not come from it (SYSTIMER, through
//! `embassy_time::Instant`). The microsecond reads sit *outside* the cycle
//! reads on both sides, so `us` never covers less than `cycles` does.
//!
//! The second clock is on the record so that no future reader can revive a
//! "the counter pauses under X" story: if the two disagree about how long a
//! kernel took, **that disagreement is the finding**, not a fixture problem.
//! Plan one spent a sitting on exactly such a story and it turned out to be a
//! link mismatch (`notes.md` F3).
//!
//! ## What stays outside this crate
//!
//! The doctrine `jit_math_perf` documents, applied a little wider: a chip
//! fact is injected, and arithmetic over bytes lives here.
//!
//! - Reading either clock is a chip fact — injected as [`Clocks`], two plain
//!   function pointers.
//! - **Where a function lives**, IRAM against flash, is a chip fact: it is an
//!   `#[esp_hal::ram]` attribute and a linker section. So the
//!   `iram_loop`/`flash_loop` pair comes from the firmware harness. So does
//!   any loop whose instruction count must be *known* rather than guessed,
//!   because that means inline assembly, which means an ISA.
//! - **An MMIO address** is a chip fact, so the poll loops come from the
//!   harness too.
//!
//! What lives here is the bracket, the repetition, the record, and the three
//! kernels that are ordinary Rust with no address in them: the flash-resident
//! code walk, the `.rodata` stride walk and the slice-shaped case.
//!
//! ## Reading the record
//!
//! One `[fw-check-json] {"kind":"cycle-probe", …}` line per kernel per
//! repetition. Repetitions are **never** averaged here: variance on silicon
//! is data, and a mean would throw away the only evidence that a kernel was
//! disturbed. `bracket_overhead` is reported and **never subtracted
//! silently** — a consumer that wants it subtracted subtracts it in the open.
//!
//! `insns` appears only where the count is exact: an assembly loop of known
//! length times its iteration count. It is absent — not guessed — for the
//! kernels whose bodies are compiled Rust, whose sizes are measured off the
//! ELF and recorded in the calibration report instead.

mod code_walk;
mod rodata;
mod runner;
mod slice_shape;

pub use code_walk::{CODE_WALK_STEPS, walk_once};
pub use rodata::{ACCESSES, RODATA_BYTES, STRIDES};
pub use runner::{Clocks, Group, Kernel, REPS, run_all};
pub use slice_shape::{SLICE_ITERS, slice_shape};

/// The sentinel line. The payload runs to completion.
pub const DONE_MARKER: &str = "[cycle-probe] === DONE ===";

/// The kernels that need no chip fact, one group each.
///
/// `code_walk` is **one group of two kernels** on purpose: `cold` and `warm`
/// only mean anything back to back, so they share a repetition. Repetition 0
/// is the only genuinely cold pass — every later `cold` reading walks code the
/// previous repetition's `warm` pass has just touched — and the spread between
/// repetition 0 and the rest is itself part of the measurement.
pub static PORTABLE_GROUPS: &[Group] = &[
    Group {
        kernels: &[
            Kernel {
                name: "code_walk/cold",
                iters: CODE_WALK_STEPS,
                insns_per_iter: None,
                body: walk_once,
            },
            Kernel {
                name: "code_walk/warm",
                iters: CODE_WALK_STEPS,
                insns_per_iter: None,
                body: walk_once,
            },
        ],
    },
    Group {
        kernels: &[Kernel {
            name: "rodata_stride/16",
            iters: ACCESSES,
            insns_per_iter: None,
            body: rodata::stride_walk_16,
        }],
    },
    Group {
        kernels: &[Kernel {
            name: "rodata_stride/32",
            iters: ACCESSES,
            insns_per_iter: None,
            body: rodata::stride_walk_32,
        }],
    },
    Group {
        kernels: &[Kernel {
            name: "rodata_stride/64",
            iters: ACCESSES,
            insns_per_iter: None,
            body: rodata::stride_walk_64,
        }],
    },
    Group {
        kernels: &[Kernel {
            name: "rodata_stride/256",
            iters: ACCESSES,
            insns_per_iter: None,
            body: rodata::stride_walk_256,
        }],
    },
    Group {
        kernels: &[Kernel {
            name: "rodata_stride/1024",
            iters: ACCESSES,
            insns_per_iter: None,
            body: rodata::stride_walk_1024,
        }],
    },
    Group {
        kernels: &[Kernel {
            name: "rodata_stride/4096",
            iters: ACCESSES,
            insns_per_iter: None,
            body: rodata::stride_walk_4096,
        }],
    },
    Group {
        kernels: &[Kernel {
            name: "slice_shape",
            iters: SLICE_ITERS,
            insns_per_iter: None,
            body: slice_shape,
        }],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Two kernels that share a name would collapse into one column of the
    /// calibration table without anyone noticing.
    #[test]
    fn portable_kernel_names_are_unique() {
        let mut seen: [&str; 16] = [""; 16];
        let mut n = 0usize;
        for group in PORTABLE_GROUPS {
            for kernel in group.kernels {
                for name in seen.iter().take(n) {
                    assert_ne!(*name, kernel.name, "duplicate kernel name");
                }
                seen[n] = kernel.name;
                n += 1;
            }
        }
        assert_eq!(n, 9, "the portable kernel count moved; update the report");
    }

    /// One stride kernel per declared stride, named after it. The names are
    /// spelled out rather than formatted, so that a stride list edited
    /// without a matching kernel fails here instead of silently dropping a
    /// row from the stride curve.
    #[test]
    fn every_stride_has_a_kernel() {
        const NAMED: [(usize, &str); 6] = [
            (16, "rodata_stride/16"),
            (32, "rodata_stride/32"),
            (64, "rodata_stride/64"),
            (256, "rodata_stride/256"),
            (1024, "rodata_stride/1024"),
            (4096, "rodata_stride/4096"),
        ];
        for (stride, name) in NAMED {
            assert!(STRIDES.contains(&stride), "stride {stride} left the list");
            let mut found = false;
            for group in PORTABLE_GROUPS {
                for kernel in group.kernels {
                    found |= kernel.name == name;
                }
            }
            assert!(found, "no kernel named {name}");
        }
        assert_eq!(STRIDES.len(), NAMED.len(), "a stride has no kernel");
    }
}
