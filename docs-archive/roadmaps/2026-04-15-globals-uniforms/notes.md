# Globals & Uniforms — Roadmap Notes

## Scope

Add module-scope globals and uniforms to the LPIR → codegen → instance
lifecycle. Shaders can declare `uniform` variables (set by the host before
execution) and plain global variables (initialized once, reset per pixel).

The render model is:

1. Host sets uniform values on the instance.
2. Instance runs global init code (evaluates initializers, may read uniforms).
3. Instance snapshots the initialized globals buffer.
4. For each pixel: memcpy snapshot → globals, then call `render`.
5. Optimize: skip step 3–4 memcpy when shader has no mutable globals.

Extra credit: JIT-compile a texture-specific tight loop that inlines the shader
and writes directly to texture memory (out fragColor pattern).

## Current State

### What exists

- **VmContext** (`lpvm/src/vmcontext.rs`): 16-byte header (fuel, trap_handler,
  metadata). Has `globals_base()` / `globals_base_mut()` pointing after the
  header — but nothing is allocated there. `get_global`, `set_global`,
  `get_uniform` are `unimplemented!("Milestone 2")`.

- **LpirModule**: only `imports: Vec<ImportDecl>` and
  `functions: Vec<IrFunction>`. No global/uniform table.

- **LpsModuleSig**: only `functions: Vec<LpsFnSig>`. No global/uniform info.

- **LpirOp**: has `Load`, `Store`, `SlotAddr`, `Memcpy` — generic memory ops.
  No global-specific ops. `SlotAddr` is function-local only.

- **lps-frontend lowering**: walks only named user functions. Does not traverse
  `naga::Module::global_variables`. `Expression::Load` from a GlobalVariable
  hits `"Load from non-local pointer"` error. Statement `Store` to a global
  hits `"store to non-local pointer"`.

- **LpsType / LayoutRules**: `LpsType` covers all GLSL types. `LayoutRules`
  has `Std430` (implemented) and `Std140` (reserved).

- **Layout computation** (`lps-shared/src/layout.rs`): `type_size()`,
  `type_alignment()`, `array_stride()`, `round_up()` — fully implemented
  for std430.

- **LpvmDataQ32** (`lpvm/src/lpvm_data_q32.rs`): typed byte-backed data
  buffer using the layout functions. Supports path-based `get`/`set`,
  `to_value`/`from_value` marshaling, `offset_of()`. Already has the
  read/write logic for all LpsType variants to/from raw bytes.

- **Path resolution** (`lps-shared/src/path_resolve.rs`): `offset_for_path()`,
  `type_at_path()` for navigating struct/array paths to byte offsets.

- **NativeJitInstance**: holds `module` + `vmctx_guest: u32`. The vmctx
  allocation is just the header — no space for globals/uniforms.

- **Render loop** (`lp-engine/src/gfx/native_jit.rs`): nested x/y loop,
  `call_direct` per pixel with args `[frag_coord, output_size, time]`.
  No init, no reset, no uniform setting.

- **Filetests**: 39 files under `filetests/global/`, all marked
  `@unimplemented` on all backends.

### What doesn't exist

- No LPIR representation for module-scope storage.
- No frontend lowering of Naga `GlobalVariable` / `AddressSpace`.
- (Layout computation and typed data buffers already exist — see above.)
- No vmctx allocation with space after the header.
- No init function synthesis.
- No per-frame/per-pixel global reset mechanism.
- No uniform setter API on instance.
- No engine integration for the init/reset lifecycle.

## Questions

### Q1: How should globals and uniforms be represented in LPIR?

**Context**: Currently LPIR has no module-level storage. We need to represent
both uniform reads and global reads/writes. Two approaches:

**Option A — Dedicated ops**: Add `GlobalLoad { dst, global_index, offset }` and
`GlobalStore { global_index, offset, value }` ops, plus a `UniformLoad` variant.
The module carries a `globals: Vec<GlobalDecl>` table with type/qualifier info.

**Option B — Base pointer + Load/Store**: Emit `GlobalBase { dst }` to get the
vmctx globals pointer, then use existing `Load`/`Store` with computed offsets.
Module still carries a globals table for layout info, but codegen just sees
pointer arithmetic.

**Suggested answer**: Option B. It reuses existing memory ops, keeps LPIR
simple, and matches how the data physically lives (flat buffer at vmctx base +
offset). The globals table in `LpirModule` provides metadata for layout but
doesn't need dedicated IR ops. A single new op (`GlobalBase`) or even just a
convention (use vmctx vreg + known offset) is sufficient.

**Answer**: Neither — no new LPIR ops at all. The vmctx vreg (VReg(0)) is
already available in every function. The frontend computes byte offsets during
lowering (header_size + field_offset) and emits plain `Load { dst, base:
VMCTX_VREG, offset }` / `Store { base: VMCTX_VREG, offset, value }`. Every
backend already handles Load/Store. The only new thing in LPIR is module-level
metadata (globals table with types, qualifiers, offsets, total size) so the
instance knows how much space to allocate after the vmctx header.

### Q2: Where does layout computation live?

**Context**: We need to compute byte offsets for each global/uniform given its
type and packing rules. `lps-shared` already has `LpsType` and `LayoutRules`.

**Option A**: Add `size_of(ty, rules)` and `align_of(ty, rules)` to
`lps-shared`. The frontend computes offsets during lowering and embeds them
in the LPIR globals table.

**Option B**: Have `lpvm` do the layout at instantiation time.

**Suggested answer**: Option A. The frontend knows the Naga types and can map
them to offsets. Embedding offsets in the LPIR module means codegen doesn't
need to recompute layout. `lps-shared` is the natural place for the layout
functions since it owns `LpsType`.

**Answer**: Already done. `lps-shared/src/layout.rs` has `type_size()`,
`type_alignment()`, etc. `lpvm/src/lpvm_data_q32.rs` has `LpvmDataQ32` for
typed byte-backed data with path access. The frontend uses these during
lowering to compute offsets and embeds them directly into Load/Store ops.

### Q3: Uniforms and globals — same region or separate?

**Context**: The user specified that global init code can read uniforms, so
uniforms must be available before globals are initialized. Two layouts:

**Option A — Single region**: `[VmContext header | uniforms | globals]`.
Set uniforms first (host writes to known offsets), then run init code that
can read them via the same base pointer.

**Option B — Separate regions**: Two base pointers (e.g. two fields in
VmContext header, or two virtual addresses). More complex but cleaner
separation.

**Suggested answer**: Option A with an extra section.

**Answer**: Single contiguous allocation with three sections after the header:

```
[VmContext header | uniforms | globals | globals_snapshot]
```

- **Uniforms**: host-writable, shader read-only. Stable offsets after header.
- **Globals**: shader read-write. Initialized by `__lp_init()`.
- **Globals snapshot**: same size as globals. After init runs, memcpy globals →
  snapshot. Before each pixel, memcpy snapshot → globals to reset.

When `globals_size == 0`, steps 3 and 4 (snapshot/reset) are no-ops. The
snapshot lives in the same allocation — reset is a single memcpy within one
contiguous buffer, no pointer chasing.

### Q4: How is the init function synthesized?

**Context**: Global initializers like `float x = 42.0;` or
`vec2 v = vec2(uniform_a, 1.0);` need to run once before the pixel loop. Naga
already evaluates constant initializers into `global_expressions`, but some
initializers reference uniforms (runtime values).

**Option A — Synthetic `__lp_init` function**: The frontend generates an
`__lp_init()` function that evaluates all global initializers and stores them.
It's a regular LPIR function, compiled and called by the instance.

**Option B — Initializer metadata**: Store init values/expressions as data in
the module and have the runtime interpret them.

**Answer**: Option A. The frontend synthesizes a `__shader_init()` function in
LPIR that evaluates all global initializers and stores them to the globals
region via `Store { base: VMCTX_VREG, offset }`. It's a regular compiled
function — goes through the same pipeline, can reference uniforms via `Load`
from the uniforms region. Constant-only shaders get a trivial `__shader_init`
that's just a sequence of stores. Uniform-dependent initializers work naturally.
No new runtime interpreter needed.

Name is `__shader_init` (not `__lp_` which is reserved for compiler builtins).

### Q5: How does the instance lifecycle work for rendering?

**Context**: The render loop needs: set uniforms → init globals → snapshot →
per-pixel reset + call. This lifecycle lives on `LpvmInstance` or the engine's
shader wrapper.

**Option A — Instance methods**: `set_uniform(name/index, value)`,
`init_globals()`, per-pixel the instance does `reset_globals()` + call.
The snapshot is internal to the instance.

**Option B — Engine-level orchestration**: The engine render function manages
the lifecycle, calling into the instance for each step.

**Answer**: Two paths:

**Generic path** (`LpvmInstance` trait / filetests / tooling):
- `set_uniform(path, value)` writes to the uniforms region.
- `init_globals()` calls `__shader_init`, then memcpy globals → snapshot.
- `reset_globals()` memcpy snapshot → globals (no-op if `globals_size == 0`).
- Each `call`/`call_q32` invocation does reset + invoke.

**Fast path** (`LpShader::render` in `lp-engine`):
- The backend-specific render function (e.g. `render_native_jit_direct`) owns
  the full lifecycle internally: set uniforms → call `__shader_init` → snapshot
  → tight pixel loop (inline reset + `call_direct`).
- All behind the `LpShader` abstraction; `lp-engine` just calls
  `shader.render(texture, time)`.
- This is already the architecture today — the globals lifecycle folds into the
  existing backend-specific render function.
- Opens the door for future inlined tight loop optimization (extra credit).

### Q6: Filetest infrastructure — how do we test globals?

**Context**: Filetests call functions by name with explicit arguments. Globals
introduce implicit state. How do filetests set uniforms and verify global
behavior?

**Option A**: Extend filetest syntax: `// set_uniform: name = value` before
`// run:` lines.

**Option B**: Global init code runs automatically when instantiating. For
constant-initialized globals, no special syntax needed — just call the function
and check the result.

**Answer**: The filetest runner needs two things:

1. **`// set_uniform:` directive**: Sets a uniform value before the test runs.
   Syntax like `// set_uniform: time = 1.0` or `// set_uniform: resolution = vec2(800.0, 600.0)`.
   Applied per-test (or scoped to following `// run:` lines).

2. **Per-test init + reset**: For each `// run:` line, the runner does:
   - Apply any `// set_uniform:` values to the uniforms region.
   - Call `init_globals()` (runs `__shader_init`, snapshots globals).
   - Call the test function (which may read/write globals).
   - Globals are effectively fresh for each test expectation.

Constant-initialized globals (most of the existing 39 filetests) need no
`// set_uniform:` directives — just init + call. Uniform-dependent tests
use the directive to set values before init runs.

### Q7: Scope of "extra credit" inline tight loop — in or out of this roadmap?

**Context**: The user mentioned JIT-compiling a texture-specific function that
inlines the shader and writes directly to texture memory. This is a significant
optimization that touches codegen, texture format awareness, and the render
architecture.

**Answer**: Out of scope for this roadmap's implementation milestones. Documented
as the final `mN-future-work.md` milestone with design notes and prerequisites,
so we can jump off into a new roadmap when we get there. The globals/uniforms
infrastructure built here (vmctx layout, init/reset, `out` parameters) is the
foundation it needs.

## Notes

- Layout computation and `LpvmDataQ32` already exist — no new layout code needed.
- Use `LpsType::Struct` for both `uniforms_type` and `globals_type` on
  `LpsModuleSig` — reuses all existing layout/path infrastructure.
- `__shader_init` (not `__lp_` prefix, which is for compiler builtins).
- Filetest directory restructure: `global/`, `uniform/`, `global-future/`.
- M3 (filetest review) is done by orchestrating agent, not Kimi sub-agent.
- Execution approach: Opus orchestrates, Kimi sub-agents implement milestones.
  Stop for human review when creative input is needed.
