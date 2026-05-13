# JIT Math Performance Notes

## Scope

Plan a pragmatic performance spike for the LightPlayer JIT hot path, focused on Q16.16 shader math on ESP32-C6:

- Measure real on-device cycle costs for division, multiplication, trig approximations, and LUT access patterns.
- Use those numbers to choose a default fast math path for normal rendering.
- Keep the on-device GLSL compiler intact. No `std` gating, no embedded compile stubs, no host precompile requirement.
- Preserve reference math helpers only as tests/debug-probe building blocks, not as the normal product mode.

Out of scope for this plan:

- Re-optimizing `__lp_lpfn_psrdnoise2_q32`; the latest profile already says it has had a hard pass.
- A full LPIR middle-end project such as LICM, CSE, SSA, or global value numbering.
- A user-facing debug math probe. Capture the shape, but do not build it here.
- Host-only performance conclusions. Host tests may check accuracy, but ESP32 PMU numbers decide.

## User Notes To Preserve

- Latest steady-render profile target:
  - `__lp_lpfn_psrdnoise2_q32`: 1,045,940 self cycles, 21.4%.
  - `__lp_lpir_fdiv_recip_q32`: 694,080 self cycles, 14.2%.
  - `__lps_sin_q32`: 554,720 self cycles, 11.3%.
  - `[jit] render`: 433,800 self cycles, 8.9%.
  - `__lps_atan2_q32`: 115,367 self cycles, 2.4%.
  - `OutputNode::tick` / `FixtureNode::render_control` dominate inclusive cycles because steady render is the target workload.
- The product direction is to abandon the fast/accurate split for normal rendering. LightPlayer should be fast by default.
- Correctness/overflow diagnostics should come later as a math debug probe that injects detection code.
- Desired R&D topics:
  - Precomputed reciprocals.
  - LUT-based trig and whether table/cache cost beats arithmetic.
  - Inline multiplication.
  - Auto-generated LUTs if they win.
- Real on-device numbers before compiler work.
- Hardware is available at `/dev/cu.usbmodem1101`. Flash/run commands must pass
  that port explicitly or set the matching `espflash` environment variable.

## Current Codebase State

- `lp-shader/lps-q32/src/q32_options.rs`
  - `Q32Options::default()` is currently conservative:
    - `add_sub = Saturating`
    - `mul = Saturating`
    - `div = Saturating`
  - Examples such as `examples/basic/shader.toml`, `examples/rocaille/shader.toml`, and `examples/perf/fastmath/shader.toml` opt into:
    - `add_sub = "wrapping"`
    - `mul = "wrapping"`
    - `div = "reciprocal"`
- `lp-core/lpc-model/src/nodes/shader/glsl_opts.rs` and `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
  - The domain model still exposes GLSL math modes as authored shader options.
  - `ShaderNode` maps those model options into `lps_q32::q32_options::Q32Options`.
- `lp-shader/lpvm-native/src/lower.rs`
  - Q32 `Fadd` / `Fsub` wrap modes already inline to one RV32 ALU op.
  - Q32 `Fmul` wrapping already inlines as a 5-VInst `mul` / `mulh` / shift / `or` sequence.
  - Q32 `Fdiv` reciprocal still lowers to a `__lp_lpir_fdiv_recip_q32` symbol call.
  - `AluOp` has signed `MulH`; it does not have unsigned `MulHu`, which is needed for a bit-equivalent inline reciprocal-divide wide product.
- `lp-shader/lps-builtins/src/builtins/lpir/fdiv_recip_q32.rs`
  - The reciprocal helper uses:
    - div-by-zero saturation guard.
    - unsigned absolute values.
    - one `u32` divide to compute reciprocal.
    - one 64-bit unsigned multiply and shift.
- `lp-shader/lps-builtins/src/builtins/glsl/sin_q32.rs`
  - `__lps_sin_q32` uses modulo range reduction plus a Taylor-style approximation.
  - It calls the saturating `__lp_lpir_fmul_q32` helper and uses several integer divisions by constants.
  - Its tests currently allow about 3% relative tolerance.
- `lp-shader/lps-builtins/src/builtins/glsl/sincos_q32.rs`
  - `sincos` shares range folding but still evaluates Taylor twice.
- LUT precedent already exists:
  - `lp-shader/lps-builtins/src/builtins/lpfn/generative/gnoise/smooth_lut_q32.rs` has const-generated 256-entry smoothstep LUTs.
  - `lp-shader/lps-builtins/src/builtins/lpfn/generative/psrdnoise/*_lut_q32_data.rs` has generated LUT data with ignored regeneration tests.
- `lp-fw/fw-esp32/src/tests/test_msafluid.rs`
  - Provides a good pattern for an ESP32 feature-gated performance harness:
    - feature in `fw-esp32/Cargo.toml`
    - `tests` module selected by `main.rs`
    - ESP32 PMU setup using `mpcer` / `mpcmr` / `mpccr`
    - warmup, medians, matrices, and serial logs.
- `justfile`
  - Existing hardware recipes include `test-msafluid`, `test-rmt`, `test-dither`, etc.
  - A `fwtest-jit-math-perf-esp32c6` recipe should follow this style.
  - Hardware runs should use `/dev/cu.usbmodem1101` explicitly.
- `lp-fw/fw-esp32/.cargo/config.toml`
  - The RV32 cargo runner is `espflash flash --chip esp32c6 --monitor --after hard-reset`.
  - It does not include a port, so hardware cargo-run commands should set
    `ESPFLASH_PORT=/dev/cu.usbmodem1101` unless the recipe passes an explicit
    espflash port flag.
- `docs/future/2026-04-20-middle-end-optimization.md`
  - Previously identified inline reciprocal `Fdiv` and const-divisor specialization.
  - Important note: for arbitrary runtime divisors, the current one-`divu` plus wide multiply algorithm is already close to the obvious fast path. Avoiding the divide is higher leverage than inventing a new arbitrary-divisor algorithm.
- `docs/future/2026-05-03-rocaille-fastmath-profile.md`
  - Fastmath reduced a rocaille profile from 30.49M to 20.69M attributed cycles.
  - Remaining fastmath top self cost was dominated by `__lps_sin_q32` and `__lp_lpir_fdiv_recip_q32`.
  - The note points toward fast trig, constant/hoisted reciprocal division, and later LICM.

## Suggested Answers To Open Questions

### Should this plan remove all math mode plumbing immediately?

Suggested answer: not in the first measurement phases. First collect numbers. Once a candidate wins, change defaults and normal compiler behavior to fast math, while keeping reference helpers available for tests and future probes. Remove or de-emphasize user-facing `glsl_opts` after the compiler path is validated, not before.

### Should the first measurement happen in `fw-esp32` or the emulator?

Suggested answer: `fw-esp32` first. The user explicitly wants real on-device numbers and cache/LUT cost. Emulator steady-render profiles remain the end-to-end regression gate, but hardware PMU measurements decide primitive math choices.

### Should LUTs live in flash or RAM?

Suggested answer: measure both. Start with `static`/rodata tables and an optional RAM-copied table in the firmware harness. The plan should report sequential, strided, and pseudo-random access costs at candidate sizes before choosing.

### Should trig quality be fixed before measuring?

Suggested answer: define an accuracy budget per candidate and print both cycle and quality summaries. Current `sin` tests already tolerate about 3%; for visual shader fast math, the first pass should compare max absolute error, RMS error, and key angles over a representative Q32 corpus.

### Should multiplication still be part of the spike if it is already inlined?

Suggested answer: yes, but as verification. The plan should confirm wrapping `Fmul` inline cost versus helper cost and make sure the default path really uses inline multiply everywhere steady-render expects. Do not spend much time here unless the hardware data contradicts expectations.
