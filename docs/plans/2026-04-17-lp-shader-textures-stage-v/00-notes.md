# M2.0 — `__render_texture` Plan Notes

Roadmap: [`docs/roadmaps/2026-04-16-lp-shader-textures/m2-render-frame.md`](../../roadmaps/2026-04-16-lp-shader-textures/m2-render-frame.md)

Prerequisite milestones:
- **M1.1 (done)** — six narrow memory ops + `R16Unorm` / `Rgb16Unorm`
  formats + `compile_px` validation.

(M1.2 typed-ABI refactor was considered and dropped in favour of a
dedicated `LpvmInstance::call_render_texture` trait method — see Q8.)

## Scope of work

Move the per-pixel render loop into LPIR by synthesising a
`__render_texture(tex_ptr, width, height)` function in the `LpirModule`
*after* `lps_frontend::lower()` and *before* backend `compile()`. The
function:

- Iterates `y` in `[0, height)` and `x` in `[0, width)`.
- Resets globals from the in-buffer snapshot each pixel
  (`Memcpy(globals, snapshot, globals_size)`) **conditionally** based on
  whether globals can be mutated.
- Computes Q32 pixel coordinates (pixel-center convention; see Q1).
- Calls `render(pos)` (real `Call` until the inliner lands; see Q2).
- Converts each Q32 channel to unorm16 with the existing
  `clamp + (v * 65535) >> 16` math.
- Writes channels to `tex_ptr + pixel_offset` via `Store16`.

`__render_texture` is parameterised by `TextureStorageFormat` (channels,
bytes-per-pixel, return type). One specialised function per `compile_px`
call (we know the format at compile time).

In parallel, extend `LpvmInstance` with a dedicated
`call_render_texture` method (single fn, internal entry caching per
backend) that lets `LpsPxShader::render_frame` invoke the synthesised
function via a typed, allocation-free hot path — bypassing the
generic `call_q32` slow path entirely.

Refactor `LpsPxShader` to drop the `<M: LpvmModule>` generic via a
`Box<dyn PxShaderBackend>` so the public type is monomorphic.

## Current state of the codebase

### Pixel rendering today

Three host-side per-pixel loops exist, all using the **legacy** lpfx
shader contract `(frag_x, frag_y, output_size_x, output_size_y, time)`
returning Q32 RGBA via `DirectCall::call_i32_buf`:

- `lpfx/lpfx-cpu/src/render_cranelift.rs`  → calls `reset_globals()` per
  pixel.
- `lp-core/lp-engine/src/gfx/cranelift.rs` → uses a stack `VmContextHeader`
  default + `DirectCall::call_i32_buf`, **no per-pixel reset**.
- `lp-core/lp-engine/src/gfx/native_jit.rs` → `NativeJitInstance::call_direct`,
  no per-pixel reset.

Q32 → unorm16 is identical in all three: `clamp(v, 0, 65536)` then
`((v as i64 * 65535) / 65536) as u16` (or shift by 16 — semantically
equal in-range).

Consumers expect the **legacy 5-arg shader interface**, not the new
`render(pos: vec2) -> vecN` contract M1 introduced. This affects the
"migrate consumers" deliverable (see Q9).

### `LpvmInstance` invocation

`lp-shader/lpvm/src/instance.rs` exposes
`call_q32(&mut self, name, &[i32]) -> Result<Vec<i32>, _>`. Both
Cranelift (`lpvm-cranelift/src/lpvm_instance.rs`) and native JIT
(`lpvm-native/src/rt_jit/instance.rs`) call `reset_globals()` on entry.
So calling `__render_texture` resets globals **once**; the per-pixel
reset must happen **inside** `__render_texture` via emitted `Memcpy`
ops.

The slow `call_q32` path does, per call: HashMap symbol lookup + arg
`Vec<i32>` allocation + per-arg flattening + return `Vec<i32>` allocation
+ per-return unflattening. Negligible per frame, but historically a 10×
slowdown when applied per *pixel* — which is what motivated the
`DirectCall` fast path. We will follow the same precedent for
`__render_texture` by giving it a dedicated trait method; see Q8.

### Globals + `__shader_init`

`lps-frontend/src/lower.rs` already synthesises `__shader_init` (constant
init → `Store` into vmctx). After init the backend `snapshot_globals()`
copies globals → snapshot region inside the same vmctx buffer. Layout
in `lps-shared/src/sig.rs`: `header | uniforms | globals | snapshot`.

For M2.0 we need the offsets at synthesis time — already available from
`LpsModuleSig` (`vmctx_globals_offset`, `vmctx_snapshot_offset`,
`globals_size`).

### LPIR builder API (`lp-shader/lpir/src/builder.rs`)

Helpers exist for: `push_if/else/end_if`, `push_loop/continuing/end_loop`,
`push_block/end_block`, `push_exit_block`, `push_call`, `push_return`,
`add_param`, `alloc_vreg`, `alloc_slot`, raw `push(LpirOp::*)`. No
dedicated helper for `Break`/`Continue`/`BrIfNot` — use raw `push`.

`Block`/`ExitBlock` (forward-only structured CFG) landed via the M1.1
cherry-pick from `feature/inline`.

### Module mutation post-`lower()`

`LpirModule.functions: BTreeMap<FuncId, IrFunction>` — append a new
function with `FuncId(max+1)` to avoid renumbering. `LpsModuleSig.functions`
also needs a matching `LpsFnSig` so callers and validation see the symbol.
There is no `ModuleBuilder::from(LpirModule)` rehydrate; mutate the module
directly.

### Q32 → unorm16

`lps-q32` exposes `Q32::to_u16_saturating` with the same math; we will
**emit** the equivalent LPIR sequence inline (no helper function emitted
in IR).

### Inliner status

The actual inliner pass commit (`2d110e8c`) is **not** on `feature/lpfx`.
Only the prerequisite plumbing landed (`Block`/`ExitBlock`, stable
`FuncId`/`ImportId`, `CompilerConfig`). So the synthesised
`__render_texture` will contain a real `Call render(...)` until the
inliner ships. The roadmap's "no `Call` in op stream" regression test
must be deferred.

### `LpsPxShader` today (`lp-shader/lp-shader/src/px_shader.rs`)

Generic over `M: LpvmModule`. `render_frame` is a stub that only applies
uniforms. Tests in `lp-shader/lp-shader/src/tests.rs` exercise
uniform application but assert nothing about pixel contents.

### `LpsTextureBuf`

`width()`, `height()`, `format()`, `data()/data_mut()`, `guest_ptr() ->
LpvmPtr`, `row_stride()`. `LpvmPtr::guest_value() -> u64` (no `.raw()`).
Buffer is tightly packed (`width * height * bytes_per_pixel`,
`debug_assert`'d).

The underlying `LpvmBuffer` carries both representations (host pointer +
32-bit guest offset), so the trait method (Q8) can give each backend
exactly what its calling convention needs.

## Questions

### Q1. Pixel coordinate convention — center or corner? ✅ resolved

**Context.** The roadmap pseudo-code uses pixel-center coords
(`(x << 16) + 32768`). The existing host loops in `lpfx-cpu` and
`lp-engine` use pixel-corner coords (`x * 65536`). We discussed this
during M1.1 planning and the user "explicitly requested to proceed with
the pixel center convention".

**Suggested answer.** Pixel center (`(x << 16) + 32768`). It matches
GLSL `gl_FragCoord.xy` semantics for full-pixel coverage and is what we
already chose. The legacy host paths can keep their corner convention
until they migrate (see Q9).

**Answer:** Pixel center (`(x << 16) + 32768`). The convention is the
sampling position *within* a pixel cell, not the texture origin.

### Q2. Inliner — accept `Call` overhead, or land inliner first? ✅ resolved

**Context.** Without the inliner pass on `feature/lpfx`, every pixel
will execute a real `Call render(...)`. That is a per-pixel function
call + register save/restore in the generated code, defeating one of
the perf motivations in the roadmap rationale. Three options:

  A. **Land inliner first.** Cherry-pick `2d110e8c` (and any cleanup
     commits) ahead of M2.0. Adds scope but delivers full perf.
  B. **Accept `Call` overhead in M2.0.** Build the synthesis correctly,
     defer inlining to a follow-up. Simpler M2.0; perf parity comes later.
  C. **Pull just enough of the inliner to inline `render` only.**
     Hand-rolled, special-cased — avoids importing the generic pass.
     Probably more work than (A).

**Suggested answer.** **B.** Ship `__render_texture` with a real `Call`,
get end-to-end correctness + tests landed. Track inliner integration as
a separate follow-up milestone. This keeps M2.0 reviewable; perf is
already better than today (single host call instead of `width*height`).

**Answer:** **B.** Ship M2.0 with a real `Call render(...)` per pixel.
Inliner integration becomes a separate follow-up milestone (purely a
backend perf win, validated against M2.0's golden tests).

### Q3. Per-pixel global reset — always emit, or conditional? ✅ resolved

**Context.** `lpfx-cpu` resets globals every pixel; `lp-engine`
Cranelift/native paths skip it. The semantically safe answer is to
emit `Memcpy(globals, snapshot, globals_size)` per pixel inside
`__render_texture` whenever `globals_size > 0`. Skipping it would let
mutations from one pixel leak into the next.

**Suggested answer.** Always emit when `globals_size > 0`. Match
`lpfx-cpu` semantics (correctness > perf). When the inliner lands,
constant-prop / dead-store will optimise the `Memcpy` away when
`render` doesn't write globals.

**Answer:** **Emit conditionally** based on a "globals may be mutated"
flag. This is the hot path and exactly the kind of codegen-time
optimisation we should be doing.

Seed heuristic for M2.0: emit `Memcpy` iff the module has at least one
**non-const** (mutable) global. Concretely, iterate `LpsModuleSig`
globals/globals layout — if everything is `const`-style (initialised
once via `__shader_init`, never written by user code), skip the
`Memcpy`. Otherwise emit it.

Follow-up refinement (separate plan): actual mutation analysis — scan
`render`'s LPIR for stores into the globals region and elide the
`Memcpy` when there are none. Tracked as future work.

### Q4. Per-format synthesis: one specialised function, or runtime branch? ✅ resolved

**Context.** `compile_px` knows the `TextureStorageFormat` at compile
time. We can either emit one `__render_texture` specialised for that
format (loop unrolled per channel, fixed `bytes_per_pixel`), or emit
one generic function and pass the format as a runtime arg.

**Suggested answer.** **Specialise per call.** `LpsPxShader` is created
once per `compile_px(format)`; emitting a specialised function gives
the backend the best chance to const-fold offsets and avoid runtime
dispatch. Cleaner LPIR too.

**Answer:** **Specialise per call.** Bake channel count, bytes-per-pixel,
and per-channel store offsets into the emitted IR — keep the runtime
inner loop as small as possible.

Additional constraint: the synthesis routine must be **backend-agnostic
and reusable across all backends** (Cranelift, native JIT, WASM,
emulator). It operates on `LpirModule` + `LpsModuleSig` +
`TextureStorageFormat` and produces an `IrFunction`; any backend then
compiles it via its existing `compile()` path. This reinforces putting
the synthesis in the high-level `lp-shader` crate (see Q6) where all
backends already converge.

The function name encodes the format so a single module can hold one
synthesised render function per format that ever gets requested.
Suggested naming: `__render_texture_r16`, `__render_texture_rgb16`,
`__render_texture_rgba16` (pinned at synthesis time; each backend's
`LpvmInstance` caches the resolved entry internally on first call).

### Q5. `LpsPxShader` type-erasure — `Box<dyn>` boundary? ✅ resolved

**Context.** Roadmap proposes:

```rust
pub struct LpsPxShader {
    inner: Box<dyn PxShaderBackend>,
    output_format: TextureStorageFormat,
    meta: LpsModuleSig,
    render_fn_name: String,  // format-specific (e.g. "__render_texture_rgba16")
}

trait PxShaderBackend {
    fn call_render_texture(
        &mut self,
        name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), LpsError>;

    fn set_uniform(&mut self, name: &str, value: &LpsValueF32)
        -> Result<(), LpsError>;
}
```

The trait object owns `(M, M::Instance)` together. Hot-path entry
caching is the *backend's* responsibility (Q8): the LPVM instance
caches the resolved render-texture entry internally on first call, so
`PxShaderBackend::call_render_texture` just forwards. `lp-shader` is
`no_std + alloc`, so `Box<dyn>` is fine.

**Suggested answer.** Yes, follow the roadmap. Two trait methods is a
small surface; the adapter just forwards.

**Answer:** Accepted. `Box<dyn PxShaderBackend>` boundary as described.
The per-`render_frame` v-call is small compared to other per-frame
overhead and not worth optimising prematurely. The instance-internal
entry cache (Q8) ensures we pay the lookup cost exactly once.

### Q6. Where does the synthesis live — which crate / file? ✅ resolved

**Context.** Three plausible homes:

  A. `lp-shader/lp-shader/src/synth/render_texture.rs` (new module
     inside the high-level facade crate). Fits with `LpsPxShader`,
     can read `TextureStorageFormat` directly.
  B. `lp-shader/lps-frontend/src/synth.rs` (alongside `__shader_init`).
     Frontend already does synthesis; format would have to flow in.
  C. `lp-shader/lpir/src/synth/render_texture.rs` (in lpir, generic
     primitive). Cleanest dependency story; might be over-engineered.

**Suggested answer.** **A.** The synthesis is parameterised on
`TextureStorageFormat` (a `lps-shared` concept) and the `render`
function's signature, both of which live one layer above the frontend.
Putting it in `lp-shader` keeps `lps-frontend` GLSL-focused and
isolates the texture-loop concern.

**Answer:** **A.** `lp-shader/lp-shader/src/synth/render_texture.rs`.
Shader execution and pixel-loop concerns belong in the `lp-shader`
crate; the rest of the system (lpir, lpvm-*, lps-frontend) is properly
agnostic.

### Q7. Inliner regression test — defer, ignore, or invert? ✅ resolved

**Context.** Roadmap calls for a test that asserts no `Call` to
`render` in the `__render_texture` op stream. With Q2 = B, that test
would fail today.

**Suggested answer.** Defer it to the inliner integration milestone.
In M2.0 add a different sanity test: assert `__render_texture` exists
with the expected signature and contains exactly one `Call` to
`render`. That test will be updated (to assert zero) when the inliner
lands.

**Answer:** **C.** Write a positive sanity assertion now: exactly one
`Call` op in `__render_texture` targeting the `render` function, with
the expected arg/result counts. When the inliner ships and starts
fusing, this test will (correctly) fail, signalling that the assertion
needs to be inverted.

The test must carry a clear, well-commented header explaining:
- why it asserts presence rather than absence today (no inliner on
  this branch);
- how to invert it once the inliner lands (assert zero `Call`s plus
  presence of inlined body ops);
- the link to this plan / roadmap milestone.

### Q8. Texture pointer ABI — how does the host hand the texture to the shader? ✅ resolved

**Context.** `__render_texture` takes a pointer-typed first argument.
We considered three approaches; all but the third had real problems.

  A. **Cast `LpvmPtr::guest_value()` to `i32` and use the existing
     `call_q32(&[i32])` slow path.** Works on all 32-bit-guest
     backends (RV32, emu, WASM); breaks on `lpvm-cranelift` host JIT where
     `IrType::Pointer` is 64-bit. That made a single `call_q32` path
     incapable of carrying the texture pointer for every backend without
     packing hacks — one reason the dedicated trait method won.
  B. **Typed `call_q32` ABI refactor.** Promote args from `&[i32]` to
     `&[LpsValueQ32]` and add `Pointer(u64)` variants to both
     `LpsValueQ32` and `LpsValueF32`. Clean type-system answer but
     requires churn across every backend's `call_q32` impl + every
     call site in the workspace, and only exists to support a single
     hot-path call. (Drafted, then abandoned — see "Out of scope"
     below.)
  C. **32-bit guest address space (allocation pool in vmctx) on JIT.**
     Make all pointers uniformly 32-bit offsets across all backends by
     giving the JIT its own pool. Solves the asymmetry but couples
     deref sites to a vmctx-known pool base — and vmctx isn't a
     fundamental LPIR concept (the backend doesn't know which param
     is the pool base), so the convention has to leak out somewhere.
     Also tangles with stack/sret pointers, which legitimately need
     to stay host-width on JIT.
  D. **Dedicated `LpvmInstance::call_render_texture` trait method.**
     Bake the hot-path call shape into the trait itself with a typed
     signature `(name, &mut LpvmBuffer, u32, u32) -> ()`. Each backend
     implements the resolve + invocation natively for its own ABI,
     **caching the resolved entry internally** on first call:
       - `lpvm-cranelift` JIT extracts `texture.host_ptr()` (real 64-bit host pointer);
       - RV32 / emu / WASM / Wasmtime extract `texture.guest_base() as i32`.
     `IrType::Pointer` semantics in the LPIR stay unchanged (already
     backend-polymorphic).

**Suggested answer.** **D.**

Why D wins:

1. **No type / value system churn.** `LpsValue*` and `call_q32` keep
   their current shapes. No call-site sweep across the workspace.
2. **No memory-infrastructure rework.** `LpvmBuffer` already carries
   both representations; backends extract what they need.
3. **Faster than the slow path.** First call: name-resolve, cache the
   entry. Every subsequent frame: cached function-pointer call + three
   primitive args. No `Vec<i32>` allocation, no per-arg marshal /
   unmarshal. Avoids the 10× cost we measured historically when the
   per-pixel call went through the generic path.
4. **JIT supported naturally.** The trait method's signature accepts
   `&mut LpvmBuffer`, so each backend gets exactly the pointer form
   it can use. No "JIT unsupported" bailout needed.
5. **Cleanly additive.** `call_q32` continues to serve init / uniform
   updates / one-off calls. Only `__render_texture` gets the dedicated
   path.
6. **Single method, internal caching.** No public handle type, no
   warmup ceremony. The instance owns the lookup→cache lifecycle; the
   first call pays the lookup cost, the rest reuse it. Same hot-path
   throughput as a separate `lookup` + `call` pair, with a smaller
   trait surface.

Cost: each LPVM backend gets a small dedicated impl (cache-or-resolve
+ extract pointer-form + invoke; ~30–60 lines apiece). That's
mechanical work, not architectural risk — and it's precisely the right
place for backend-specific call-convention code.

The user explicitly endorsed the precedent: "it's not bad architecture
to bake your hot path into the native language API shape, even if it's
a 'special' case — it's actually the *main* case." The empirical
justification is that we already paid (and then walked back) the
generic-call-path cost for per-pixel rendering at 10× slowdown.

**Answer:** **D — dedicated trait method (single, with internal caching).**

Concretely, add to `LpvmInstance` (in `lp-shader/lpvm/src/instance.rs`):

```rust
pub trait LpvmInstance {
    /// Hot path: invoke the synthesised `__render_texture[_<format>]`
    /// entry by name. The instance is responsible for resolving the
    /// entry on first call and caching it internally; subsequent
    /// calls with the same name should hit the cache.
    ///
    /// Validates signature shape `(Pointer, I32, I32) -> ()` on the
    /// first lookup. Returns the backend's existing `Error` type for
    /// missing symbol, signature mismatch, or guest trap.
    fn call_render_texture(
        &mut self,
        fn_name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error>;

    // existing call / call_q32 / set_uniform* unchanged.
}
```

No associated `RenderTextureHandle` type, no separate `lookup_*`
method. The cache is an implementation detail of each backend
(typically a `Option<(String, ResolvedEntry)>` field, or a small
`HashMap<String, ResolvedEntry>` when more than one format may be
called on the same instance — uncommon but cheap to support).

Per-backend impls (each backend keeps the cache in its own field; the
public surface stays uniform):

- **lpvm-cranelift**: cache stores a `*const u8` (cast JIT'd function
  pointer). `call_render_texture` casts to the right `extern "C"` fn
  type and invokes with `(vmctx, host_ptr, w, h)`.
- **lpvm-native** (RV32fa JIT + emu): cache stores the resolved guest
  entry. Invokes with `(vmctx, guest_offset, w, h)` per RV32 ABI.
- **lpvm-wasm**: cache stores a `wasmtime::Func` (or browser
  equivalent). Invokes with `(vmctx, guest_offset, w, h)`.

Synthesis side (`lp-shader/src/synth/render_texture.rs`) emits the
LPIR with `tex_ptr: IrType::Pointer` — the existing polymorphic
pointer type — so each backend lowers it appropriately at codegen
time without per-backend synthesis variants.

Warmup-time validation moves out of the trait: `LpsPxShader::new`
inspects `meta()` directly to confirm `__render_texture_<format>`
exists with the expected signature *before* the first `render_frame`.
That keeps the "is the function present?" check off the hot path and
out of the trait surface.

**Out of scope (closed off):**
- The typed-`call_q32` ABI refactor (was M1.2; planned then dropped).
  We keep `call_q32(&[i32]) -> Vec<i32>` as-is; future hot paths
  follow the same trait-method precedent established here rather than
  forcing a generic-ABI redesign.
- The 32-bit guest-pool approach. Sandboxing and uniform pointer
  width are still attractive for other reasons, but the texture
  pointer specifically no longer requires either.

**Plan order:** M2.0 lands as a single milestone in
`docs/plans/2026-04-17-lp-shader-textures-stage-v/`. The trait
extension is a phase within this plan (not a separate milestone).

### Q9. Migrate host consumers — in M2.0 or follow-up? ✅ resolved

**Context.** Roadmap lists three consumers to migrate:
`lpfx/lpfx-cpu/src/render_cranelift.rs`, `lp-core/lp-engine/src/gfx/cranelift.rs`,
`lp-core/lp-engine/src/gfx/native_jit.rs`. **All three use the legacy
5-arg shader contract** (`frag_x, frag_y, out_w, out_h, time`), not the
new `render(pos: vec2) -> vecN` contract M1 introduced. Migration
requires either (a) those consumers to switch to the new contract too,
or (b) shaders that compile under both contracts. That is a
non-trivial, separable effort.

**Suggested answer.** Defer consumer migration to a separate milestone
(call it M2.1 or roll into M3). Keep M2.0 focused on:
`__render_texture` synthesis + `LpsPxShader` refactor + standalone
end-to-end tests inside `lp-shader`. The host consumers can migrate
once their shader pipeline targets the new contract.

**Answer:** Already covered by **M4 — Consumer Migration**
(`docs/roadmaps/2026-04-16-lp-shader-textures/m4-consumer-migration.md`).
M4's scope: lpfx-cpu, both lp-engine paths (`gfx/cranelift.rs`,
`gfx/native_jit.rs`), texture-type consolidation, `noise.fx` update.

The "Migrate consumers" bullet that previously lived in
`m2-render-frame.md` was stale (predates the M2/M4 split) and has
been removed from the roadmap as part of preparing this plan.

Context the user added: lpfx is the *new* pipeline (we're on
`feature/lpfx`), so `render_cranelift.rs` is itself transient lpfx
work; lp-engine will eventually go through lpfx rather than directly
through lp-shader. M4 handles all of that.

M2.0 stays focused on: `__render_texture` synthesis + LPVM trait
extension (Q8) + `LpsPxShader` refactor + standalone end-to-end tests
inside `lp-shader` + the inliner sanity test from Q7.

### Q10. End-to-end tests — which formats and shaders, on which backend? ✅ resolved

**Context.** Roadmap lists three correctness scenarios per format
(constant, gradient, uniform-driven). Three independent dials: test
depth (smoke vs correctness), format coverage (R16 / Rgb16 / Rgba16),
backend coverage (six `LpvmInstance` impls). Naive expansion is
3 × 3 × 6 = 54 tests; obviously wrong.

The user's framing (the deciding factor): **format tests are more
important than backend tests** because the backends will be covered by
integration tests:

- **fw-tests** (existing) → fw-emu → covers one format end-to-end on a
  near-hardware target.
- **fw-wasm** (future) → mirror for the wasm path.
- **lpfx wasm demo / authoring tool** (future) → covers the browser
  path.

So per-backend unit smoke tests would be ~5 nearly-identical
constant-color tests that integration will eventually obsolete. Skip
those in favour of focused per-format coverage. Workspace `cargo
build` already catches compile-time wiring of the trait extension on
every backend; the only remaining concern is *runtime* correctness,
which integration carries.

**Host execution for these tests is Wasmtime** (`lpvm-wasm` /
`WasmLpvmEngine`): deterministic, 32-bit guest pointers like RV32 /
browser, and the supported direction for `lp-shader` unit tests and
future `lp-cli` host execution (see **Q10a**). **`lpvm-cranelift` is not
the format-test target** — it remains in-tree for legacy callers and
for the Phase 2 handwritten smoke only.

**Answer:** **F+wasmtime+jit-smoke** — 5 tests total in M2.0: four
format-correctness tests on **Wasmtime**, plus one **lpvm-cranelift**
handwritten JIT smoke (Phase 2).

> Refinement (post-planning): the four format-correctness tests were
> first scoped for `lpvm-native::rt_emu`, then moved to
> `lpvm-cranelift` at the user's request; they **later moved again to
> Wasmtime** when we deprecated the host Cranelift JIT path for new work
> (see **Q10a**). Rationale unchanged for RV32: `fw-emu`-based
> integration tests (M4) are the right vehicle for end-to-end RV32
> validation. For the host path, Wasmtime provides an integration-grade
> engine with per-instance isolation and no 64-bit host-pointer ABI
> special case in `call_render_texture`.

**Format correctness on `lpvm-wasm` / Wasmtime (4 tests, full pipeline):**

1. **`R16Unorm` constant** — shader returns a fixed scalar; assert
   every pixel matches the expected unorm16 value across a small
   (e.g. 2×2) buffer. Catches: 1-channel synthesis, `bytes_per_pixel
   = 2`, scalar return type.
2. **`Rgb16Unorm` constant** — shader returns a fixed `vec3`; assert
   per-channel layout. Catches: 3-channel synthesis, `bytes_per_pixel
   = 6`, packed RGB store offsets, `vec3` return type.
3. **`Rgba16Unorm` constant** — shader returns a fixed `vec4`; assert
   per-channel layout. Catches: 4-channel synthesis, `bytes_per_pixel
   = 8`, `vec4` return type.
4. **`Rgba16Unorm` gradient** — shader returns the per-pixel position
   scaled to recover the integer pixel index; assert adjacent pixels
   differ as expected and the +0.5 pixel-centre offset (Q1) is
   present. Catches pixel-coord propagation (`pos_x` / `pos_y`
   computation) and the per-pixel synthesis loop body. Only needed
   on one format because the gradient math is format-agnostic.

**JIT trait smoke on lpvm-cranelift (1 test, handwritten LPIR):**

5. **`call_render_texture` smoke** — bypasses `compile_px` entirely.
   Builds a hand-rolled `IrFunction` matching the
   `(Pointer, I32, I32) -> Void` signature, registers it in an
   `LpirModule`, compiles via `lpvm-cranelift`, and asserts that
   `CraneliftInstance::call_render_texture` writes the expected
   bytes. This lands in **Phase 2** (before synth exists) so the
   trait extension has a runtime check between Phases 2 and 5.

**Existing test that stays:** `render_frame_sets_uniforms` continues
to cover the uniform-application path independently.

**Backends covered by build-only (beyond Wasmtime + JIT smoke):**
`NativeJitInstance`, `NativeEmuInstance`, `EmuInstance`,
`BrowserLpvmInstance`. Their `call_render_texture` impls are validated
by `cargo build` for compile-time correctness; runtime correctness falls
to fw-tests (RV32, M4) / future fw-wasm / future lpfx-wasm-demo.
`WasmLpvmInstance` additionally gets the four Phase 5 format tests. If
any of the build-only backends grows a dedicated integration suite that
doesn't yet exist, M2.0 doesn't block on it.

### Q10a. (Addendum) Why Wasmtime for host `lp-shader` tests instead of `lpvm-cranelift`? ✅ resolved

**Context.** The original M2.0 plan ran Phase 5 pixel tests on the
`lpvm-cranelift` host JIT because that matched `lp-cli` and was the only
in-tree host path with no fw-integration backstop.

**Decision (later).** Host execution for new work uses **`lpvm-wasm` /
`WasmLpvmEngine` (Wasmtime)**. The `lpvm-cranelift` / `JITModule` path
remains in the workspace but is **deprecated for new work**; full
removal is deferred until `lp-engine` / `lpfx-cpu` migrate off it (M4).

**Why pivot:**

1. **Non-deterministic state leak across multiple JIT instances in one
   process** — a long-standing Cranelift JIT issue: reproduces as
   deterministic-looking test failures under `cargo test
   --test-threads=1` (panic: “function must be compiled before it can be
   finalized”, `cranelift/jit/src/backend.rs`). We hit this before and
   disabled JIT in `lps-filetests` for the same class of problem.
2. **Wasmtime uses Cranelift internally with proper per-instance
   isolation** — similar codegen quality without sharing broken JITModule
   state across tests.
3. **32-bit guest pointers** — matches RV32, emulator, and browser;
   removes the 64-bit-host-pointer asymmetry that forced extra complexity
   into `call_render_texture` for the old host-JIT path.

The **Phase 2 handwritten smoke** in `lpvm-cranelift` stays: single
instance, hits the JIT trait implementation directly, and remains the
early guardrail for JIT trait-shape regressions if the path is needed
later.

### Q11. Does `compile_px` need to expose `__render_texture` separately, or just call it transparently? ✅ resolved

**Context.** `LpsModuleSig.functions` will gain an entry for
`__render_texture` (we need it for instance-side calling). Existing
synthetic functions like `__shader_init` are already exposed (not
filtered) by `LpsPxShader::meta()` today. Question: do we filter,
status-quo expose, or add a kind indicator?

A grep of the codebase showed that backends rely on a 1:1 zip between
`LpsModuleSig.functions` and `LpirModule.functions` (`lpvm-native/rt_jit/module.rs:63`,
`lpvm-wasm/compile.rs:60-81`). Filtering at the `LpsModuleSig` level
would break that zip — so any filtering must happen at the
`LpsPxShader::meta()` accessor only, leaving the underlying sig
unchanged.

**Answer:** **D — add a kind indicator on `LpsFnSig`.**

The user's preference: keep all functions visible (don't filter), but
let consumers distinguish synthetic-vs-user via a structured field
rather than the `__` prefix convention.

```rust
pub enum LpsFnKind {
    UserDefined,
    Synthetic,
}

pub struct LpsFnSig {
    // existing fields ...
    pub kind: LpsFnKind,
}
```

`lps-frontend` sets `kind = UserDefined` for user-lowered functions
and `kind = Synthetic` for `__shader_init`. M2.0's synthesis sets
`kind = Synthetic` for `__render_texture[_<format>]`.

Granularity choice: **coarse two-variant enum** (`UserDefined` /
`Synthetic`) rather than per-synthetic-function variants
(`SyntheticInit`, `SyntheticRenderTexture`, …). The `__` prefix and
the `name` field already distinguish *which* synthetic; the kind
field exists to answer the binary question "is this user code or
implementation detail?". Finer-grained variants can be added later
if a consumer actually needs to pattern-match on them.

(Confirm with the user during planning if they prefer the
finer-grained enum; if so, it's a small change.)

**Sub-decision:** since `meta()` keeps its current behaviour (returns
the full sig including synthetics), no new accessor is needed.
Consumers who want only user code filter via
`meta().functions.iter().filter(|f| f.kind == LpsFnKind::UserDefined)`.

### Q11.5. (Explored, rejected) Should `__shader_init` run per frame instead of per instance? ✅ resolved

**Context.** A natural follow-on from Q11 was: should we add a
`call_init_q32` fast path on the trait, mirroring `call_render_texture`?
That led to discovering that `__shader_init` currently runs **once at
instance construction** (every backend's `instantiate()` calls
`init_globals()` and the snapshot is then frozen for the lifetime of
the instance).

This is broken under desktop GLSL semantics: `variables.adoc:1279-1280`
explicitly allows global initializer expressions to call user-defined
functions, and only `const` globals are restricted to constant
expressions (`variables.adoc:1973-1976`). So a desktop GLSL shader
with `vec3 baseColor = uTint * 0.5;` at module scope would compute
`baseColor` from the *zero-initialized* uTint at instantiation, then
silently render a black frame after the user calls `set_uniform`.

**Why the bug is unreachable in practice (the deciding factor):**
the lpfx-gpu pipeline (planned in `docs/roadmaps/2026-04-15-lpfx/m2-gpu.md`)
goes GLSL → naga → WGSL → wgpu. WGSL module-scope `var<private>` can
only be initialized with const-expressions; uniform-dependent
module-scope initializers are not expressible. naga's GLSL frontend
either rejects them or hoists them into `main()` to produce valid
WGSL. Either way, **a GPU-portable shader cannot exercise the bug**.

ESSL similarly requires global initializers to be constant
expressions (`variables.adoc:1909-1913`).

**Answer:** **Keep the once-per-instance `__shader_init` model
as-is.** No per-frame init in `__render_texture`. No `call_init_q32`
trait method. M2.0 does not change the init story.

**Future-work notes (tracked separately, not part of M2.0):**
- **naga subset enforcement.** Tighten lps-frontend to reject
  GLSL inputs that wouldn't survive translation to WGSL (in
  particular, non-const-expression module-scope initializers). This
  formalises the constraint that's already enforced in practice
  by the GPU path.
- **Compile-time const-expression evaluation.** `__shader_init`
  exists today because globals with initializers are stored via
  emitted LPIR `Store` ops at runtime. With const-expression-only
  semantics, the lowering pipeline could evaluate initializers at
  compile time and bake the initial values into the module's static
  data (snapshot region preset). This would eliminate `__shader_init`
  entirely. Strictly an optimisation; orthogonal to M2.0.

### Q12. Trait extension scope — which backends are in scope for M2.0? ✅ resolved

**Context.** Trait methods are not optional: every type that
`impl LpvmInstance for X` must implement the new methods or it stops
compiling. The real question is real-impl vs `Err(Unsupported)` stub.

Six `LpvmInstance` impls in scope:

| Crate / impl | Role |
|---|---|
| `lpvm-cranelift::CraneliftInstance` | Host JIT (deprecated for new work; Phase 2 smoke) |
| `lpvm-native::rt_jit::NativeJitInstance` | RV32fa JIT, host execution (production) |
| `lpvm-native::rt_emu::NativeEmuInstance` | RV32fa via embedded emu (fw-emu integration target) |
| `lpvm-emu::EmuInstance` | Standalone RV32 emu |
| `lpvm-wasm::rt_wasmtime::WasmLpvmInstance` | Host Wasmtime (`lp-shader` tests; target for lp-cli) |
| `lpvm-wasm::rt_browser::BrowserLpvmInstance` | Browser-side |

Each impl is small (cache-or-resolve entry + extract pointer-form
from `LpvmBuffer` + invoke; ~30–60 lines). No architectural risk;
just per-backend calling-convention plumbing.

**Answer:** **A — real implementation on all six backends.**

Stubbing-then-implementing-later would be more total work than just
implementing once. Both candidates that might have been stubbed
(`BrowserLpvmInstance`, `lpvm-emu`) will eventually need real impls
anyway when their downstream consumers materialise (browser for the
lpfx web demo, lpvm-emu for whatever still uses it).

**Per-backend test posture (from Q10):** `lpvm-wasm` / Wasmtime runs
the four Phase 5 format tests; `lpvm-cranelift` gets the Phase 2
handwritten JIT smoke only. The remaining four implementations are
validated by `cargo build` for compile-time correctness; runtime
correctness is carried by integration tests as they materialise
(fw-tests for `lpvm-native::rt_emu`, future fw-wasm for the wasm path,
future lpfx web demo for `BrowserLpvmInstance`).

**Future-work note (out of scope for M2.0):** if we want broader
automated cross-backend rendering coverage, the right vehicle is a
new shader-filetest infrastructure analogous to `lps-filetests` that
makes it easy to switch backends per fixture. M2.0 does not build
this; it relies on per-backend impls being correct by construction
plus the Phase 2 JIT smoke + Wasmtime format tests + future integration coverage.

### Q13. What does `LpvmInstance::Error` look like for `call_render_texture` failures? ✅ resolved

**Context.** With the consolidated single-method shape (Q8), one
trait method covers both first-call resolve and per-frame invocation.
Failure modes:
- **First call only:** symbol missing, signature mismatch.
- **Every call:** guest trap, OOM, abort (same modes as `call_q32`).

The existing `LpvmInstance::Error` is per-backend; we need to make
sure these failure modes are expressible across all six impls without
inventing a new cross-cutting error enum.

**Answer:** Reuse each backend's existing `Error` type.

- **Resolve-time failures** map to whatever the backend already
  returns for "function not found" / "signature mismatch" today
  (these variants already exist on each backend's error enum from
  `call_q32` lookup paths — confirm during implementation; add the
  variant only where genuinely missing).
- **Call-time failures** use the same traps/aborts the backend
  already produces from `call_q32` (guest trap, host trap, OOM,
  RV32 illegal-instruction, wasmtime fault, etc).

`LpsPxShader` collapses the per-backend `Error` into `LpsError` at
the adapter boundary — the same pattern already used by
`set_uniform`. No new error variant is required at the `lp-shader`
layer; "render call failed" is sufficient to surface to consumers,
with the underlying backend message attached for debugging.

**Sub-question dissolved (was Q13b: handle safety / cross-instance
misuse).** With no public `RenderTextureHandle` type, there's nothing
for callers to mix up across instances. The cache lives entirely
inside the backend, keyed by `&str`; mismatches between modules are
impossible by construction.

## Proposed phase breakdown (driven by Q&A answers)

All open questions (Q1–Q13) are answered. Phases below will be
hardened into separate `0X-…md` plan files.

1. **`LpsFnKind` on `LpsFnSig` (Q11).** Add the `UserDefined` /
   `Synthetic` enum to `lps-shared/src/sig.rs` and propagate through
   `lps-frontend` (set `Synthetic` for `__shader_init`). Standalone
   prep step; no behavioural change. Lands first because subsequent
   phases (synthesis, instance trait) want to mark new functions as
   `Synthetic` from the moment they exist.

2. **LPVM trait extension (Q8, Q12, Q13).** Add `call_render_texture`
   to `LpvmInstance` in `lp-shader/lpvm/src/instance.rs`. Implement on
   all six backends: `lpvm-cranelift`, `lpvm-native::rt_jit`,
   `lpvm-native::rt_emu`, `lpvm-emu`, `lpvm-wasm::rt_wasmtime`,
   `lpvm-wasm::rt_browser`. Each impl owns its own internal cache.
   Cargo build is the conformance check; per-backend correctness lands
   in phase 5. JIT smoke test (Q10 #5) lands here as soon as
   lpvm-cranelift's impl compiles.

3. **Synthesis routine (Q4, Q6).** New module
   `lp-shader/lp-shader/src/synth/render_texture.rs` producing an
   `IrFunction` named `__render_texture_<format>` from
   `(LpsModuleSig, TextureStorageFormat, render_fn_id)`. Pure
   function; no LPVM dependency. Marks the synthesised function with
   `LpsFnKind::Synthetic` (Q11). Unit test on the resulting LPIR
   shape (sanity / Q7 inliner-presence assertion).

4. **`LpsPxShader` refactor (Q5).** Drop `<M>` generic, introduce
   `Box<dyn PxShaderBackend>`. `LpsPxShader::new` validates the
   synthesised render function's presence + signature in `meta()`
   (warmup check moved out of the trait per Q8). `render_frame` wires
   uniforms + delegates to `PxShaderBackend::call_render_texture`,
   which forwards into `LpvmInstance::call_render_texture`.

5. **End-to-end pixel tests (Q10).** Four format-correctness tests on
   **`lpvm-wasm` / Wasmtime** (default `lp-shader` test engine): R16
   constant, Rgb16 constant, Rgba16 constant, Rgba16 gradient. The
   Phase 2 `lpvm-cranelift` handwritten-LPIR JIT smoke stays.
   `render_frame_sets_uniforms` continues to cover uniforms. RV32
   runtime validation is deferred to `fw-emu`-based integration tests
   under `lpfx`/`lp-engine` (M4); `lpvm-wasm` runtime validation
   awaits a future `fw-wasm` harness.

6. **Cleanup.** Verify `LpsPxShader::meta()` exposes the full sig
   (per Q11 — no filtering); spot-check that consumers needing
   user-only functions can filter via
   `kind == LpsFnKind::UserDefined`. Remove the stale "Migrate
   consumers" reference if any survive in the roadmap, sig docs, or
   inline comments. Workspace `cargo build --all-features` + targeted
   `cargo test -p` runs.
