//! The `shader-compile-stress` payload's device half, on the classic ESP32.
//!
//! **The compile-parity claim is the point**: the same shader, compiled on
//! the device's own JIT, tick by tick, with the memory figures beside each
//! tick — so that silicon and `lp-emu:esp32v3:t1` can be compared field by
//! field rather than in aggregate. The per-tick line and the two summary
//! records are byte-for-byte the C6's shapes, because
//! `lp_emu_validate::payload`'s `COMPILE_TICK` series and
//! `shader-compile-stress` field specs parse both chips' transcripts with one
//! regex and one field table. That is the whole reason one payload name
//! spans two chips (M5 ruling R5).
//!
//! `fw-checks`'s `check-shader-compile` carries the record shapes the host
//! side parses; the C6's `src/tests/incremental_shader_compile/` is another
//! chip's file and is **mirrored, never imported**.
//!
//! ## What this harness has to build that the app gets for free
//!
//! A `test_*` feature sets `fw_harness`, which cfg's the whole application
//! out of `main.rs` — including `esp_alloc::heap_allocator!` and the JIT code
//! region install. A compile harness needs both, so it does them itself, and
//! the two are not interchangeable with the app's:
//!
//! - **The heap is one region, [`HARNESS_HEAP_SIZE`] + the SRAM1 tail.** The
//!   app has four regions (the `dram_seg` arena, the SRAM1 tail, and the two
//!   ROM boot stacks). This has the first two: the arena, at the app's own
//!   size so the compile meets the same ceiling it meets in the product, and
//!   the tail, because the JIT lives in SRAM0 and the tail above it is free
//!   either way. The two ROM stacks are deliberately left alone — reclaiming
//!   them is the *app's* measured decision and copying it here would put a
//!   number in this transcript that the app's own boot log is the evidence
//!   for.
//! - **The JIT code region is installed before the first compile.** Without
//!   it a compiled shader executes from heap, which has no I-bus view on this
//!   chip: EXCCAUSE=2 on the first render tick, observed on the DOM-Z-102
//!   (`Cargo.toml`, the `lpvm-native` dependency note). `xt-placed-code` is
//!   on through that same dependency line, so the engine's link step takes
//!   the fixed-region path and this install is what gives it the region.

extern crate alloc;

use alloc::sync::Arc;

use log::{info, warn};
use lp_shader::{
    CompilePxDesc, LpsEngine, ShaderCompileBudget, ShaderCompileStageDetail,
    ShaderCompileStepResult, ShaderFrontend, TextureStorageFormat,
};
use lpvm_native::{BuiltinTable, NativeCompileOptions, NativeJitEngine};

/// The `dram_seg` arena this harness installs.
///
/// The app's `HEAP_SIZE` exactly (`main.rs`), and for the reason in the
/// module docs: a compile-parity figure is only worth recording if the
/// compile met the ceiling the product's compile meets.
const HARNESS_HEAP_SIZE: usize = 110 * 1024;

/// The per-tick step budget. The C6's, unchanged — a slice is a slice on
/// either chip, and changing it here would make the two `ticks` counts
/// incomparable, which is the one structural field this payload has.
const COMPILE_BUDGET: ShaderCompileBudget = ShaderCompileBudget {
    frontend_steps: 1,
    backend_steps: 1,
};

/// The slice budget a `warn!` is printed above. Advisory: nothing gates on
/// it, on either chip.
const TARGET_TICK_US: u64 = 5_000;

/// CCOUNT ticks per microsecond at the clock the harness entry point sets
/// (`CpuClock::max()` = 240 MHz). The conversion lives here rather than in a
/// shared crate because the rate is a chip fact.
const CYCLES_PER_US: u64 = 240;

/// One compile case: a shader and what to compile it as.
struct ShaderCompileCase {
    name: &'static str,
    glsl: &'static str,
}

/// The corpus. One case, the same one the C6 records — `projects/test/basic`,
/// which is in the tree at a stable path and is what both chips' committed
/// transcripts are of.
static SHADER_COMPILE_CASES: &[ShaderCompileCase] = &[ShaderCompileCase {
    name: "examples-basic",
    glsl: include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../projects/test/basic/shader.glsl"
    )),
}];

impl ShaderCompileCase {
    fn desc(&self) -> CompilePxDesc<'static> {
        // `LpsGlsl`: the incremental (stepped) frontend path only exists for
        // the device pipeline's native frontend; naga completes in one step,
        // and a one-tick transcript measures nothing.
        CompilePxDesc::new(
            self.glsl,
            TextureStorageFormat::Rgba16Unorm,
            lpir::CompilerConfig::default(),
            ShaderFrontend::LpsGlsl,
        )
    }
}

struct CaseSummary {
    name: &'static str,
    tick_count: u32,
    total_us: u64,
    max_slice_us: u64,
    max_slice_stage: ShaderCompileStageDetail,
    peak_used: usize,
    resident_used: usize,
    after_drop_used: usize,
}

/// Entry point. Brings up the heap and the code region, builds the engine,
/// runs the corpus, prints the done marker, and idles.
pub fn run() -> ! {
    // The `dram_seg` arena. `heap_allocator!` both declares the storage and
    // registers it with `esp_alloc::HEAP`, which `main.rs`'s `RetryingHeap`
    // (the one `#[global_allocator]` in every build of this crate) wraps.
    esp_alloc::heap_allocator!(size: HARNESS_HEAP_SIZE);
    add_sram1_heap_tail();

    // ⚠️ **Arm the FPU before the compiler runs**, not before the shader does
    // — and before anything else here, which is why it precedes even the
    // logger.
    //
    // `board::esp32v3::fpu`'s docs frame this as "compiled shader code
    // contains bare FP instructions and arms nothing" — true, and not the
    // whole rule. The *compiler* does f32 arithmetic too (constant folding in
    // the frontend, and `float-f32` is on by default on this crate), and
    // rustc emits real LX6 FP instructions for it. Measured here: without
    // this call the harness reaches `tick=2` and then walks the stack down
    // through an exception loop — `EXCCAUSE=32` with nothing able to handle
    // it, which on the machine surfaces as a strict-bus stop inside
    // `__naked_double_exception` and on silicon would be a reset with no
    // message. The app never sees this because `init_board` arms the PRO core
    // long before any compile. The resulting `CPENABLE` is reported rather
    // than assumed, one line below the header.
    let cpenable = crate::board::esp32v3::fpu::arm();

    // A logger, because `fw-checks`'s `emit_record_json` goes through `log`
    // and nothing else in a harness image installs one.
    esp_println::logger::init_logger(log::LevelFilter::Info);

    // The JIT's code region, before any compile. See the module docs: without
    // it the first shader executes from heap, which this chip cannot fetch
    // from.
    let jit_region = lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT;
    lpvm_native::codemem_esp32::global::install(jit_region);

    lps_builtins::ensure_builtins_referenced();
    let mut table = BuiltinTable::new();
    table.populate();
    let options = NativeCompileOptions {
        stage_trace: true,
        ..NativeCompileOptions::default()
    };
    let engine = LpsEngine::new(NativeJitEngine::new(Arc::new(table), options));

    // The transcript header, first, before any record: it is what lets a
    // committed capture be checked against the sidecar that claims to
    // describe it (`lp-fw/fw-checks/src/header.rs`). Printed through
    // `esp_println` rather than logged: this harness happens to install a
    // logger above, but the header must reach the port on every payload
    // harness whether or not that stays true, so all of them go through the
    // same sink-agnostic entry point.
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "shader-compile-stress",
            chip: super::CHIP,
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );
    info!(
        "[inc-shader-compile] cpenable={cpenable:#010x} (PRO core, armed before the first compile)"
    );
    info!(
        "[inc-shader-compile] === incremental shader compile experiment starting ({} KiB arena + {} B SRAM1 tail, JIT region {:#010x}..{:#010x}) ===",
        HARNESS_HEAP_SIZE / 1024,
        jit_region.reclaimable_heap_span().1,
        jit_region.ibus_base(),
        jit_region.ibus_end(),
    );
    run_all(&engine);
    info!("[inc-shader-compile] === DONE ===");

    loop {
        core::hint::spin_loop();
    }
}

/// Give the allocator SRAM1's tail, the same span the app reclaims.
///
/// # Safety
/// The span is `'static` (a fixed hardware address) and exclusively the
/// allocator's: the JIT code region is SRAM1's only other claimant, and
/// `codemem_esp32`'s const-asserts prove the two abut without overlap — a
/// fact the compiler keeps rather than a rule this comment remembers.
fn add_sram1_heap_tail() {
    let (base, len) = lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT.reclaimable_heap_span();
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            base as *mut u8,
            len as usize,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
}

fn run_all(engine: &LpsEngine<NativeJitEngine>) {
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
            "[inc-shader-compile] summary case={} build={} ticks={} max_slice={} max_slice_stage={:?} peak={} resident={} after_drop={}",
            summary.name,
            fmt_ms_1(summary.total_us),
            summary.tick_count,
            fmt_ms_1(summary.max_slice_us),
            summary.max_slice_stage,
            fmt_kib_1(summary.peak_used),
            fmt_kib_1(summary.resident_used),
            fmt_kib_1(summary.after_drop_used),
        );
        fw_checks::emit_record_json(format_args!(
            "{{\"kind\":\"case-summary\",\"check\":\"shader-compile-stress\",\"case\":\"{}\",\"build_us\":{},\"ticks\":{},\"max_slice_us\":{},\"max_slice_stage\":\"{:?}\",\"peak_used\":{},\"resident_used\":{},\"after_drop_used\":{}}}",
            summary.name,
            summary.total_us,
            summary.tick_count,
            summary.max_slice_us,
            summary.max_slice_stage,
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

fn run_case(engine: &LpsEngine<NativeJitEngine>, case: &ShaderCompileCase) -> CaseSummary {
    info!("[inc-shader-compile] --- case={} ---", case.name);

    let mut job = engine.start_compile_px_job(case.desc());
    let start_free = esp_alloc::HEAP.free();
    let start_used = esp_alloc::HEAP.used();
    let mut peak_used = start_used;
    let mut max_slice_cycles = 0u64;
    let mut max_slice_stage = ShaderCompileStageDetail::Done;
    let mut total_cycles = 0u64;
    let mut tick_count = 0u32;

    loop {
        tick_count = tick_count.saturating_add(1);
        let stage = job.stage_detail();
        let before_free = esp_alloc::HEAP.free();
        let before_used = esp_alloc::HEAP.used();
        let cycle_start = read_cycles();
        let step_result = job.step(COMPILE_BUDGET);
        let slice_cycles = u64::from(read_cycles().wrapping_sub(cycle_start));
        let slice_us = cycles_to_us(slice_cycles);
        let after_free = esp_alloc::HEAP.free();
        let after_used = esp_alloc::HEAP.used();

        peak_used = peak_used.max(after_used);
        if slice_cycles > max_slice_cycles {
            max_slice_cycles = slice_cycles;
            max_slice_stage = stage;
        }
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
                let total_us = cycles_to_us(total_cycles);
                let max_slice_us = cycles_to_us(max_slice_cycles);
                let resident_free = esp_alloc::HEAP.free();
                let resident_used = esp_alloc::HEAP.used();
                info!(
                    "[inc-shader-compile] case={} finished ticks={} total_cycles={} total_us={} \
                     max_slice_cycles={} max_slice_us={} max_slice_stage={:?} heap_start={} free/{} used \
                     heap_peak_used={} heap_resident={} free/{} used",
                    case.name,
                    tick_count,
                    total_cycles,
                    total_us,
                    max_slice_cycles,
                    max_slice_us,
                    max_slice_stage,
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
                    max_slice_stage,
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

/// CCOUNT, the LX6's own free-running cycle counter.
fn read_cycles() -> u32 {
    esp_hal::xtensa_lx::timer::get_cycle_count()
}

const fn cycles_to_us(cycles: u64) -> u64 {
    cycles / CYCLES_PER_US
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
