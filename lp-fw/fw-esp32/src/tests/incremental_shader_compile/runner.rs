//! Runs the incremental shader compile corpus and logs per-tick timing/memory.

use log::{info, warn};
use lp_shader::{LpsEngine, ShaderCompileBudget, ShaderCompileStepResult};
use lpvm_native::NativeJitEngine;

use super::cycle_counter;
use super::shader_compile_case::{SHADER_COMPILE_CASES, ShaderCompileCase};

const TARGET_TICK_US: u64 = 5_000;
const COMPILE_BUDGET: ShaderCompileBudget = ShaderCompileBudget {
    frontend_steps: 1,
    backend_steps: 1,
};

struct CaseSummary {
    name: &'static str,
    tick_count: u32,
    total_us: u64,
    max_slice_us: u64,
    peak_used: usize,
    resident_used: usize,
    after_drop_used: usize,
}

pub fn run_all(engine: &LpsEngine<NativeJitEngine>) {
    info!(
        "[inc-shader-compile] compile budget: frontend_steps={} backend_steps={}",
        COMPILE_BUDGET.frontend_steps, COMPILE_BUDGET.backend_steps,
    );
    let mut total_build_us = 0u64;
    let mut worst_slice_us = 0u64;
    let mut worst_peak_used = 0usize;
    for case in SHADER_COMPILE_CASES {
        let summary = run_case(engine, case);
        total_build_us = total_build_us.saturating_add(summary.total_us);
        worst_slice_us = worst_slice_us.max(summary.max_slice_us);
        worst_peak_used = worst_peak_used.max(summary.peak_used);
        info!(
            "[inc-shader-compile] summary case={} build={} ticks={} max_slice={} peak={} resident={} after_drop={}",
            summary.name,
            fmt_ms_1(summary.total_us),
            summary.tick_count,
            fmt_ms_1(summary.max_slice_us),
            fmt_kib_1(summary.peak_used),
            fmt_kib_1(summary.resident_used),
            fmt_kib_1(summary.after_drop_used),
        );
        fw_checks::emit_record_json(format_args!(
            "{{\"kind\":\"case-summary\",\"check\":\"shader-compile-stress\",\"case\":\"{}\",\"build_us\":{},\"ticks\":{},\"max_slice_us\":{},\"peak_used\":{},\"resident_used\":{},\"after_drop_used\":{}}}",
            summary.name,
            summary.total_us,
            summary.tick_count,
            summary.max_slice_us,
            summary.peak_used,
            summary.resident_used,
            summary.after_drop_used,
        ));
    }
    info!(
        "[inc-shader-compile] summary total_build={} cases={} worst_slice={} worst_peak={}",
        fmt_ms_1(total_build_us),
        SHADER_COMPILE_CASES.len(),
        fmt_ms_1(worst_slice_us),
        fmt_kib_1(worst_peak_used),
    );
    fw_checks::emit_record_json(format_args!(
        "{{\"kind\":\"total-summary\",\"check\":\"shader-compile-stress\",\"build_us\":{},\"cases\":{},\"worst_slice_us\":{},\"worst_peak_used\":{}}}",
        total_build_us,
        SHADER_COMPILE_CASES.len(),
        worst_slice_us,
        worst_peak_used,
    ));
}

fn fmt_ms_1(us: u64) -> FmtMs1 {
    FmtMs1 { us }
}

fn fmt_kib_1(bytes: usize) -> FmtKiB1 {
    FmtKiB1 { bytes }
}

struct FmtMs1 {
    us: u64,
}

impl core::fmt::Display for FmtMs1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let tenths_ms = (self.us + 50) / 100;
        write!(f, "{}.{}ms", tenths_ms / 10, tenths_ms % 10)
    }
}

struct FmtKiB1 {
    bytes: usize,
}

impl core::fmt::Display for FmtKiB1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let tenths_kib = (self.bytes.saturating_mul(10) + 512) / 1024;
        write!(f, "{}.{}KiB", tenths_kib / 10, tenths_kib % 10)
    }
}

fn run_case(engine: &LpsEngine<NativeJitEngine>, case: &ShaderCompileCase) -> CaseSummary {
    info!("[inc-shader-compile] --- case={} ---", case.name);

    let mut job = engine.start_compile_px_job(case.desc());
    let start_free = esp_alloc::HEAP.free();
    let start_used = esp_alloc::HEAP.used();
    let mut peak_used = start_used;
    let mut max_slice_cycles = 0u64;
    let mut total_cycles = 0u64;
    let mut tick_count = 0u32;

    loop {
        tick_count = tick_count.saturating_add(1);
        let stage = job.stage();
        let before_free = esp_alloc::HEAP.free();
        let before_used = esp_alloc::HEAP.used();
        let cycle_start = cycle_counter::read();
        let step_result = job.step(COMPILE_BUDGET);
        let slice_cycles = cycle_counter::read().wrapping_sub(cycle_start) as u64;
        let slice_us = cycle_counter::cycles_to_us(slice_cycles);
        let after_free = esp_alloc::HEAP.free();
        let after_used = esp_alloc::HEAP.used();

        peak_used = peak_used.max(after_used);
        max_slice_cycles = max_slice_cycles.max(slice_cycles);
        total_cycles = total_cycles.saturating_add(slice_cycles);

        info!(
            "[inc-shader-compile] case={} tick={} stage={stage:?} slice_cycles={} slice_us={} \
             mem_before={} free/{} used mem_after={} free/{} used",
            case.name,
            tick_count,
            slice_cycles,
            slice_us,
            before_free,
            before_used,
            after_free,
            after_used,
        );

        match step_result {
            ShaderCompileStepResult::Pending => {}
            ShaderCompileStepResult::Finished(shader) => {
                let total_us = cycle_counter::cycles_to_us(total_cycles);
                let max_slice_us = cycle_counter::cycles_to_us(max_slice_cycles);
                let resident_free = esp_alloc::HEAP.free();
                let resident_used = esp_alloc::HEAP.used();
                info!(
                    "[inc-shader-compile] case={} finished ticks={} total_cycles={} total_us={} \
                     max_slice_cycles={} max_slice_us={} heap_start={} free/{} used \
                     heap_peak_used={} heap_resident={} free/{} used",
                    case.name,
                    tick_count,
                    total_cycles,
                    total_us,
                    max_slice_cycles,
                    max_slice_us,
                    start_free,
                    start_used,
                    peak_used,
                    resident_free,
                    resident_used,
                );
                if max_slice_us > TARGET_TICK_US {
                    warn!(
                        "[inc-shader-compile] case={} exceeded target slice budget: {}us > {}us",
                        case.name, max_slice_us, TARGET_TICK_US,
                    );
                }
                drop(shader);
                let after_drop_used = esp_alloc::HEAP.used();
                info!(
                    "[inc-shader-compile] case={} after_drop={} free/{} used",
                    case.name,
                    esp_alloc::HEAP.free(),
                    after_drop_used,
                );
                return CaseSummary {
                    name: case.name,
                    tick_count,
                    total_us,
                    max_slice_us,
                    peak_used,
                    resident_used,
                    after_drop_used,
                };
            }
            ShaderCompileStepResult::Failed(err) => {
                panic!(
                    "incremental shader compile failed for case {} after {} ticks: {}",
                    case.name, tick_count, err
                );
            }
        }
    }
}
