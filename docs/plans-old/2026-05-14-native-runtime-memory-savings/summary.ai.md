# Native Runtime Memory Savings Summary

## What was built

- Made native `lpvm-native` runtime compile output debug-light by default.
- Stopped the default JIT path from unconditionally building and cloning structured debug sections.
- Kept emulator and debug-assembly paths explicitly debug-enabled.
- Trimmed retained `NativeJitModule` runtime state by replacing full retained IR dependence with compact per-entry summaries.
- Preserved direct-call behavior and render entry validation using precomputed entry metadata.
- Re-ran the ESP32 incremental compile stress harness and captured new memory/timing numbers.

## Measurements

Baseline before this plan's work, after the earlier intermediate-release fix:
- `heap_peak_used=109528`
- `max_slice_us=58095`

Final measurement after this plan:
- trace: `/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/traces/2026-05-14T08-42-36-inc-shader-compile-stress.txt`
- `heap_peak_used=81088`
- `max_slice_us=54641`
- `heap_resident=297148 free/22852 used`
- `after_drop=313644 free/6356 used`

Observed improvement from this plan:
- peak used heap reduced by `28,440` bytes versus the `109,528` baseline
- roughly `26%` lower peak heap on top of the previous intermediate-release win

## Decisions for future reference

#### Runtime JIT is debug-light by default

- **Decision:** The default native JIT path no longer builds or retains rich function/module debug payloads unless explicitly requested.
- **Why:** Firmware warm-up and background compilation care about heap much more than debug observability.
- **Rejected alternatives:** Keep unconditional debug data everywhere; remove debug support entirely.
- **Revisit when:** A runtime consumer appears that genuinely needs on-device structured debug info.

#### Emulator and host debug paths keep rich debug support

- **Decision:** `rt_emu` and `debug_asm` continue to opt into debug data explicitly.
- **Why:** Filetests, disassembly, and host-side diagnostics still benefit from the richer view.
- **Rejected alternatives:** Force all backends into one lowest-common-denominator debug model.

#### JIT runtime entries use compact summaries instead of retained IR

- **Decision:** `NativeJitModule` now keeps per-entry summaries for direct-call and render-entry validation instead of retaining the full IR for those tasks.
- **Why:** The IR was resident memory with no runtime value once the executable image and entry metadata existed.
- **Rejected alternatives:** Keep the IR for convenience; redesign all module/signature APIs immediately.
- **Revisit when:** We decide whether `LpsModuleSig` can also be compacted further without destabilizing runtime APIs.

#### No dedicated link-buffer rewrite yet

- **Decision:** Do not perform a separate link/JIT-buffer restructuring in this pass.
- **Why:** After trimming debug payload and retained IR, the hardware trace no longer pointed to `AssembleModule` or final JIT buffer creation as the dominant memory problem.
- **Rejected alternatives:** Rewrite the buffer/link flow speculatively.
- **Revisit when:** A future trace shows a new peak centered on link/finalization rather than frontend or per-function backend stages.
