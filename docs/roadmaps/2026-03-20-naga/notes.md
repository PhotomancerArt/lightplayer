# Naga Migration — Notes

## Scope

Replace the custom GLSL frontend (`lp-glsl-frontend` + `glsl-parser` fork) with
Naga's GLSL frontend, and rewrite backends to consume `naga::Module` instead of
`TypedShader`.

WASM backend first (web demo), Cranelift backend later (ESP32 / host JIT).

## Current state

### Dependency graph (compiler path)

```
fw-esp32 → lp-engine → lp-glsl-cranelift → lp-glsl-frontend → glsl-parser (fork)
                                          → lp-glsl-builtin-ids
web-demo → lp-glsl-wasm → lp-glsl-frontend → glsl-parser (fork)
                         → lp-glsl-builtin-ids
lp-glsl-filetests → lp-glsl-cranelift (cranelift.q32 target)
                   → lp-glsl-wasm (wasm.q32 target)
```

### Key interfaces

- **lp-glsl-frontend** produces `TypedShader` (contains `TypedFunction`s,
  `FunctionRegistry`, `global_constants`). Both backends consume this.
- **lp-glsl-cranelift** exposes `glsl_jit_streaming(&str, GlslOptions) →
  Box<dyn GlslExecutable>`. Called by lp-engine.
- **lp-glsl-wasm** exposes `glsl_wasm(&str, WasmOptions) → WasmModule`.
  Called by web-demo.
- **lp-glsl-filetests** tests both backends via `compile_for_target()`.
  Default targets: `cranelift.q32` and `wasm.q32`.
- **Builtins** identified by `BuiltinId` enum (generated). Frontend resolves
  by name; backends dispatch as inline code or imports.

### Spike results (spikes/naga-wasm-poc)

- Naga v29 `glsl-in` compiles under `#![no_std]` — no fork needed.
- GLSL → Naga IR → WASM → wasmtime execution works for f32 and Q32 modes.
- Naga lowers `in` params to LocalVariable + Store; handled by a mapping pass.
- Expression arena length gives local count upfront (scratch pool problem solved).

### Size concern

- Naga rlib (all features): 13M vs lp-glsl-frontend + glsl-parser: 5.1M.
- With only `glsl-in` + LTO + dead code elimination, real delta is unknown.
- Compiler runs on ESP32 at runtime (JIT). ROM impact must be measured
  empirically after a minimal integration.

## Questions

### Q1: New frontend crate or modify existing?

Should we create a new `lp-glsl-naga` crate that wraps Naga, or modify
`lp-glsl-frontend` in place?

**Context**: Both backends currently depend on `lp-glsl-frontend`. During
migration we need both paths working (old Cranelift backend on old frontend,
new WASM backend on Naga). After migration, the old frontend can be deleted.

**Suggestion**: New crate `lp-glsl-naga`. It wraps `naga::front::glsl` and
exposes a compilation result containing `naga::Module` plus LP-specific
metadata (float mode, builtin mappings). Old frontend stays untouched until
Cranelift is ported. Clean separation, no risk of breaking the working system.

**Answer**: New crate `lp-glsl-naga`. Clean break from old frontend. Copy
useful code from old frontend as needed. Old frontend stays untouched until
Cranelift is ported.

### Q2: New WASM backend crate or modify lp-glsl-wasm in place?

**Context**: `lp-glsl-wasm` currently depends on `lp-glsl-frontend`. Rewriting
it to use Naga means changing its fundamental input type. We could create a new
crate or rewrite in place.

**Answer**: Rewrite `lp-glsl-wasm` in place. It's not in production use yet,
has known bugs (scratch overflow, local.tee), and the whole point is to replace
it. Switch dependency from `lp-glsl-frontend` to `lp-glsl-naga`.

### Q3: Filetest strategy during migration?

**Context**: Filetests currently run both `cranelift.q32` and `wasm.q32`
targets. During migration, the WASM target will be rewritten but the Cranelift
target stays on the old frontend.

**Answer**: Keep both targets active. `wasm.q32` switches to the new
Naga-based stack. `cranelift.q32` stays on the old frontend. Cross-validation:
both must agree on expected outputs. Filetests will depend on both frontends
during migration (extra compile time, not a correctness issue).

### Q4: Builtin strategy with Naga?

**Context**: Naga already knows standard GLSL builtins (sin, cos, etc.) as
`Expression::Math { fun: MathFunction::Sin, .. }`. Custom `lpfx_*` functions
are unknown to Naga.

**Answer**: Two-part approach:
- **Standard GLSL builtins**: Naga parses these into `MathFunction` variants.
  Backend maps `MathFunction` → inline WASM or `BuiltinId` import.
- **LPFX builtins**: Prepend forward declarations (prototypes only, no bodies)
  like `float lpfx_snoise(vec3 p);` before user source. Follow with `#line 1`
  to reset line numbering so user error messages stay correct. Backend
  recognizes calls by name and emits `BuiltinId` imports.

### Q5: What GLSL subset must the WASM backend support for the web demo?

**Answer**: Target `rainbow.glsl` as definition of done. It exercises scalars,
vectors (vec2/vec3/vec4), swizzles, standard builtins (smoothstep, mix, sin,
cos, atan), lpfx calls, and control flow (for loops). Once rainbow.glsl
renders correctly in the web demo, Phase II is complete.
