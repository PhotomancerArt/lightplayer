//! SPIKE (`claude/s3-fluid-spike`, never merged): the fluid solver's
//! variants timed at boot, before the runtime starts. Build with
//! `LP_FLUID_BENCH=1`. One line per row:
//! `[fluid-bench] chip n iters place variant us/step sum addr`.

use lpc_engine::nodes::fluid::spike::{NumKind, VARIANTS, bench_row};

use crate::board::esp32c6::{constants::CPU_HZ, cycle_counter};

pub fn run() {
    if option_env!("LP_FLUID_BENCH").is_none() {
        return;
    }
    cycle_counter::setup();
    let cycles = || cycle_counter::read();
    // Give a host time to open the port after the reset that started us.
    esp_println::println!("[fluid-bench] starting in 8 s");
    for _ in 0..8 {
        let t0 = cycles();
        while cycles().wrapping_sub(t0) < CPU_HZ as u32 {}
    }
    for n in [32usize, 48] {
        for v in VARIANTS {
            // Soft float: a few steps are plenty, and 48² would take minutes.
            let (warm, steps) = if v.num == NumKind::F32 { (4, 2) } else { (12, 8) };
            let (cyc, sum, addr) = bench_row(v, n, 10, warm, steps, &cycles);
            let us = cyc as u64 * 1_000_000 / CPU_HZ;
            esp_println::println!(
                "[fluid-bench] esp32c6 n={n} iters=10 place=internal v={} us={us} sum={sum:#010x} addr={addr:#010x} warm={warm}",
                v.name
            );
        }
    }
    esp_println::println!("[fluid-bench] done");
}
