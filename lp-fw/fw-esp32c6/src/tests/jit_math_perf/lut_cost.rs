//! Synthetic LUT access cost measurements.

extern crate alloc;

use alloc::vec::Vec;

use log::info;

use super::runner;

const TABLE_256: [i32; 256] = make_table();
const TABLE_512: [i32; 512] = make_table();
const TABLE_1024: [i32; 1024] = make_table();
const TABLE_2048: [i32; 2048] = make_table();

pub fn run() {
    info!("[jit-math-perf] --- LUT access cost ---");
    bench_rodata("lut/rodata-seq-256", &TABLE_256, AccessPattern::Sequential);
    bench_rodata("lut/rodata-stride-256", &TABLE_256, AccessPattern::Stride);
    bench_rodata("lut/rodata-random-256", &TABLE_256, AccessPattern::Random);
    bench_rodata("lut/rodata-seq-512", &TABLE_512, AccessPattern::Sequential);
    bench_rodata("lut/rodata-stride-512", &TABLE_512, AccessPattern::Stride);
    bench_rodata("lut/rodata-random-512", &TABLE_512, AccessPattern::Random);
    bench_rodata(
        "lut/rodata-seq-1024",
        &TABLE_1024,
        AccessPattern::Sequential,
    );
    bench_rodata("lut/rodata-stride-1024", &TABLE_1024, AccessPattern::Stride);
    bench_rodata("lut/rodata-random-1024", &TABLE_1024, AccessPattern::Random);
    bench_rodata(
        "lut/rodata-seq-2048",
        &TABLE_2048,
        AccessPattern::Sequential,
    );
    bench_rodata("lut/rodata-stride-2048", &TABLE_2048, AccessPattern::Stride);
    bench_rodata("lut/rodata-random-2048", &TABLE_2048, AccessPattern::Random);

    bench_ram("lut/ram-seq-1024", &TABLE_1024, AccessPattern::Sequential);
    bench_ram("lut/ram-stride-1024", &TABLE_1024, AccessPattern::Stride);
    bench_ram("lut/ram-random-1024", &TABLE_1024, AccessPattern::Random);
}

fn bench_rodata(label: &str, table: &'static [i32], pattern: AccessPattern) {
    runner::measure(label, table.len(), || sweep(table, pattern));
}

fn bench_ram(label: &str, source: &[i32], pattern: AccessPattern) {
    let mut table = Vec::with_capacity(source.len());
    table.extend_from_slice(source);
    runner::measure(label, table.len(), || sweep(&table, pattern));
}

fn sweep(table: &[i32], pattern: AccessPattern) -> i32 {
    let mut acc = 0i32;
    let mut state = 0x1234_5678u32;
    for i in 0..table.len() {
        let index = match pattern {
            AccessPattern::Sequential => i,
            AccessPattern::Stride => i.wrapping_mul(17) & (table.len() - 1),
            AccessPattern::Random => {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                state as usize & (table.len() - 1)
            }
        };
        let value = unsafe { core::ptr::read_volatile(table.as_ptr().add(index)) };
        acc = acc.wrapping_add(value);
    }
    acc
}

#[derive(Clone, Copy)]
enum AccessPattern {
    Sequential,
    Stride,
    Random,
}

const fn make_table<const N: usize>() -> [i32; N] {
    let mut table = [0i32; N];
    let mut i = 0usize;
    while i < N {
        table[i] = ((i as i32).wrapping_mul(1_103_515_245)).rotate_left((i & 31) as u32);
        i += 1;
    }
    table
}
