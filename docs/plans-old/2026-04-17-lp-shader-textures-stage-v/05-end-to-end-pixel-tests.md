# Phase 5 — End-to-end pixel correctness tests (wasmtime path)

## Scope

Land four format-correctness tests on **`lpvm-wasm` / `WasmLpvmEngine`
(Wasmtime)** — the supported host-execution backend for `lp-shader` unit
tests and the direction for `lp-cli` / authoring tools. Each test compiles
a real GLSL shader through the full pipeline (`compile_px` → synth →
backend compile → `LpsPxShader::new` → `render_frame`) and verifies the
byte output of `LpsTextureBuf` matches a hand-computed expected encoding.

The four tests cover (Q10): per-format Q32 → unorm16 conversion, the
pixel-centre convention (Q1), correct row/column enumeration, correct
byte ordering inside a pixel, and that `render_frame` produces the
expected bytes end-to-end across the *entire* M2.0 pipeline.

The `lpvm-native` (RV32 emulator) and browser wasm paths are deliberately
*not* tested here — they're validated downstream once M2.0 is threaded
through `lpfx` (`fw-emu`-based integration tests for RV32) and any future
`fw-wasm` harness. Per-backend correctness for those paths is deferred to
those integration milestones; this phase's job is to lock down the synth
+ adapter + `call_render_texture` wiring on a **deterministic host engine
with 32-bit guest pointers** (same pointer model as RV32 / browser).

Closes Q10 in [`00-notes.md`](./00-notes.md).

## Prerequisites

- Phases 1–4 merged.

## Code organisation reminders

- Tests live in `lp-shader/lp-shader/src/tests.rs`, extending the
  existing test module.
- The `test_engine()` helper wires `LpsEngine<WasmLpvmEngine>` — reuse it
  directly. No new helper needed.
- Each test computes the expected unorm16 bytes via the same
  `clamped - (clamped >> 16)` formula the synth emits, so any
  drift between expected and actual is a real codegen bug, not a
  test-vs-impl arithmetic mismatch.
- These tests *subsume* end-to-end coverage relative to the old host-JIT
  plan, but they do **not** replace the Phase 2 `lpvm-cranelift`
  handwritten-LPIR smoke test. The Phase 2 smoke stays — it's the only
  check between Phases 2 and 5 that catches a **JIT** trait-extension
  regression early; Phase 5 is the full-pipeline check on Wasmtime.

## Implementation details

### Helper: expected unorm16 encoder

```rust
// lp-shader/lp-shader/src/tests.rs

/// Mirror the synth's exact arithmetic so test expectations and
/// runtime output share a single formula.
fn q32_to_unorm16_bytes(value_q32: i32) -> [u8; 2] {
    let clamped = value_q32.clamp(0, 65536);
    let unorm = (clamped - (clamped >> 16)) as u16;
    unorm.to_le_bytes()
}

/// Convenience: encode an f32 in [0.0, 1.0] (or out-of-range, clamped)
/// the same way GLSL float literals would round-trip through Q32.
fn unorm16_bytes_from_f32(v: f32) -> [u8; 2] {
    let q = (v * 65536.0).round() as i32;
    q32_to_unorm16_bytes(q)
}
```

### Test 1: R16Unorm constant

```rust
#[test]
fn render_frame_r16_constant_writes_expected_bytes() {
    let engine = test_engine();
    let glsl = r#"
        float render(vec2 pos) { return 0.5; }
    "#;
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::R16Unorm)
        .expect("compile_px R16");
    let mut tex = engine
        .alloc_texture(2, 2, TextureStorageFormat::R16Unorm)
        .expect("alloc_texture");

    let uniforms = LpsValueF32::Struct { name: None, fields: vec![] };
    shader.render_frame(&uniforms, &mut tex).expect("render_frame");

    let expected = unorm16_bytes_from_f32(0.5);
    let bytes = tex.data();
    assert_eq!(bytes.len(), 2 * 2 * 2, "2x2 R16 = 8 bytes");
    for (i, chunk) in bytes.chunks_exact(2).enumerate() {
        assert_eq!(chunk, &expected, "pixel {i}");
    }
}
```

### Test 2: Rgb16Unorm constant

```rust
#[test]
fn render_frame_rgb16_constant_writes_expected_bytes() {
    let engine = test_engine();
    let glsl = r#"
        vec3 render(vec2 pos) { return vec3(0.25, 0.5, 0.75); }
    "#;
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgb16Unorm)
        .expect("compile_px Rgb16");
    let mut tex = engine
        .alloc_texture(2, 2, TextureStorageFormat::Rgb16Unorm)
        .expect("alloc_texture");

    let uniforms = LpsValueF32::Struct { name: None, fields: vec![] };
    shader.render_frame(&uniforms, &mut tex).expect("render_frame");

    let r = unorm16_bytes_from_f32(0.25);
    let g = unorm16_bytes_from_f32(0.5);
    let b = unorm16_bytes_from_f32(0.75);
    let expected_pixel: [u8; 6] = [r[0], r[1], g[0], g[1], b[0], b[1]];
    let bytes = tex.data();
    assert_eq!(bytes.len(), 2 * 2 * 6, "2x2 Rgb16 = 24 bytes");
    for (i, chunk) in bytes.chunks_exact(6).enumerate() {
        assert_eq!(chunk, &expected_pixel, "pixel {i}");
    }
}
```

### Test 3: Rgba16Unorm constant

```rust
#[test]
fn render_frame_rgba16_constant_writes_expected_bytes() {
    let engine = test_engine();
    let glsl = r#"
        vec4 render(vec2 pos) { return vec4(0.0, 1.0, 0.5, 1.0); }
    "#;
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px Rgba16");
    let mut tex = engine
        .alloc_texture(2, 2, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");

    let uniforms = LpsValueF32::Struct { name: None, fields: vec![] };
    shader.render_frame(&uniforms, &mut tex).expect("render_frame");

    let r = unorm16_bytes_from_f32(0.0);
    let g = unorm16_bytes_from_f32(1.0);
    let b = unorm16_bytes_from_f32(0.5);
    let a = unorm16_bytes_from_f32(1.0);
    let expected_pixel: [u8; 8] = [r[0], r[1], g[0], g[1], b[0], b[1], a[0], a[1]];
    let bytes = tex.data();
    assert_eq!(bytes.len(), 2 * 2 * 8);
    for (i, chunk) in bytes.chunks_exact(8).enumerate() {
        assert_eq!(chunk, &expected_pixel, "pixel {i}");
    }
}
```

### Test 4: Rgba16Unorm gradient (verifies x/y enumeration + pixel centre)

The shader returns the integer-pixel position as Q32 fractions; for a
3×2 texture this means we can read back exactly what the synth wrote
into per-pixel `pos.x` / `pos.y` and assert the row/column ordering
plus the pixel-centre offset.

```rust
#[test]
fn render_frame_rgba16_gradient_verifies_pos_and_enumeration() {
    let engine = test_engine();

    // r = pos.x / 65536  -> recovers the integer pixel x as a Q-decimal
    // g = pos.y / 65536  -> ditto y
    // b = 0
    // a = 1
    //
    // GLSL input is 'float' (Q32 fixed point). pos.x for pixel column x
    // is `(x << 16) + 32768`, i.e. the pixel centre.  We divide by
    // 65536.0 to recover the integer column with a +0.5 offset.
    let glsl = r#"
        vec4 render(vec2 pos) {
            return vec4(pos.x * (1.0/65536.0),
                        pos.y * (1.0/65536.0),
                        0.0, 1.0);
        }
    "#;
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px");
    let (w, h) = (3u32, 2u32);
    let mut tex = engine
        .alloc_texture(w, h, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    let uniforms = LpsValueF32::Struct { name: None, fields: vec![] };
    shader.render_frame(&uniforms, &mut tex).expect("render_frame");

    let bytes = tex.data();
    assert_eq!(bytes.len(), (w * h * 8) as usize);
    for y in 0..h {
        for x in 0..w {
            let off = ((y * w + x) * 8) as usize;
            let pixel = &bytes[off..off + 8];

            // Expected pos.x = (x << 16) + 32768; Q32 -> f32 = x + 0.5
            let expected_r = unorm16_bytes_from_f32(x as f32 + 0.5);
            let expected_g = unorm16_bytes_from_f32(y as f32 + 0.5);
            let expected_b = unorm16_bytes_from_f32(0.0);
            let expected_a = unorm16_bytes_from_f32(1.0);
            let expected: [u8; 8] = [
                expected_r[0], expected_r[1],
                expected_g[0], expected_g[1],
                expected_b[0], expected_b[1],
                expected_a[0], expected_a[1],
            ];
            assert_eq!(pixel, &expected, "pixel ({x},{y})");
        }
    }
}
```

> Caveats to keep in mind during implementation:
> - The gradient test relies on `1.0 / 65536.0` round-tripping
>   exactly through GLSL → naga → LPIR Q32. If naga const-folds
>   the multiplication and the resulting Q32 value differs from
>   our hand-computed expectation by 1 LSB, switch to a tolerance
>   compare (`pixel_diff_le(pixel, expected, 1)`).
> - For columns where `x + 0.5 > 1.0`, the value clamps to 1.0
>   (encoding 0xFFFF). The test above already handles this via
>   `unorm16_bytes_from_f32`, which clamps. Pick texture sizes
>   that include both in-range (`x = 0` → 0.5) and clamped
>   (`x = 1, 2` → 1.5, 2.5 → clamped to 1.0) values to maximise
>   coverage.

### Cross-backend posture (deferred)

`lpvm-native`'s rt_emu path will be exercised end-to-end when M2.0
is threaded through `lpfx` and `lp-engine`, where existing
`fw-emu`-based integration tests already run real shaders and
inspect output. Per the Q10 conclusion, that's where cross-backend
coverage lands — not duplicated here. Same posture for browser wasm
once a `fw-wasm` harness exists.

M2.0 ships with **runtime correctness for the full pixel pipeline
validated on the Wasmtime path**; `lpvm-cranelift` is still guarded by
the Phase 2 handwritten smoke (single-instance JIT trait check). The
other backends are validated by `cargo build` for compile-time
correctness (Phase 2's per-backend impls all build clean, and they share
the same trait shape and same synth output). The Phase 2 smoke catches
JIT trait regressions early; the Phase 5 tests catch synth + adapter
regressions on Wasmtime.

## Validate

```bash
cargo test -p lp-shader -- render_frame_
```

No feature flags required — the default `lp-shader` test configuration
uses `WasmLpvmEngine`.

All four format tests pass; pre-existing `render_frame_*` tests
(`render_frame_no_uniforms`, `render_frame_sets_uniforms`) continue
to pass.

## Failure modes to expect during implementation

- **Phase 3 emits the wrong per-channel byte offset.** Symptom:
  test 3 / test 4 fail with channels swapped or shifted; tests 1
  and 2 may still pass. Fix: audit the per-channel `Store16`
  loop in synth.
- **Pixel-centre offset wrong.** Symptom: gradient test fails by
  exactly 0.5 in one axis. Fix: synth emits `(x << 16)` instead of
  `(x << 16) + 32768` (or vice versa).
- **Globals reset emitted incorrectly.** Symptom: gradient test
  fails because `render` reads a stale global between pixels. Fix:
  audit the `module_globals_mutated` gating in synth (Phase 3).
- **Q32 clamp polarity inverted.** Symptom: out-of-range values
  produce 0 instead of 0xFFFF. Fix: audit the `Select` polarity in
  the `emit_q32_to_unorm16` clamp.
- **Pointer / vmctx mismatch under Wasmtime.** Symptom: render_frame
  traps, writes nothing, or corrupts memory. Fix: confirm the wasm
  lowering passes the guest linear-memory offset from `LpvmBuffer`
  (`guest_base()`), and that texture bytes live in the instance's
  linear memory as expected. For JIT-only issues, the Phase 2
  `lpvm-cranelift` smoke remains the narrow reproducer.
