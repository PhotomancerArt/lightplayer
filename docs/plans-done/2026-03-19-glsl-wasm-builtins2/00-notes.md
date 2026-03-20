# WASM Builtins: Path to Rainbow — Planning Notes

## Scope of work

Complete builtin function support in the `lp-glsl-wasm` backend so that `examples/basic/src/rainbow.shader/main.glsl` compiles in Q32 mode, links with `lp_glsl_builtins_wasm.wasm` + shared `env.memory`, and runs under wasmtime.

Predecessor: `docs/plans-done/2026-03-18-glsl-wasm-builtins/` — delivered `builtins.wasm`, Q32 import plumbing, inline gentype builtins, `q32_builtin_link` smoke test.

Remaining work:
1. **Math gaps** — `floor`, `fract` (Q32), `atan(y,x)` codegen paths
2. **LPFX call emission** — FunCall dispatch + flattened arg emission for `lpfx_worley`, `lpfx_fbm`, `lpfx_psrdnoise`
3. **Out parameter support** — `lpfx_psrdnoise`'s `out vec2 gradient` needs memory pointers + post-call loads
4. **Filetest runner** — `wasm_runner.rs` needs builtins.wasm + shared memory + linker
5. **Rainbow integration** — compile + run full shader, fix remaining gaps

## Current state

### Codegen (`lp-glsl-wasm`)

**Working:**
- AST pre-scan (`builtin_scan.rs`) collects `HashSet<BuiltinId>` for Q32 math and LPFX.
- Import section: `env.memory` (1 page, conditional on Q32 + builtins) + `builtins.<name>` per scanned BuiltinId.
- Function indices: imports `0..M-1`, user functions `M..`.
- Inline builtins (`builtin_inline.rs`): `abs`, `min`, `max`, `clamp`, `mix`, `step`, `sign`, `smoothstep`, `mod` — Float + Q32.
- Q32 math import calls (`builtin_call.rs`): `emit_q32_math_libcall` — scalar imports with component-wise vector expansion. Covers `sin`, `cos`, `exp`, `sqrt`, etc.
- `q32_builtin_link.rs` integration test: shared memory + wasmtime linker + `sin(1.0)` runs end-to-end.

**FunCall dispatch** (`expr/mod.rs`): constructors → user functions → inline builtins → Q32 math imports → **error**. No LPFX path — falls to error branch.

**Not working:**
- `fract` Q32 — only Float implemented. Rainbow uses it in `paletteRainbow`.
- `floor` — not in inline set. Rainbow uses it in `applyPalette`.
- LPFX calls — scan records BuiltinIds, imports section includes them, but codegen hits error.
- Out params / memory writes — no `memory.rs`, no offset allocation.
- Filetest runner — `Instance::new(&mut store, &module, &[])`, no imports.

### Cranelift reference implementations

- `floor` Q32: inline `sshr(a, 16)` then `ishl(result, 16)` — arithmetic right shift zeros fractional bits, left shift restores position. Simple, no import needed.
- `fract` Q32: `x - floor(x)` — per-component, uses inline `emit_float_floor` then `emit_float_sub`.
- LPFX calls: `find_lpfx_fn(name, &param_types)` → validate → flatten args (in params as components, out/inout as pointers) → call. Vector returns use sret pointer prepended as first param.
- `build_call_signature` builds Cranelift signature from GLSL params (in → expanded, out → pointer). Does **not** include hidden extern "C" params like `seed`.

### LPFX ABI observations

**psrdnoise2 Q32 extern "C" signature** (from `psrdnoise2_q32.rs`):
```
__lpfx_psrdnoise2_q32(x: i32, y: i32, period_x: i32, period_y: i32, alpha: i32, gradient_out: *mut i32, seed: u32) -> i32
```
7 params: 5 scalar values + 1 gradient pointer + 1 seed. Returns scalar.

**GLSL signature**: `float lpfx_psrdnoise(vec2 x, vec2 period, float alpha, out vec2 gradient)` — 4 GLSL params, no seed.

**Generated `wasm_import_val_types(LpfxPsrdnoise2Q32)`**: 7× i32 → 1× i32. Matches the extern "C" function including seed.

**Cranelift `build_call_signature`**: builds from GLSL params only (no seed) → 6 params. This means either:
- (a) Cranelift's builtin registry declares the function with the full extern "C" signature (7 params) and somehow injects the seed, or
- (b) There is a mismatch between the WASM import types (7 params, generated from extern "C") and what the Cranelift-style call actually passes (6 params).

This needs to be resolved before implementing psrdnoise codegen.

### Rainbow builtins inventory

| Builtin | Current status | Notes |
|---------|---------------|-------|
| `clamp`, `abs`, `mod`, `smoothstep`, `mix`, `min` | Inline ✓ | |
| `sin` | Import ✓ | Verified via `q32_builtin_link` |
| `cos`, `exp` | Import (untested) | Same path as `sin`, should work |
| `floor` | **Gap** | Cranelift inlines as shift/shift |
| `fract` | **Gap** | Q32 not implemented; `x - floor(x)` in Cranelift |
| `atan(y,x)` | **Gap** | 2-arg → `LpQ32Atan2`; should map via `glsl_q32_math_builtin_id` |
| `lpfx_worley` | **Gap** | No FunCall dispatch; scalar return, seed in GLSL args |
| `lpfx_fbm` | **Gap** | No FunCall dispatch; scalar return, seed in GLSL args |
| `lpfx_psrdnoise` | **Gap** | No FunCall dispatch; out param + hidden seed |

## Questions

### Q1: `floor` Q32 — inline or import?

**Context:** Cranelift inlines `floor` Q32 as `sshr(a, 16)` → `ishl(result, 16)`. This arithmetic right shift discards fractional bits. There is also a `BuiltinId` for floor (check: does one exist?). The inline approach avoids an import, matches Cranelift, and is simple.

**Suggested answer:** Inline in WASM as `i32.shr_s(a, 16)` → `i32.shl(result, 16)`. Add to `builtin_inline.rs` and `q32_builtin_import_suppressed`. No import needed.

**Decision:** Inline, matching Cranelift.

### Q2: psrdnoise seed param — mismatch between GLSL (no seed) and extern "C" (has seed)

**Context:** The generated `wasm_import_val_types(LpfxPsrdnoise2Q32)` includes 7 i32 params (matching the extern "C" ABI which includes `seed: u32`). But the GLSL signature has 4 params and no seed. When the WASM codegen emits `call` to the psrdnoise import, it needs 7 values on the stack. From the GLSL args: vec2 → 2, vec2 → 2, float → 1, out vec2 → 1 pointer = 6 values. The 7th (seed) must come from somewhere.

Other LPFX functions (worley, fbm) expose seed as a normal GLSL param (`0u` in rainbow.shader), so their GLSL arg count matches after flattening. psrdnoise is unique — seed is in the extern "C" but hidden from GLSL.

How does Cranelift handle this? `build_call_signature` builds from GLSL params only (6 params). Does the builtin registry declare the function with 7 params? If so, there's a 6 vs 7 mismatch in Cranelift too.

**Suggested answer:** Investigate whether Cranelift currently passes seed=0 or omits it. For WASM, the generated import types include seed. Options: (a) Codegen injects `i32.const 0` for the seed; (b) Regenerate import types without seed for psrdnoise; (c) Add seed to the GLSL signature (breaking change). Option (a) is simplest.

**Finding:** Cranelift's `signature_for_builtin` declares `LpfxPsrdnoise2Q32` with **6 params** (no seed). The actual extern "C" function has 7 (including `seed: u32`). Cranelift calls with 6 args; the 7th register has garbage. This "works" on native because the seed just produces a different-but-valid permutation — UB that happens to be harmless. All other LPFX functions (worley, fbm, etc.) already expose seed as a normal GLSL parameter.

**Decision:** Fix the bug properly — add `seed: UInt` to the `lpfx_psrdnoise` GLSL signature in `lpfx_fns.rs` (both vec2 and vec3 overloads). Update Cranelift registry to declare 7 params. Update all GLSL shader calls to pass `0u` as seed. The WASM generated import types (7 params) are already correct. This makes psrdnoise consistent with worley/fbm and eliminates the UB.

### Q3: LPFX codegen module organization

**Context:** `builtin_call.rs` handles Q32 math imports with component-wise expansion — a fundamentally different pattern from LPFX (pre-flattened, mixed types, out params, possibly hidden params). Putting LPFX logic in the same file would make it large and mix two distinct calling conventions.

**Suggested answer:** New `lpfx_call.rs` alongside `builtin_call.rs`. FunCall dispatch adds an `is_lpfx_fn` branch that calls into it.

**Decision:** Separate `lpfx_call.rs`, matching Cranelift's approach (which has `lpfx_fns.rs` + `lpfx_sig.rs` separate from the math builtins). Signature types are already handled by the generated `builtin_wasm_import_types.rs`, so we only need one new file for call emission + arg flattening + out-param handling.

### Q4: Memory allocation for out params

**Context:** Only psrdnoise needs memory writes (8 bytes for gradient vec2). Future LPFX or textures may need more. Options: (a) static fixed offsets (bytes 0–7 reserved), (b) per-function bump allocator, (c) per-call-site compile-time allocation. The memory import is already emitted.

**Suggested answer:** Static offsets. psrdnoise gradient is always at offset 0 (8 bytes). Simple, deterministic, no runtime allocator. Document the layout. Grow the reserved region later if more out params emerge.

**Decision:** Static offset 0. Gradient vec2 is always at bytes 0–7. Safe because: no reentrant LPFX calls, and values are copied to locals immediately after `call` before any subsequent call could overwrite. If future out-param builtins appear, they can reuse offset 0 with the same pattern.

### Q5: Filetest runner refactor strategy

**Context:** `wasm_runner.rs` uses `Instance::new(&mut store, &module, &[])`. `q32_builtin_link.rs` has the correct pattern (load builtins.wasm, create memory, linker). The linking code needs to be shared. Options: (a) extract to a helper module in `lp-glsl-wasm`, (b) extract to `lp-glsl-filetests`, (c) duplicate inline in both places.

**Suggested answer:** Helper in `lp-glsl-filetests` (e.g. `wasm_link.rs`) since both `wasm_runner.rs` and tests need it. `q32_builtin_link.rs` could also use it. The helper should handle: loading builtins.wasm, creating memory, instantiating builtins, building the linker.

**Decision:** Helper in `lp-glsl-filetests` (e.g. `wasm_link.rs`). No circular dependency — filetests depends on lp-glsl-wasm but not vice versa. `q32_builtin_link.rs` in lp-glsl-wasm/tests/ keeps its own inline version (small, self-contained).
