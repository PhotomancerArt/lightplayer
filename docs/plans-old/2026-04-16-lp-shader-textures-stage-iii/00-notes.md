# M2 — `render_frame` Pixel Loop: Planning Notes

## Scope of Work

Implement `LpsPxShader::render_frame` — the per-pixel loop that calls
`render(vec2 pos)` for each pixel and writes the result into `LpsTextureBuf`.

Two paths:
1. **Generic (slow) path**: Uses `LpvmInstance::call_q32` through the trait.
   Works for any backend. Auto-resets globals per call.
2. **Cranelift fast path**: Uses `DirectCall::call_i32_buf` with raw vmctx
   pointer. Mirrors existing `lpfx-cpu/render_cranelift.rs`.

Also centralizes Q32→unorm16 conversion, which is currently duplicated in
`lpfx-cpu/render_cranelift.rs` and `lp-engine/gfx/native_jit.rs`.

## Current State of Codebase

### `LpsPxShader` (lp-shader/lp-shader/src/px_shader.rs)

Has `module`, `instance: RefCell<M::Instance>`, `output_format`, `meta`,
`render_fn_index`. `render_frame` is a stub that only applies uniforms.

### `LpvmInstance` trait (lpvm/src/instance.rs)

- `call(name, &[LpsValueF32]) -> Result<LpsValueF32>` — auto-resets globals
- `call_q32(name, &[i32]) -> Result<Vec<i32>>` — auto-resets globals
- `set_uniform(path, &LpsValueF32)` / `set_uniform_q32(path, &LpsValueQ32)`

No `reset_globals()` or direct call on the trait.

### Cranelift direct call (lpvm-cranelift/src/direct_call.rs)

`DirectCall` holds a resolved function pointer. Methods:
- `call_i32(vmctx, args) -> Vec<i32>` — allocates return vec
- `call_i32_buf(vmctx, args, out) -> ()` — zero-alloc, writes to caller buf

`CraneliftInstance` has `vmctx_ptr()`, `reset_globals()`.
`CraneliftModule` (aka `JitModule`) has `direct_call(name) -> Option<DirectCall>`.

### Native JIT direct call (lpvm-native/src/rt_jit/)

`NativeJitDirectCall` resolved from `NativeJitModule::direct_call(name)`.
`NativeJitInstance::call_direct(handle, args, out)` — zero-alloc.
`NativeJitInstance::reset_globals()`.

### Existing render loops (reference implementations)

**lpfx-cpu/render_cranelift.rs** — Cranelift DirectCall fast path:
- `reset_globals()` per pixel
- `(x as i32) * 65536` for coords (integer pixel coords, NOT pixel centers)
- 5 args: `fragCoord.xy`, `outputSize.xy`, `time` (old `render(vec2,vec2,float)` convention)
- Q32→unorm16: `clamp(0, 65536)` then `(val * 65535) / 65536` via i64

**lp-engine/gfx/native_jit.rs** — NativeJit direct call:
- Same Q32 math, same 5-arg convention
- Does NOT reset globals per pixel

### Q32→unorm16 conversion

Not centralized. Duplicated in both render loops above:
```rust
let clamp = |v: i32| v.max(0).min(65536);
let channel = ((clamp(q32_val) as i64 * 65535) / 65536i64) as u16;
```

Roadmap note says simpler `min(q32, 65535)` would also work (single
discontinuity at pure white is invisible).

### Pixel coordinate convention

Design doc says `pos` = pixel centers `(0.5, 0.5)` to `(w-0.5, h-0.5)`,
like `gl_FragCoord`. Existing loops use integer coords `(0, 0)` to
`(w-1, h-1)` scaled by `65536`.

## Questions

### Q1: Pixel coordinate convention — pixel centers or integers?

**Context**: The M1 design doc says `pos` ranges from `(0.5, 0.5)` to
`(width - 0.5, height - 0.5)`, matching `gl_FragCoord`. Existing loops in
lpfx-cpu and lp-engine use integer coords (`x * 65536`).

In Q32: pixel centers would be `x * 65536 + 32768`.

**Suggested approach**: Use pixel centers as specified in the design. This
matches the GL convention, and any future GPU path will use `gl_FragCoord`
natively. The `+ 32768` is trivial cost.

**Answer**: Pixel centers. `(0.5, 0.5)` to `(w-0.5, h-0.5)`. In Q32:
`x * 65536 + 32768`. Matches GL convention and encourages resolution
independence (`pos / outputSize` yields proper normalized UV).

### Q2: Slow path first, fast path second, or both?

**Context**: The roadmap suggests a generic slow path (backend-agnostic) and
a Cranelift-specific fast path. The slow path uses `call_q32("render", &args)`,
which allocates a `Vec<i32>` per pixel. The fast path uses `call_i32_buf`
into a stack buffer (zero alloc).

Options:
- (a) Slow path only in this plan, fast path in a later stage
- (b) Both paths in this plan
- (c) Fast path only (Cranelift), skip the slow path
- (d) Synthetic LPIR `__render_texture` (true fast path — inline render()
  into a compiled pixel loop, no per-pixel call overhead)

**Suggested approach**: (b) Both paths. The slow path is simple (few lines)
and proves correctness. The fast path is also few lines, mirrors existing
code, and is what we'll actually use. Ship both.

**Answer**: **Synthetic LPIR `__render_texture`**. After exploration, this
is both faster *and* simpler than the DirectCall host loop:

- Performance: one compiled function, `render()` inlined into the loop body,
  backend optimizes across the loop (hoist invariants, etc.)
- Simplicity: `render_frame` = one `call_q32("__render_texture", &[ptr, w, h])`
  through the `LpvmInstance` trait
- No generic leakage: `LpsPxShader` no longer needs to be generic over `M`
- No backend-specific impls, no `DirectCall` plumbing, no per-backend code
- Portability: works on Cranelift, native JIT, WASM, and emulator via the
  same code path

**LPIR capability check**: `lpir_op.rs` already has everything needed —
integer arithmetic (`Iadd`, `Imul`, `IshlImm`), comparisons (`IltS`),
loops (`LoopStart`/`Break`/`End`), memory (`Load`/`Store`/`Memcpy`),
function calls (`Call` with `CalleeRef`), constants. Globals reset via
`Memcpy` from snapshot region.

**Missing pieces** (dependencies for this plan):
1. **`Store16` LPIR op** — current `Store` is 32-bit. Needed to write u16
   channels for `Rgba16Unorm` cleanly. Small addition. (Alternative: pack
   two u16 channels into i32 and use 32-bit Store — works for RGBA but
   awkward for future Rgb16Unorm where 3 channels don't pack cleanly.)
2. **Stable function IDs** — from `feature/inline` branch. Needed to add
   new functions (`__render_texture`) to LPIR programmatically.

### Q3: Where should Q32→unorm16 live?

**Context**: Currently duplicated in lpfx-cpu and lp-engine. Options:
- (a) `lps-shared` (accessible to all crates)
- (b) `lp-shader` (private helper in `px_shader.rs` or a new `convert.rs`)
- (c) `lps-shared::TextureStorageFormat` method

**Suggested approach**: (b) In `lp-shader` for now. It's the only consumer
in the new path. If lpfx-cpu and lp-engine want it later, we can move it
to `lps-shared`. Avoid premature generalization.

**Answer**: `lps-q32`. It's fundamentally a Q32 arithmetic operation
(`q32_to_unorm16`), and that crate is the home for Q32 math. Also means
lpfx-cpu and lp-engine can deduplicate to it when ready.

### Q4: Should `render_frame` take `&self` or `&mut self`?

**Context**: Current stub takes `&self` (instance behind `RefCell`). For the
fast path (Cranelift), we need `&mut CraneliftInstance` for `reset_globals()`
and `vmctx_ptr()`. The `RefCell` approach works but adds runtime borrow
checking overhead per pixel.

Options:
- (a) Keep `&self` with `RefCell` (current design)
- (b) Switch to `&mut self` (simpler, no runtime cost)

**Suggested approach**: Keep `&self` + `RefCell` for now. The `RefCell` borrow
is once for the entire frame (borrow_mut at start, hold through the loop),
not per-pixel. No overhead concern.

**Answer**: `&self`. Rendering a frame is not semantically mutative — the
internal `LpvmInstance` mutation is an implementation detail. Keep `RefCell`
(borrow once per frame, not per pixel).

### Q5: How do we access backend-specific fast paths through `LpsPxShader<M>`?

**Context**: `LpsPxShader<M: LpvmModule>` is generic. The fast path needs
concrete types (`CraneliftModule`, `CraneliftInstance`, `DirectCall`).

Options:
- (a) `impl LpsPxShader<CraneliftModule>` with a `render_frame_fast` method
- (b) Detect backend at runtime and dispatch
- (c) The slow path is good enough; skip fast path for now

**Suggested approach**: (a) Add `impl LpsPxShader<CraneliftModule>` with
specialized `render_frame` that uses `DirectCall`. This is the same pattern
as the roadmap's `impl FragInstance<CraneliftInstance>`. The generic
`render_frame` on `impl<M> LpsPxShader<M>` uses the slow path.

### Q6: Should the fast path resolve `DirectCall` once or per-frame?

**Context**: `DirectCall` is cheap to resolve (one hash lookup). But
resolving once at construction and storing it on `LpsPxShader` would avoid
even that.

Options:
- (a) Resolve once in `LpsPxShader::new`, store as field
- (b) Resolve per-frame in `render_frame`

**Suggested approach**: (a) Resolve once. Store `DirectCall` on the
Cranelift-specific `LpsPxShader`. This matches how lpfx-cpu and lp-engine
do it.

But: `LpsPxShader` is generic over `M`, and `DirectCall` is
Cranelift-specific. We'd need either a wrapper struct or a specialized
constructor. Alternatively, resolve lazily on first `render_frame` call.

### Q7: Reset globals per pixel?

**Context**: `call_q32` auto-resets globals. Direct calls do not.
`render_cranelift` explicitly resets per pixel. `render_native_jit_direct`
does not.

For a pixel shader calling `render(vec2 pos)`, mutable globals would be
unusual but possible (e.g., a shader using a global counter).

**Suggested approach**: Reset globals per pixel for correctness. This is
cheap when `globals_size == 0` (early return). Only the direct-call path
needs this explicitly; `call_q32` already does it.
