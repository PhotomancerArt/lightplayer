# Globals & Uniforms — Overview

## Motivation / Rationale

LightPlayer shaders currently have no module-scope state — every function is
pure, taking all inputs as parameters and returning outputs. Real GLSL shaders
use `uniform` variables (host-set per-frame values like time, resolution) and
global variables (per-shader mutable state initialized once, reset per pixel).
Without these, shaders can't maintain state across helper functions, use
standard GLSL patterns, or receive host parameters through the normal GLSL
uniform mechanism.

The existing infrastructure is surprisingly ready: `VmContext` already reserves
space after the header and has stub accessors, `LpsType`/`LayoutRules`/
`LpvmDataQ32` provide full std430 layout computation and typed byte-backed
data, and the frontend parses globals via Naga — it just doesn't lower them to
LPIR yet.

## Architecture / Design

### VMContext Memory Layout

Single contiguous allocation per instance:

```
┌─────────────────────┐  offset 0
│   VmContext header   │  fuel, trap_handler, metadata
│   (16 bytes on RV32) │
├─────────────────────┤  header_size
│   Uniforms region    │  host-writable, shader read-only
│   (std430 layout)    │
├─────────────────────┤  header_size + uniforms_size
│   Globals region     │  shader read-write (init code + render)
│   (std430 layout)    │
├─────────────────────┤  header_size + uniforms_size + globals_size
│   Globals snapshot   │  copy of globals after __shader_init
│   (same size)        │  memcpy'd back before each pixel
└─────────────────────┘
total = header_size + uniforms_size + 2 * globals_size
```

### Data Flow Per Frame

```
Host sets uniforms ──► call __shader_init() ──► memcpy globals → snapshot
                                                       │
         ┌─────────────────────────────────────────────┘
         ▼
    for each pixel:
         memcpy snapshot → globals  (skip if globals_size == 0)
         call render(fragCoord, outputSize, time) → rgba
         write pixel to texture
```

### LPIR Representation

No new LPIR ops. The frontend computes byte offsets during lowering and emits
plain `Load { base: VMCTX_VREG, offset }` / `Store { base: VMCTX_VREG, offset,
value }`. The module carries a globals/uniforms metadata table (types,
qualifiers, offsets, sizes) so instances know how much to allocate and where
each variable lives.

### Init Function

The frontend synthesizes a `__shader_init()` function that evaluates all global
initializers (may read uniforms, call helper functions). It's a regular compiled
LPIR function — goes through the same pipeline as user code.

### Render Paths

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

### Layout Computation (Existing)

Already implemented in the codebase:
- `lps-shared/src/layout.rs`: `type_size()`, `type_alignment()`, `array_stride()`
- `lpvm/src/lpvm_data_q32.rs`: `LpvmDataQ32` typed byte-backed data with path access
- `lps-shared/src/path_resolve.rs`: `offset_for_path()`, `type_at_path()`

## Alternatives Considered

- **Dedicated LPIR ops** (`GlobalLoad`, `GlobalStore`, `UniformLoad`): rejected
  because existing `Load`/`Store` with vmctx base + offset already work and
  every backend handles them. Adding ops means every backend needs new lowering.

- **Separate uniform/global allocations**: rejected in favor of single
  contiguous buffer — simpler addressing, one allocation, cache-friendly,
  snapshot is adjacent.

- **Runtime interpreter for initializers**: rejected in favor of
  `__shader_init()` function — goes through the same compile pipeline, handles
  all cases including uniform-dependent initializers.

## Risks

- **Naga `GlobalVariable` edge cases**: Naga represents `shared`, `buffer`,
  `in`, `out` as different address spaces. Scoping initial work to `uniform` +
  private globals de-risks this. Other address spaces can be added later.

- **Q32 fixed-point uniforms**: the host needs to know whether to write Q32 or
  f32 to the uniforms region. This is already a property of `FloatMode` on the
  module — not a new problem.

- **Per-pixel memcpy cost**: for `globals_size > 0`, every pixel pays a memcpy.
  For typical shaders (a few scalars/vectors) this is tens of bytes — negligible.
  The optimization for `globals_size == 0` eliminates it entirely for pure
  shaders.
