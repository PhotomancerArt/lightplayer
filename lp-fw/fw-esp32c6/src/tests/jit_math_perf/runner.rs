//! Small benchmark runner for ESP32-C6 PMU cycle measurements.

use core::sync::atomic::{AtomicI32, Ordering};

use log::info;

use super::cycle_counter;

const WARMUP_SAMPLES: usize = 8;
const MEASURED_SAMPLES: usize = 31;

static SINK: AtomicI32 = AtomicI32::new(0);

pub struct BenchResult {
    pub median: u64,
}

pub fn run_overhead_baseline() {
    let empty = measure("overhead/empty", 1, || 0);
    let counter = measure("overhead/read-counter-pair", 1, || {
        let a = cycle_counter::read();
        let b = cycle_counter::read();
        b.wrapping_sub(a) as i32
    });
    info!(
        "[jit-math-perf] overhead summary: empty={} cycles, counter-pair={} cycles",
        empty.median, counter.median,
    );
}

pub fn measure<F>(label: &str, calls_per_sample: usize, mut body: F) -> BenchResult
where
    F: FnMut() -> i32,
{
    for _ in 0..WARMUP_SAMPLES {
        black_hole(body());
    }

    let mut samples = [0u64; MEASURED_SAMPLES];
    let mut checksum = 0i32;
    for sample in &mut samples {
        let start = cycle_counter::read();
        let value = body();
        let end = cycle_counter::read();
        checksum ^= value;
        *sample = end.wrapping_sub(start) as u64;
    }
    black_hole(checksum);

    samples.sort_unstable();
    let median = samples[MEASURED_SAMPLES / 2];
    let min = samples[0];
    let max = samples[MEASURED_SAMPLES - 1];
    let avg = samples.iter().sum::<u64>() / MEASURED_SAMPLES as u64;
    let calls = calls_per_sample.max(1) as u64;
    let per_call = median / calls;

    info!(
        "[jit-math-perf] bench {label:<32} median={median:>10} per_call={per_call:>6} \
         avg={avg:>10} min={min:>10} max={max:>10} calls={calls_per_sample:>5} checksum={checksum}",
    );

    BenchResult { median }
}

#[inline(never)]
pub fn black_hole(value: i32) {
    SINK.fetch_xor(value, Ordering::Relaxed);
}
