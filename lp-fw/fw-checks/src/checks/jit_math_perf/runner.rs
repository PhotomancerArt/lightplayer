//! Small benchmark runner for cycle-counter measurements.
//!
//! Reading the cycle counter is not portable arithmetic — the ESP32-C6 has no
//! standard RISC-V Zicntr CSR backing it, only an Espressif PMU register
//! (`board::esp32c6::cycle_counter` in `fw-esp32c6`) — so it is injected as a
//! plain function pointer rather than read here. Everything downstream of one
//! `start`/`end` pair (the statistics, the log line, the JSON record) is
//! ordinary arithmetic over `u64`s and lives in this crate.

use core::sync::atomic::{AtomicI32, Ordering};

const WARMUP_SAMPLES: usize = 8;
const MEASURED_SAMPLES: usize = 31;

static SINK: AtomicI32 = AtomicI32::new(0);

/// One bench's cycle statistics.
pub struct BenchStats {
    pub median: u64,
    pub per_call: u64,
    pub avg: u64,
    pub min: u64,
    pub max: u64,
    pub checksum: i32,
}

/// Measure `body`, worth `calls_per_sample` calls of work each sample, using
/// `read_cycles` as the clock. Logs a human line, emits a `jit-bench`
/// `[fw-check-json]` record, and returns the stats besides.
pub fn measure<F>(
    label: &str,
    calls_per_sample: usize,
    read_cycles: fn() -> u32,
    mut body: F,
) -> BenchStats
where
    F: FnMut() -> i32,
{
    for _ in 0..WARMUP_SAMPLES {
        black_hole(body());
    }

    let mut samples = [0u64; MEASURED_SAMPLES];
    let mut checksum = 0i32;
    for sample in &mut samples {
        let start = read_cycles();
        let value = body();
        let end = read_cycles();
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

    log::info!(
        "[jit-math-perf] bench {label:<32} median={median:>10} per_call={per_call:>6} \
         avg={avg:>10} min={min:>10} max={max:>10} calls={calls_per_sample:>5} checksum={checksum}",
    );
    crate::emit_record_json(format_args!(
        "{{\"kind\":\"jit-bench\",\"label\":\"{label}\",\"calls\":{calls_per_sample},\
         \"median\":{median},\"per_call\":{per_call},\"avg\":{avg},\"min\":{min},\"max\":{max},\
         \"checksum\":{checksum}}}",
    ));

    BenchStats {
        median,
        per_call,
        avg,
        min,
        max,
        checksum,
    }
}

pub fn run_overhead_baseline(read_cycles: fn() -> u32) {
    let empty = measure("overhead/empty", 1, read_cycles, || 0);
    let counter = measure("overhead/read-counter-pair", 1, read_cycles, || {
        let a = read_cycles();
        let b = read_cycles();
        b.wrapping_sub(a) as i32
    });
    log::info!(
        "[jit-math-perf] overhead summary: empty={} cycles, counter-pair={} cycles",
        empty.median,
        counter.median,
    );
}

#[inline(never)]
pub fn black_hole(value: i32) {
    SINK.fetch_xor(value, Ordering::Relaxed);
}
