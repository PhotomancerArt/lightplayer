//! The bracket, the repetition and the record.
//!
//! Everything here is arithmetic over `u32`/`u64` plus one `log::info!`. The
//! two clocks arrive as function pointers (see the module docs) and nothing
//! in this file knows what chip it is running on.

use core::hint::black_box;

/// Repetitions per kernel. The brief's floor is five; every repetition's
/// figures reach the record, and nothing here computes a mean.
pub const REPS: usize = 5;

/// The largest group this runner can hold readings for on the stack.
/// `code_walk` is the only group above one today.
const MAX_GROUP: usize = 4;

/// The two clocks, injected. `read_cycles` is the chip's cycle counter;
/// `read_us` is a microsecond clock that is **not** derived from it.
#[derive(Clone, Copy)]
pub struct Clocks {
    pub read_cycles: fn() -> u32,
    pub read_us: fn() -> u64,
}

/// One kernel: what it is called, how much work it does, how many
/// instructions that is if the count is *exact*, and the body.
///
/// `insns_per_iter` is `None` wherever the body is compiled Rust. Guessing an
/// instruction count from source lines would put a fabricated number in a
/// calibration record, which is worse than the absence of one — the report
/// takes those sizes off the ELF instead.
pub struct Kernel {
    pub name: &'static str,
    pub iters: u32,
    pub insns_per_iter: Option<u32>,
    pub body: fn(u32) -> u32,
}

/// Kernels that only mean anything back to back share a repetition.
pub struct Group {
    pub kernels: &'static [Kernel],
}

#[derive(Clone, Copy, Default)]
struct Reading {
    cycles: u32,
    us: u64,
    acc: u32,
}

/// The empty bracket: what a measurement costs before it has measured
/// anything. Reported as its own kernel and **never subtracted here**.
fn empty_body(_iters: u32) -> u32 {
    0
}

static BRACKET_OVERHEAD: Group = Group {
    kernels: &[Kernel {
        name: "bracket_overhead",
        iters: 0,
        insns_per_iter: None,
        body: empty_body,
    }],
};

/// Measure one group: every repetition runs the group's kernels back to back,
/// and the records are emitted only once the whole group is finished.
///
/// The emission order matters. A `log::info!` between two brackets is a USB
/// write, a formatting pass and — the part that would ruin `code_walk` — a
/// walk through a good deal of flash-resident logging code. Buffering the
/// readings keeps the console out of the measurement.
fn measure_group(clocks: &Clocks, group: &Group) {
    debug_assert!(group.kernels.len() <= MAX_GROUP);
    let n = group.kernels.len().min(MAX_GROUP);
    let mut readings = [[Reading::default(); MAX_GROUP]; REPS];

    for rep in readings.iter_mut() {
        for (slot, kernel) in rep.iter_mut().zip(group.kernels.iter()).take(n) {
            let us_before = (clocks.read_us)();
            let c_before = (clocks.read_cycles)();
            let acc = (kernel.body)(black_box(kernel.iters));
            let c_after = (clocks.read_cycles)();
            let us_after = (clocks.read_us)();
            *slot = Reading {
                cycles: c_after.wrapping_sub(c_before),
                us: us_after.saturating_sub(us_before),
                acc: black_box(acc),
            };
        }
    }

    for (rep_index, rep) in readings.iter().enumerate() {
        for (slot, kernel) in rep.iter().zip(group.kernels.iter()).take(n) {
            emit(kernel, rep_index, slot);
        }
    }
}

fn emit(kernel: &Kernel, rep: usize, reading: &Reading) {
    let name = kernel.name;
    let iters = kernel.iters;
    let cycles = reading.cycles;
    let us = reading.us;
    let acc = reading.acc;

    match kernel.insns_per_iter {
        Some(per_iter) => {
            let insns = u64::from(iters) * u64::from(per_iter);
            log::info!(
                "[cycle-probe] kernel {name:<24} rep={rep} iters={iters} cycles={cycles} \
                 us={us} insns={insns} acc={acc}"
            );
            crate::emit_record_json(format_args!(
                "{{\"kind\":\"cycle-probe\",\"kernel\":\"{name}\",\"rep\":{rep},\
                 \"iters\":{iters},\"cycles\":{cycles},\"us\":{us},\"insns\":{insns},\
                 \"acc\":{acc}}}"
            ));
        }
        None => {
            log::info!(
                "[cycle-probe] kernel {name:<24} rep={rep} iters={iters} cycles={cycles} \
                 us={us} acc={acc}"
            );
            crate::emit_record_json(format_args!(
                "{{\"kind\":\"cycle-probe\",\"kernel\":\"{name}\",\"rep\":{rep},\
                 \"iters\":{iters},\"cycles\":{cycles},\"us\":{us},\"acc\":{acc}}}"
            ));
        }
    }
}

/// Run the empty bracket, then every group in order, then print the sentinel.
///
/// `group_lists` is the caller's: the firmware harness passes its own
/// chip-bound lists (the IRAM/flash pair, the mul/div chains, the MMIO polls)
/// alongside [`super::PORTABLE_GROUPS`], because only it can supply them.
pub fn run_all(clocks: &Clocks, group_lists: &[&[Group]]) {
    log::info!("[cycle-probe] === cycle probe starting, {REPS} reps per kernel ===");
    measure_group(clocks, &BRACKET_OVERHEAD);
    for list in group_lists {
        for group in list.iter() {
            measure_group(clocks, group);
        }
    }
    log::info!("{}", super::DONE_MARKER);
}

#[cfg(test)]
mod tests {
    use super::*;

    static CLOCK: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

    fn fake_cycles() -> u32 {
        CLOCK.fetch_add(7, core::sync::atomic::Ordering::Relaxed)
    }

    fn fake_us() -> u64 {
        u64::from(CLOCK.fetch_add(1, core::sync::atomic::Ordering::Relaxed)) / 160
    }

    fn adder(iters: u32) -> u32 {
        (0..iters).fold(0u32, |a, i| a.wrapping_add(i))
    }

    /// The runner does not average, does not subtract, and reaches every
    /// repetition of every kernel in a group.
    #[test]
    fn every_repetition_of_every_kernel_is_measured() {
        static GROUP: Group = Group {
            kernels: &[
                Kernel {
                    name: "a",
                    iters: 4,
                    insns_per_iter: Some(3),
                    body: adder,
                },
                Kernel {
                    name: "b",
                    iters: 4,
                    insns_per_iter: None,
                    body: adder,
                },
            ],
        };
        let clocks = Clocks {
            read_cycles: fake_cycles,
            read_us: fake_us,
        };
        // No panic, and the group's two kernels stay under the stack budget.
        measure_group(&clocks, &GROUP);
        assert_eq!(adder(4), 6);
    }

    #[test]
    fn a_group_never_exceeds_the_readings_buffer() {
        for group in super::super::PORTABLE_GROUPS {
            assert!(
                group.kernels.len() <= MAX_GROUP,
                "group starting with `{}` has {} kernels, buffer holds {MAX_GROUP}",
                group.kernels[0].name,
                group.kernels.len(),
            );
        }
    }
}
