//! SPIKE (`claude/s3-fluid-spike`, never merged): the fluid solver's
//! variants timed at boot, before the runtime starts (no interrupts, no
//! scheduler). Build with `LP_FLUID_BENCH=1`. One line per row:
//! `[fluid-bench] chip n iters place variant us/step sum addr`.

use core::sync::atomic::Ordering;

use lpc_engine::nodes::fluid::spike::{VARIANTS, bench_row};

use crate::board::esp32s3::{constants::CPU_HZ, cycle_counter};

pub fn run() {
    if option_env!("LP_FLUID_BENCH").is_none() {
        return;
    }
    let cycles = || cycle_counter::read();
    // Give a host time to open the port after the reset that started us.
    esp_println::println!("[fluid-bench] starting in 8 s");
    for _ in 0..8 {
        let t0 = cycles();
        while cycles().wrapping_sub(t0) < CPU_HZ as u32 {}
    }
    crate::fluid_pie::run();
    let before = esp_alloc::EXTERNAL_FIRST_MIN.load(Ordering::Relaxed);
    for n in [32usize, 48] {
        for (place, min) in [("psram", before), ("internal", usize::MAX)] {
            esp_alloc::EXTERNAL_FIRST_MIN.store(min, Ordering::Relaxed);
            for v in VARIANTS {
                let (cyc, sum, addr) = bench_row(v, n, 10, 12, 8, &cycles);
                let us = cyc as u64 * 1_000_000 / CPU_HZ;
                esp_println::println!(
                    "[fluid-bench] esp32s3 n={n} iters=10 place={place} v={} us={us} sum={sum:#010x} addr={addr:#010x}",
                    v.name
                );
            }
        }
    }
    esp_alloc::EXTERNAL_FIRST_MIN.store(before, Ordering::Relaxed);
    esp_println::println!("[fluid-bench] done");
}
