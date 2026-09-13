//! The `shader-compile-stress` payload's device half, on the ESP32-S3.
//!
//! **The compile-parity claim is the point**: the same shader, compiled on
//! the device's own JIT, tick by tick, with the memory figures beside each
//! tick — so that silicon and `lp-emu:esp32s3:t1` can be compared field by
//! field rather than in aggregate. The per-tick line and the two summary
//! records are byte-for-byte the C6's and the classic's shapes, because
//! `lp_emu_validate::payload`'s `COMPILE_TICK` series and
//! `shader-compile-stress` field specs parse all three chips' transcripts
//! with one regex and one field table. That is the whole reason one payload
//! name spans three chips (M5 ruling R5).
//!
//! `fw-checks`'s `check-shader-compile` carries the record shapes the host
//! side parses; `fw-esp32c6/src/tests/incremental_shader_compile/` and
//! `fw-esp32v3/src/tests/shader_compile_incremental.rs` are other chips'
//! files and are **mirrored, never imported** (the `lint-emu-fence` rule's
//! firmware-side twin: each chip crate owns its chip code).
//!
//! ## ⚠️ Why this harness is the milestone's write→execute test
//!
//! The classic installs a **fixed SRAM0 code region** before its first
//! compile, because a shader executing from its heap faults with EXCCAUSE=2
//! on that part — the classic's heap has no I-bus view. **This chip is the
//! other case, and it is the one M6 D2 is about.** The S3's SRAM1 is
//! dual-mapped: a heap buffer written through the D-bus at `0x3FC8_xxxx` is
//! executable through its I-bus alias at `+0x6F_0000`
//! (`lpvm_native::exec_addr`'s S3 arm), so the JIT allocates code straight
//! out of `esp_alloc` and no region is installed at all. `codemem_esp32`
//! contributes zero symbols to this image.
//!
//! What a run of this harness therefore drives, and nothing else in M6 does,
//! is that rule on a **dynamic allocation**: the code is stored through one
//! view and every intra-module relocation is patched with the other view's
//! address, so a machine modelling the two as two memories emits branch
//! targets the fetch path cannot reach.
//!
//! ⚠️ **Stated no wider than it is.** This harness compiles and *drops*; it
//! never renders, so the shader it builds is never fetched. The fetch side of
//! the alias is exercised by this firmware's own `.rwtext` and vectors, which
//! live in the I-bus view on every boot of every payload. Executing JIT'd
//! code through the alias needs a loaded project, and no payload in M6 loads
//! one.
//!
//! ## What this harness has to build that the app gets for free, and what it
//! does not
//!
//! A `test_*` feature sets `fw_harness`, which cfg's the whole application
//! out of `main.rs`. Two of the three things the classic's harness therefore
//! has to do itself are **already done** here, and doing them again would be
//! a second, different setup:
//!
//! - **The heap is the app's.** `main.rs`'s harness entrypoint already calls
//!   `esp_alloc::heap_allocator!(size: HEAP_SIZE)` with the app's own
//!   240 KiB before dispatching, so the compile meets exactly the ceiling
//!   the product's compile meets. Installing a second region here would move
//!   the figure this transcript exists to report.
//! - **The clock is 240 MHz.** That entrypoint takes `CpuClock::max()`
//!   deliberately — esp-hal's S3 default is 80 MHz, and
//!   `board::esp32s3::constants::CPU_HZ` hardcodes 240 — so every cycle→µs
//!   figure below is against the rate the divisor assumes.
//! - **The JIT code region is NOT installed**, because there is none. See
//!   the alias section above.
//!
//! What is still this module's to do is arm the FPU. See [`run`].

use alloc::sync::Arc;

use log::info;
use lpvm_native::{BuiltinTable, NativeCompileOptions, NativeJitEngine};

mod runner;

/// Entry point. Arms the FPU, installs a logger, builds the engine, prints
/// the payload header, runs the corpus, prints the done marker, and idles.
pub fn run() -> ! {
    // ⚠️ **Arm the FPU before the COMPILER runs**, not merely before the
    // shader does — and before anything else here, which is why it precedes
    // even the logger.
    //
    // `board::esp32s3::fpu`'s docs frame this as "compiled shader code
    // contains bare FP instructions and arms nothing". True, and not the
    // whole rule: the *compiler* does f32 arithmetic too (constant folding
    // in the frontend, and `float-f32` is on by default on this crate), and
    // rustc emits real LX7 FP instructions for it. The classic measured the
    // failure mode — reach `tick=2`, then walk the stack down through an
    // `EXCCAUSE=32` loop nothing can handle — and this chip has the same
    // coprocessor gate. The app path never sees it because `init_board` arms
    // the core long before any compile; a harness build does not run
    // `init_board` at all. The resulting `CPENABLE` is reported rather than
    // assumed, one line below the header.
    let cpenable = crate::board::esp32s3::fpu::arm();

    // A logger, because `fw-checks`'s `emit_record_json` goes through `log`
    // and nothing else in a harness image installs one.
    esp_println::logger::init_logger(log::LevelFilter::Info);

    lps_builtins::ensure_builtins_referenced();
    let mut table = BuiltinTable::new();
    table.populate();
    let options = NativeCompileOptions {
        stage_trace: true,
        ..NativeCompileOptions::default()
    };
    let engine = lp_shader::LpsEngine::new(NativeJitEngine::new(Arc::new(table), options));

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
        "[inc-shader-compile] cpenable={cpenable:#010x} (armed before the first compile)"
    );
    info!(
        "[inc-shader-compile] === incremental shader compile experiment starting \
         ({} KiB heap, JIT out of the heap through SRAM1's I-bus alias — no reserved region) ===",
        crate::HEAP_SIZE / 1024,
    );
    runner::run_all(&engine);
    info!("[inc-shader-compile] === DONE ===");

    loop {
        core::hint::spin_loop();
    }
}
