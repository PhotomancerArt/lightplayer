# VMContext, Globals, and Uniforms — Roadmap Notes

## Scope

Enable GLSL shaders to access `uniform` and `global` variables by introducing a `VMContext` struct
that is passed as a hidden first parameter to all shader functions. This context contains pointers
to uniform buffers and global storage.

Target backends: RISC-V32 (embedded) and WebAssembly (browser/emu).

## Current State

- **lpvm**: Recently created with `GlslType`, layout computation (`std430`), and path resolution.
  Can describe struct types and compute byte offsets.
- **lpir**: Pure IR without GLSL metadata. Functions have explicit parameters only.
- **lpvm-cranelift**: JIT compilation for RISC-V32 and host ISAs. Uses `DirectCall` and `invoke` for
  calling shaders. Signatures are built from `IrFunction` param/return counts.
- **lps-wasm**: WASM emission with shadow stack (`$sp` global). Functions exported directly; no
  context parameter currently.
- **lps-frontend**: Lowers GLSL to LPIR, but does not yet collect uniform/global metadata or emit
  context-aware code.

## Questions

### Q1: VMContext structure layout

**Context**: We need a stable layout for `VMContext` that both backends can agree on. It needs:

- Pointer to uniforms struct (read-only)
- Pointer to globals struct (read-write)
- Optional: fuel counter, trap handler pointer

**Suggested approach**: Define a `VmContext` GlslType in lpvm with `#[repr(C)]` layout guarantees.
Generate offset constants for each field that backends can use.

**Question**: Should we version the VMContext struct for future compatibility? If so, how (version
field at offset 0, or separate ABI version global)?

**Answer**: No version field needed. We're JIT-only with tight coupling between generator and
harness. VMContext is a single flat struct created dynamically at compile time:

- Well-known fields (fuel, trap_handler) at fixed offsets (0, 8, etc.)
- Shader-specific uniforms and globals laid out after the header
- Host can access well-known fields via a Rust struct (unsafe cast), shader accesses everything via
  offsets
- Single contiguous allocation for cache efficiency

### Q2: Function signature changes

**Context**: Every shader function needs to accept VMContext as arg 0. This affects:

- LPIR function signatures (add one implicit param)
- Cranelift signature generation (add `pointer_type` param)
- WASM function signatures (add `i32` param)
- `DirectCall` and `invoke` APIs (caller provides context pointer)

**Suggested approach**: Make VMContext an explicit first parameter in all lowered code. The LPIR
`IrFunction.param_count` stays as the *user-visible* param count; backends add +1 for the context.

**Question**: Should LPIR represent VMContext explicitly (new opcodes) or stay agnostic (backends
just add the param)?

**Answer**: LPIR stays agnostic. No new opcodes needed. Lowering produces regular `Op::Load`/
`Op::Store` with addresses computed as `vmctx + offset`. The first vreg in LPIR functions is
implicitly the VMContext pointer; backends treat it as native pointer type (`pointer_type` in
Cranelift, `i32` in WASM).

### Q3: Uniform binding model

**Context**: GLSL uniforms have bindings (`layout(binding = 0) uniform Foo { ... }`). We need to map
binding numbers to offsets within the uniforms struct.

**Suggested approach**: The `VmContext.uniforms` field points to a struct where each binding is a
member. The member name can be `binding_0`, `binding_1`, etc., or we use a special naming
convention.

**Question**: How do we handle uniforms without explicit bindings? GLSL allows this, but it's
implementation-defined. Should we require explicit bindings in LightPlayer?

**Answer**: No explicit bindings required. Uniforms are accessed by name. The name→offset mapping is
stored in GlslType metadata. Host looks up offset by name (can be cached after first access). This
produces cleaner GLSL code. Slower than binding-based access but acceptable for our use case. Noted
as a known design choice / limitation.

**Design doc needed**: `docs/design/uniforms-globals.md` to capture this decision (part of Milestone
I).

### Q4: Global initialization and _init()

**Context**: GLSL globals need initialization (e.g., `int counter = 0;`). Some may be complex
expressions requiring shader code to run. We may need an `_init()` function per shader.

**Suggested approach**: Naga's constant evaluation handles simple cases. For complex initialization,
emit an `_init(vmctx)` function that the host calls once before first use. Store init-done flag in
VMContext.

**Question**: Should we support re-initialization (reset globals to initial values)? If so, is that
a separate `_reset()` function or does `_init()` handle it?

**Answer**: Optimization approach: Assume globals are based only on uniforms/constant data. Run
`_init()` once per shader to produce "default" global values, store them. On each invocation, if
there are mutable globals, `memcpy` the defaults over current globals (fast reset). Avoids
re-running shader code for initialization on every frame—critical for performance at the edge of
compute limits.

### Q5: Readonly vs mutable globals

**Context**: GLSL has `const` (compile-time) and `uniform` (read-only runtime). Global variables are
mutable. Do we need to distinguish these at the VMContext level?

**Suggested approach**: All runtime data lives in VMContext. `const` is compile-time only (handled
by Naga). `uniform` accesses go through the uniforms pointer; global accesses go through globals
pointer.

**Question**: Do we want memory protection (e.g., mark uniforms page as read-only)? Probably not for
MVP, but worth noting.

### Q6: WASM specific concerns

**Context**: WASM has linear memory. VMContext will be a pointer into that memory. We need:

- Import `env.vmctx` as i32 global (set by host before each call)
- Or: Pass as explicit i32 param (simpler, matches RISC-V approach)

**Suggested approach**: Explicit i32 param for WASM too—keeps both backends consistent. The WASM
host (JS or test harness) allocates VMContext in WASM memory and passes the pointer.

**Question**: For the test harness, do we need new APIs to allocate VMContext and set uniform
values? How does this integrate with existing filetest infrastructure?

### Q7: Milestone boundaries

**User suggested milestones**:

1. Empty VMContext placeholder — get threading working
2. Collect globals and uniforms — basic handling, maybe readonly
3. Add _init() and mutability, resetting globals
4. Anything else?

**Suggested additions**:

- Milestone 0: Design docs and type definitions (VmContext struct, layout)
- Milestone 4: Test infrastructure for VMContext-aware execution
- Milestone 5: Validation and cleanup (filetests, wasm, rv32)

**Question**: Does this sequencing make sense? Should we combine any milestones?

**Answer**: Refinement accepted:

- **Milestone I**: Design docs + empty VMContext
- **Milestone II**: Uniforms + readonly globals
- **Milestone III**: Mutability + _init() + reset
- **Milestone IV**: Validation + cleanup

## Notes

- User prefers explicit VMContext parameter over reserved register/global approach (see discussion).
  This keeps LPIR simpler and allows direct Rust interop.
- The lpvm crate is the right place for VmContext type definitions and layout constants.
- Need to ensure `fw-esp32` and `fw-emu` continue to compile with the new ABI (embedded builds).
- **Global defaults placement**: Global defaults are part of the VMContext dynamic type (not a
  separate buffer). Layout: header → uniforms → globals (mutable) → globals_defaults (source for
  reset). The `globals_defaults_offset` is stored in the well-known header (at a fixed offset),
  making the memcpy reset fast without requiring separate metadata lookups.
