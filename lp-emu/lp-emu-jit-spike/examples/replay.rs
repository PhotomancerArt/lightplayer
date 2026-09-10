//! spike: replay a recorded region trace under wasmtime.
//!
//! The native twin of `target/emu-spike/replay.mjs`: same files, same loop,
//! same "time only the `run` calls" rule, so wasmtime, JavaScriptCore and V8
//! are compared on one thing — how fast the emitted module executes guest
//! instructions — with the emulator's own per-entry host work excluded from
//! all three.
//!
//!   cargo run -p lp-emu-jit-spike --example replay --release -- target/emu-spike/rec-rk

use std::time::Instant;

use lp_emu_jit_spike::translate::{PAGES, SCRATCH};
use wasmtime::{Config, Engine, Func, Instance, Memory, MemoryType, Module, Store};

const REC: usize = 316;
const TIMED_PASSES: usize = 4;

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn i32le(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn i64le(b: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "target/emu-spike/rec-rk".into());
    let dir = std::path::PathBuf::from(dir);
    let wasm = std::fs::read(std::env::args().nth(2).unwrap_or_else(|| dir.join("region.wasm").display().to_string())).expect("region.wasm");
    let snap = std::fs::read(dir.join("mem.bin")).expect("mem.bin");
    let trace = std::fs::read(dir.join("trace.bin")).expect("trace.bin");
    let mmio = std::fs::read(dir.join("mmio.bin")).expect("mmio.bin");
    let delta = std::fs::read(dir.join("delta.bin")).expect("delta.bin");
    let n = trace.len() / REC;
    assert_eq!(trace.len() % REC, 0, "trace.bin is not whole records");

    let mut config = Config::new();
    config.cranelift_opt_level(wasmtime::OptLevel::Speed);
    // Guard-page bounds checks, the way a browser engine always does them.
    // Without these wasmtime emits an explicit check on every guest load and
    // store and the comparison against JSC/V8 is not about the module at all.
    config.signals_based_traps(true);
    config.memory_reservation(1 << 32);
    config.memory_guard_size(1 << 31);
    config.memory_may_move(false);
    let engine = Engine::new(&config).expect("engine");
    let mut store = Store::new(&engine, 0usize);
    let memory = Memory::new(&mut store, MemoryType::new(PAGES as u32, Some(PAGES as u32))).expect("memory");

    // The recorded MMIO results, handed back in order. Both pinned regions
    // recorded none — they are pure compute — but the harness is exact anyway.
    let mmio_vals: Vec<i64> = mmio.chunks_exact(8).map(|c| i64le(c, 0)).collect();
    let mmio_load = Func::wrap(&mut store, move |mut c: wasmtime::Caller<'_, usize>, _: i32, _: i64, _: i32, _: i32| -> i64 {
        let i = *c.data();
        *c.data_mut() = i + 1;
        mmio_vals.get(i).copied().unwrap_or(0)
    });
    let mmio_store = Func::wrap(&mut store, |_: i32, _: i64, _: i32, _: i32, _: i32| -> i32 { 0 });

    let t0 = Instant::now();
    let module = Module::new(&engine, &wasm).expect("compile");
    let compile = t0.elapsed();
    let t1 = Instant::now();
    let instance = Instance::new(&mut store, &module, &[mmio_load.into(), mmio_store.into(), memory.into()]).expect("instantiate");
    let instantiate = t1.elapsed();
    let run = instance
        .get_typed_func::<(i32, i64, i64), i32>(&mut store, "run")
        .expect("run");

    let s = SCRATCH as usize;
    let mut best_ns = u128::MAX;
    let mut mismatches = 0usize;
    let mut instructions = 0u64;

    for pass in 0..=TIMED_PASSES {
        let check = pass == 0;
        // Reload the snapshot.
        {
            let data = memory.data_mut(&mut store);
            data.fill(0);
            let mut at = 0;
            while at < snap.len() {
                let off = u32le(&snap, at) as usize;
                let len = u32le(&snap, at + 4) as usize;
                data[off..off + len].copy_from_slice(&snap[at + 8..at + 8 + len]);
                at += 8 + len;
            }
        }
        *store.data_mut() = 0;
        let mut ns = 0u128;
        let mut instr = 0u64;
        let mut bad = 0usize;
        let mut d = 0usize;

        for i in 0..n {
            let b = i * REC;
            {
                let data = memory.data_mut(&mut store);
                // Stand in for the interpreter's writes between entries.
                let count = u32le(&delta, d) as usize;
                d += 4;
                for _ in 0..count {
                    let off = u32le(&delta, d) as usize;
                    let len = u32le(&delta, d + 4) as usize;
                    data[off..off + len].copy_from_slice(&delta[d + 8..d + 8 + len]);
                    d += 8 + len;
                }
                for r in 0..32 {
                    data[s + 4 * r..s + 4 * r + 4].copy_from_slice(&trace[b + 36 + 4 * r..b + 40 + 4 * r]);
                }
                data[s + 140..s + 144].copy_from_slice(&0i32.to_le_bytes());
                data[s + 144..s + 152].copy_from_slice(&trace[b + 20..b + 28]);
                data[s + 152..s + 160].copy_from_slice(&trace[b + 28..b + 36]);
            }
            let t = Instant::now();
            let pc = run
                .call(&mut store, (i32le(&trace, b), i64le(&trace, b + 4), i64le(&trace, b + 12)))
                .expect("run");
            ns += t.elapsed().as_nanos();
            let data = memory.data(&store);
            instr += u64::from(u32le(data, s + 136));
            if check {
                let mut ok = pc == i32le(&trace, b + 168)
                    && i64le(data, s + 128) == i64le(&trace, b + 172)
                    && u32le(data, s + 136) == u32le(&trace, b + 180)
                    && i32le(data, s + 140) == i32le(&trace, b + 184);
                if ok {
                    for r in 1..32 {
                        if i32le(data, s + 4 * r) != i32le(&trace, b + 188 + 4 * r) {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    bad += 1;
                }
            }
        }
        if check {
            mismatches = bad;
        } else {
            best_ns = best_ns.min(ns);
        }
        instructions = instr;
    }

    println!("engine            wasmtime {} (cranelift)", env!("CARGO_PKG_VERSION_MAJOR"));
    println!("dir               {}  ({} wasm bytes, {n} entries)", dir.display(), wasm.len());
    println!("compile           {:.1} ms", compile.as_secs_f64() * 1e3);
    println!("instantiate       {:.1} ms", instantiate.as_secs_f64() * 1e3);
    println!(
        "identity          {}",
        if mismatches == 0 {
            "OK — every entry matched the native recording".to_string()
        } else {
            format!("{mismatches} MISMATCHES of {n}")
        }
    );
    println!("instructions      {instructions} ({:.1} per entry)", instructions as f64 / n as f64);
    println!("best timed pass   {:.1} ms", best_ns as f64 / 1e6);
    println!(
        "throughput        {:.1} M instr/s   ({:.2} ns/instruction)",
        instructions as f64 / (best_ns as f64 / 1e3),
        best_ns as f64 / instructions as f64
    );
}
