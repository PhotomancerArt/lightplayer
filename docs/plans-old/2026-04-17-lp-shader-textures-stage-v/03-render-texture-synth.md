# Phase 3 — `__render_texture` synthesis routine

## Scope

Backend-agnostic synthesis pass that takes a lowered `(LpirModule,
LpsModuleSig, render_fn_index, TextureStorageFormat)` and appends a
new `__render_texture_<format_suffix>` function to both. The function
contains the full nested y/x pixel loop, calls the user `render`,
converts Q32 → unorm16, and writes channels into the texture buffer
via `Store16`.

This phase produces no end-user effect on its own; Phase 4 wires it
into `LpsEngine::compile_px`.

Closes Q1, Q2, Q3, Q4, Q6, Q7 in [`00-notes.md`](./00-notes.md).

## Code organisation reminders

- Lives in `lp-shader/lp-shader/src/synth/` (Q6: not in `lps-frontend`,
  not in `lpir`). Pure function: takes `&mut` to `LpirModule` and
  `LpsModuleSig`, mutates them in place. No LPVM dependency.
- Specialise per format (Q4): one specialised function per
  `compile_px(format)` call. Format constants — channel count, bytes
  per pixel, per-channel store offsets — are baked into the IR as
  constants, not loaded at runtime.
- Maintains the 1:1 zip between `LpirModule.functions` and
  `LpsModuleSig.functions` (backends rely on this in
  [`lpvm-native/rt_jit/module.rs:63`](../../../lp-shader/lpvm-native/src/rt_jit/module.rs)
  and [`lpvm-wasm/compile.rs:60-81`](../../../lp-shader/lpvm-wasm/src/compile.rs)).
- Marks the synthesised entry with `LpsFnKind::Synthetic` (per
  Phase 1).
- Globals reset (Q2 + Q3): emit `Memcpy(globals, snapshot, size)`
  per pixel **only when** the module both has globals *and* mutates
  them in `render` (or its transitive callees).

## Prerequisites

- Phase 1 (`LpsFnKind`) merged.
- Phase 2's `validate_render_texture_sig` helper merged (so Phase 4
  can use it; not directly required by this phase, but the synth
  output must satisfy it — included as a self-test).

## LpsType representation of the pointer parameter

The synthesised function's first parameter is `IrType::Pointer` at
the LPIR level — already correct and backend-polymorphic
([`lpir/src/types.rs:16-20`](../../../lp-shader/lpir/src/types.rs)).

For the matching `LpsFnSig.parameters[0].ty` slot, **use `LpsType::UInt`**
with parameter name `"__tex_ptr"`. Rationale:

- Avoids adding a `LpsType::Pointer` variant, which would touch
  ~20 exhaustive match sites across the workspace
  (`scalar_count_of_type`, codegen, marshalling helpers, etc).
- The ABI machinery (`scalar_count_of_type` →
  `classify_params`) only needs to know "1 scalar word", which
  `UInt` gives it.
- Phase 2's `validate_render_texture_sig` should validate against
  the **`IrFunction`** parameter types (`IrType::Pointer` for slot 0)
  rather than the `LpsFnSig` types — so the lie in `LpsFnSig` is
  harmless and self-contained to consumers that filter on
  `kind == Synthetic`.

> If a future need arises (e.g. exposing pointer-typed function
> signatures to user-visible APIs), promote this to a real
> `LpsType::Pointer` variant in a follow-up cleanup. Tracked
> implicitly by this design note.

> Phase 2 docs reference `LpsType::Pointer` in their validator
> sketch — update Phase 2's helper to take `&IrFunction` instead
> (or take both `&LpsFnSig` and `&IrFunction`) so the check uses
> `IrType::Pointer`. This is the only correction needed to that
> phase's design.

## Implementation details

### File layout

```
lp-shader/lp-shader/src/synth/
├── mod.rs                # pub use render_texture::*;
└── render_texture.rs     # the synthesis routine
```

Add `pub mod synth;` to `lp-shader/lp-shader/src/lib.rs`.

### Public API

```rust
// lp-shader/lp-shader/src/synth/render_texture.rs

use lpir::{LpirModule, IrFunction, IrType, /* etc */};
use lps_shared::{LpsModuleSig, LpsFnSig, LpsFnKind, LpsType, FnParam, ParamQualifier, TextureStorageFormat};

#[derive(Debug)]
pub enum SynthError {
    /// `render_fn_index` was out of bounds for the module's function list.
    InvalidRenderFnIndex,
    /// The render function's IR or signature couldn't be located.
    RenderFunctionMissing,
}

/// Synthesise a `__render_texture_<format_suffix>` function and append
/// it to both `module.functions` and `meta.functions`.
///
/// Returns the name of the appended function (e.g. `"__render_texture_rgba16"`).
pub fn synthesise_render_texture(
    module: &mut LpirModule,
    meta: &mut LpsModuleSig,
    render_fn_index: usize,
    format: TextureStorageFormat,
) -> Result<String, SynthError>;

/// Format suffix used in synthesised function names.
pub fn render_texture_fn_name(format: TextureStorageFormat) -> &'static str {
    match format {
        TextureStorageFormat::R16Unorm    => "__render_texture_r16",
        TextureStorageFormat::Rgb16Unorm  => "__render_texture_rgb16",
        TextureStorageFormat::Rgba16Unorm => "__render_texture_rgba16",
    }
}
```

### Body construction (using `FunctionBuilder`)

`FunctionBuilder` API ([`lpir/src/builder.rs`](../../../lp-shader/lpir/src/builder.rs)):
- `new(name, &[return_types])` — `vmctx` already in vreg slot 0.
- `add_param(IrType)` — adds a parameter vreg, returns `VReg`.
- `alloc_vreg(IrType)` — allocates an internal vreg.
- `push(LpirOp::*)` — emits an op.
- `push_loop()` / `push_continuing()` / `end_loop()` — loop frame.
- `push_if(cond)` / `end_if()` — conditional frame.
- `push_call(callee, &[arg_vregs], &[result_vregs])` — call op.
- `push_return(&[])` — terminate the function.

> **Performance note (Shape B — nested loop, incremental updates).**
> The inner loop emits **zero multiplications**. `pos_x`, `pos_y`,
> and `px_off` advance by constant additions each iteration; only
> the user `render` call and the per-channel `Store16`s do real
> work. This matters because the production target is `lpvm-native`
> on `rv32fa` (no M extension) — every multiply is a software
> libcall. The naive shape (`y * width * BPP + x * BPP`,
> `(x << 16) + 32768`) cost two multiplies per pixel; this shape
> costs zero. See [`00-design.md`](./00-design.md) for the
> rationale and the rejected single-flat-loop alternative.

```rust
fn synthesise_render_texture(
    module: &mut LpirModule,
    meta: &mut LpsModuleSig,
    render_fn_index: usize,
    format: TextureStorageFormat,
) -> Result<String, SynthError> {
    let render_sig = meta.functions.get(render_fn_index)
        .ok_or(SynthError::InvalidRenderFnIndex)?;
    let render_fn = module.functions.values()
        .find(|f| f.name == render_sig.name)
        .ok_or(SynthError::RenderFunctionMissing)?;
    let render_callee = CalleeRef::Local(render_fn.id);

    let channels       = format.channel_count();          // 1, 3, or 4
    let bytes_per_px   = format.bytes_per_pixel() as i32; // 2, 6, or 8
    let has_globals    = meta.globals_size() > 0;
    let needs_reset    = has_globals && module_globals_mutated(module);

    // (Optional optimisation) needs_reset can start as `has_globals`
    // (always-reset) for M2.0; the per-call mutation analysis is the
    // optimisation called out in Q2/Q3 and can be added incrementally.

    const Q_HALF: i32 = 32768;   // pixel-centre in Q32 (0x8000)
    const Q_ONE:  i32 = 65536;   // 1.0 in Q32       (1 << 16)

    let name = render_texture_fn_name(format).to_string();
    let mut fb = FunctionBuilder::new(&name, &[]);
    let tex_ptr = fb.add_param(IrType::Pointer);
    let width   = fb.add_param(IrType::I32);
    let height  = fb.add_param(IrType::I32);

    // ---- Hoisted state (function entry, executed once) -----------------
    // pos_y starts at the centre of row 0; advances by Q_ONE per row.
    let pos_y  = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: pos_y, value: Q_HALF });
    // px_off is a flat byte offset into the texture buffer; monotonically
    // increases by BPP per pixel across the whole frame — never reset.
    let px_off = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: px_off, value: 0 });
    // y row counter.
    let y = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: y, value: 0 });

    // ---- Outer loop: rows ---------------------------------------------
    fb.push_loop();
    {
        // if y >= height: break
        let cmp_y = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IcmpGeS { dst: cmp_y, lhs: y, rhs: height });
        fb.push_if(cmp_y);
        fb.push(LpirOp::Break);
        fb.end_if();

        // pos_x reset to the centre of column 0 at the start of each row.
        let pos_x = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 { dst: pos_x, value: Q_HALF });
        // x column counter.
        let x = fb.alloc_vreg(IrType::I32);
        fb.push(LpirOp::IconstI32 { dst: x, value: 0 });

        // ---- Inner loop: columns --------------------------------------
        fb.push_loop();
        {
            // if x >= width: break
            let cmp_x = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IcmpGeS { dst: cmp_x, lhs: x, rhs: width });
            fb.push_if(cmp_x);
            fb.push(LpirOp::Break);
            fb.end_if();

            // (only when needs_reset) — restore globals from snapshot.
            if needs_reset {
                emit_globals_reset(&mut fb, meta);
            }

            // color = render(pos_x, pos_y)
            // render returns N scalars (channels = {R16:1, Rgb16:3, Rgba16:4}).
            let mut color: Vec<VReg> = Vec::with_capacity(channels);
            for _ in 0..channels { color.push(fb.alloc_vreg(IrType::I32)); }
            fb.push_call(render_callee, &[pos_x, pos_y], &color);

            // Per channel: store16(tex_ptr + px_off, ch * 2, q32_to_unorm16(color[ch]))
            // px_off is the running byte offset; ch_offset is a baked
            // per-channel constant inside Store16's `offset` field.
            let base = tex_ptr_as_base(&mut fb, tex_ptr, px_off);
            for ch in 0..channels {
                let unorm = emit_q32_to_unorm16(&mut fb, color[ch]);
                let ch_offset = (ch as u32) * 2;
                fb.push(LpirOp::Store16 {
                    base,
                    offset: ch_offset,
                    value: unorm,
                });
            }

            // ---- Incremental updates (no multiplies) ------------------
            fb.push(LpirOp::IaddImm { dst: px_off, src: px_off, imm: bytes_per_px });
            fb.push(LpirOp::IaddImm { dst: pos_x,  src: pos_x,  imm: Q_ONE });
            fb.push(LpirOp::IaddImm { dst: x,      src: x,      imm: 1 });
        }
        fb.end_loop();

        // ---- Per-row updates (no multiplies) --------------------------
        fb.push(LpirOp::IaddImm { dst: pos_y, src: pos_y, imm: Q_ONE });
        fb.push(LpirOp::IaddImm { dst: y,     src: y,     imm: 1 });
    }
    fb.end_loop();
    fb.push_return(&[]);

    let ir_fn = fb.finish();
    module.add_function(ir_fn);  // (or whatever ModuleBuilder-equivalent the in-place mutation API offers)

    meta.functions.push(LpsFnSig {
        name: name.clone(),
        return_type: LpsType::Void,
        parameters: vec![
            FnParam {
                name: String::from("__tex_ptr"),
                ty: LpsType::UInt,         // see "LpsType representation" above
                qualifier: ParamQualifier::In,
            },
            FnParam {
                name: String::from("__width"),
                ty: LpsType::Int,
                qualifier: ParamQualifier::In,
            },
            FnParam {
                name: String::from("__height"),
                ty: LpsType::Int,
                qualifier: ParamQualifier::In,
            },
        ],
        kind: LpsFnKind::Synthetic,
    });

    Ok(name)
}
```

> The pseudocode above uses op spellings (`IcmpGeS`, `ImulImm`,
> `IaddImm`, `Ishl`, `Imov`, `Iconst`, `Store16`) that all exist
> based on the M1.1 narrow-mem ops landing in stage-iv. Confirm
> exact variant names against [`lpir/src/lpir_op.rs`](../../../lp-shader/lpir/src/lpir_op.rs)
> when implementing. If `Imov` doesn't exist, simply re-emit the
> `IconstI32` for `y = 0` later (or use slot-based storage).

> `tex_ptr_as_base` is shorthand for "compute the i32 base pointer
> for the Store16, namely `tex_ptr + px_off`". Implement as a
> small helper that emits an `Iadd` of `tex_ptr` (as i32 word) and
> `px_off`. The Store16 op's `base` field is a `VReg` whose type
> at LPIR level is the pointer slot. Confirm that
> `Store16 { base, offset, value }` accepts a `Pointer`-typed
> `base` vreg (it does — same as the existing `Store`/`Load` ops
> per Phase 1's design in stage-iv).

### `emit_q32_to_unorm16` helper

Inline conversion `(v in [0, 65536]) → u16`:

```text
clamped = max(0, min(value, 65536))
unorm   = clamped - (clamped >> 16)
```

The `clamped - (clamped >> 16)` form is algebraically equivalent to
`(clamped * 65535) / 65536` and is **overflow-free in i32**, unlike
the naive `(clamped * 65535) >> 16` which overflows when
`clamped == 65536` (`65536 * 65535 ≈ 4.29e9 > i32::MAX`).

```rust
fn emit_q32_to_unorm16(fb: &mut FunctionBuilder, value: VReg) -> VReg {
    let zero  = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: zero, value: 0 });
    let max_v = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: max_v, value: 65536 });

    // tmp = max(0, value); clamped = min(tmp, 65536)
    // (Use whichever IrOp the IR actually exposes: Imax/Imin or Select.
    //  Confirm in lpir/src/lpir_op.rs.)
    let above_zero = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IcmpLtS { dst: above_zero, lhs: value, rhs: zero });
    let tmp = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::Select { dst: tmp, cond: above_zero, if_true: zero, if_false: value });

    let above_max = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IcmpGtS { dst: above_max, lhs: tmp, rhs: max_v });
    let clamped = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::Select { dst: clamped, cond: above_max, if_true: max_v, if_false: tmp });

    // unorm = clamped - (clamped >> 16)
    let shift = fb.alloc_vreg(IrType::I32);
    let s16 = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::IconstI32 { dst: s16, value: 16 });
    fb.push(LpirOp::IshrU { dst: shift, lhs: clamped, rhs: s16 });
    let unorm = fb.alloc_vreg(IrType::I32);
    fb.push(LpirOp::Isub { dst: unorm, lhs: clamped, rhs: shift });
    unorm
}
```

> Op-name confirmations to make at implementation time: `Imax`/`Imin`
> may exist as direct ops; if so, prefer them over the
> select-based clamp above (cleaner). Check
> [`lpir/src/lpir_op.rs`](../../../lp-shader/lpir/src/lpir_op.rs).

### `module_globals_mutated` (Q2 / Q3 gating)

```rust
/// Conservative: returns true if any non-init function emits a Store
/// or Memcpy targeting the globals region. M2.0 ships with the
/// always-reset (returns `true` whenever globals exist) policy; the
/// targeted analysis is a follow-up optimisation.
fn module_globals_mutated(_module: &LpirModule) -> bool {
    true   // M2.0: always reset when globals exist; refine later.
}
```

> The "specialise per call" advantage (Q3) is preserved: even with
> always-reset, the `Memcpy` is emitted directly into the loop body
> rather than going through a runtime check. Removing the reset
> entirely for non-mutating shaders is a future optimisation
> tracked in the M2 roadmap follow-ups.

### `emit_globals_reset` helper

```rust
fn emit_globals_reset(fb: &mut FunctionBuilder, meta: &LpsModuleSig) {
    let globals_addr  = compute_globals_addr_vreg(fb, meta.globals_offset());
    let snapshot_addr = compute_globals_addr_vreg(fb, meta.snapshot_offset());
    let n_bytes = meta.globals_size() as u32;
    fb.push(LpirOp::Memcpy {
        dst: globals_addr,
        src: snapshot_addr,
        len: n_bytes,
    });
}
```

`compute_globals_addr_vreg` is a small helper that does
`vmctx + offset` via `IaddImm` to produce a pointer-typed vreg. This
mirrors the pattern existing code already uses for accessing
uniforms / globals through `VMCTX_VREG`.

> Verify the `LpirOp::Memcpy` shape in
> [`lpir/src/lpir_op.rs`](../../../lp-shader/lpir/src/lpir_op.rs);
> the `__shader_init` synthesis path doesn't itself emit `Memcpy`,
> so check the existing per-call reset path in cranelift /
> lpvm-native to see how `Memcpy` is currently constructed (e.g.
> `lpvm-cranelift/src/lpvm_instance.rs:reset_globals` does it via
> `core::ptr::copy_nonoverlapping`, but there's also an LPIR-level
> usage somewhere — search for `LpirOp::Memcpy`).

### Integration with the engine (preview, full wiring in Phase 4)

Phase 4's `LpsEngine::compile_px` will sandwich this between
`lps_frontend::lower` and the backend's `compile`:

```rust
let (mut ir, mut meta) = lps_frontend::lower(&naga)?;
let render_fn_index = validate_render_sig(&meta, output_format)?;
let render_fn_name = synth::synthesise_render_texture(
    &mut ir, &mut meta, render_fn_index, output_format,
)?;
let module = self.engine.compile(&ir, &meta)?;
LpsPxShader::new(module, meta, output_format, render_fn_index, render_fn_name)?
```

(Phase 4 owns this wiring; this phase just needs the API ready.)

### Tests added in this phase

**Sanity unit tests (in `synth/render_texture.rs` test module):**

```rust
// Construct a minimal LPIR module with a stub `render(vec2) -> vec4`,
// run synthesis for Rgba16Unorm, then assert structural properties
// of the resulting __render_texture_rgba16 function.

#[test]
fn synth_rgba16_appends_function_and_sig_in_lockstep() {
    let (mut ir, mut meta) = make_stub_render_module(LpsType::Vec4);
    let n_before = ir.functions.len();
    let name = synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::Rgba16Unorm).unwrap();
    assert_eq!(name, "__render_texture_rgba16");
    assert_eq!(ir.functions.len(), n_before + 1);
    assert_eq!(meta.functions.len(), n_before + 1);
    assert_eq!(meta.functions.last().unwrap().name, name);
    assert_eq!(meta.functions.last().unwrap().kind, LpsFnKind::Synthetic);
}

#[test]
fn synth_r16_picks_correct_name_and_arity() {
    let (mut ir, mut meta) = make_stub_render_module(LpsType::Float);
    let name = synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::R16Unorm).unwrap();
    assert_eq!(name, "__render_texture_r16");
    let synth_fn = ir.functions.values().find(|f| f.name == name).unwrap();
    assert_eq!(synth_fn.param_count, 3);   // tex_ptr, width, height
    assert_eq!(synth_fn.return_types.len(), 0);
}

#[test]
fn synth_signature_passes_phase_2_validator() {
    let (mut ir, mut meta) = make_stub_render_module(LpsType::Vec4);
    let name = synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::Rgba16Unorm).unwrap();
    let synth_ir = ir.functions.values().find(|f| f.name == name).unwrap();
    // Validate against the Phase 2 helper (renamed if needed):
    lpvm::validate_render_texture_sig_ir(synth_ir).expect("synth must satisfy validator");
}
```

**Q7 inliner-presence assertion (the test we owe):**

Per Q7 (resolved in [`00-notes.md`](./00-notes.md)), this is a
**positive** assertion today (exactly one `Call` to `render` in the
synthesised body), with a clear comment explaining how to invert it
when the inliner ships and starts fusing.

```rust
// lp-shader/lp-shader/src/synth/render_texture.rs (test module)

/// Inliner regression sanity (Q7).
///
/// Today, on this branch (no inliner integrated), `__render_texture`
/// must contain exactly **one** `Call` op targeting the user `render`
/// function. When the LPIR inliner integration milestone lands and
/// fuses `render` into the loop body, this test will start failing —
/// at which point the assertion should be **inverted**:
///
///   - assert zero `Call` ops in the synthesised body, and
///   - assert presence of inlined-body ops (the user `render`'s
///     control flow / arithmetic now appearing inside this function).
///
/// See: docs/plans/2026-04-17-lp-shader-textures-stage-v/00-notes.md (Q7)
///      docs/roadmaps/2026-04-16-lp-shader-textures/m2-render-frame.md
#[test]
fn synthesised_body_calls_render_once_inliner_unintegrated() {
    let (mut ir, mut meta) = make_stub_render_module(LpsType::Vec4);
    let name = synthesise_render_texture(&mut ir, &mut meta, 0, TextureStorageFormat::Rgba16Unorm).unwrap();
    let synth_fn = ir.functions.values().find(|f| f.name == name).unwrap();

    let render_callee_id = ir.functions.values().find(|f| f.name == "render").unwrap().id;
    let calls_to_render = synth_fn.body.iter().filter(|op| {
        matches!(op, LpirOp::Call { callee: CalleeRef::Local(id), .. } if *id == render_callee_id)
    }).count();
    assert_eq!(calls_to_render, 1, "expected exactly one Call to render in __render_texture body");
}
```

`make_stub_render_module(return_ty)` is a small test helper that
builds a module containing one user function `render(vec2) -> <ty>`
with a trivial body (returns a constant). Place it inline in the
test module.

## Validate

```bash
cargo check -p lp-shader
cargo test  -p lp-shader synth::render_texture
```

Phase 5 exercises the full pipeline end-to-end with real shaders;
this phase's tests are structural only (does the synth produce the
right shape).
