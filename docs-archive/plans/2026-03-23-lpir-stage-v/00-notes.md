# Stage V: LPIR → WASM Emission — Notes

## Current state

Stage IV is implemented: `lps-frontend::lower()` produces an `IrModule`
from a `NagaModule`. The LPIR is flat, scalarized, and float-mode-unaware.

The current WASM emitter (`lps-wasm/src/emit.rs`, ~1970 lines) walks
Naga IR directly. It handles:

- Float mode: native `f32.*` WASM instructions
- Q32 mode: inline i32 fixed-point arithmetic via i64 widening/saturation
- Limited math builtins: mix, smoothstep, step, round, abs, min, max
- LPFX: imports from `builtins` module, `env.memory` for out-pointers
- Vectors: component-wise lowering via `emit_vec.rs`

The new emitter will walk `IrModule` instead. Since LPIR is scalarized,
`emit_vec.rs` is unnecessary. Since LPIR uses flat ops, `locals.rs` is
trivial (VReg N → local N).

## Questions

### Q1: `@std.math::*` imports — how to handle in WASM?

The LPIR has `@std.math::sin`, `@std.math::cos`, etc. as imports. WASM
has no native sin/cos. The current WASM emitter doesn't support these
either (errors on unsupported MathFunction variants).

Options:

- A) Map to WASM host imports (`env::sin`, etc.). Host must provide.
- B) Map to `builtins` module imports (from lps-builtins-wasm).
- C) Error for now (matches current emitter limitation).
- D) Import from `builtins` module alongside LPFX — the builtins WASM
  already contains Q32 math implementations.

**Answer:** D — Q32 mode maps `@std.math::sin` → `builtins::__lp_q32_sin`
etc. The builtins WASM module already has full Q32 math coverage. Both
std.math and LPFX imports go through the same `builtins` module.

Float mode: not required for this stage. Could use `libm` crate later
for native float support. For now, Q32-only; float mode errors.

### Q2: LPFX import resolution

LPIR has `@lpfn::lpfn_hash1(f32, f32) -> f32` etc. The WASM emitter
needs to create `builtins` module imports with the correct Q32 WASM
signature. Currently, `lps-wasm/src/lpfn.rs` resolves via
`lps-builtin-ids::BuiltinId` → `q32_lpfn_wasm_signature`.

For the new emitter, the LPIR import carries the *logical* signature
(generic, float-mode-unaware). The emitter needs to:

- Detect `@lpfn::*` imports by module name
- Resolve the LPFX name + param types → BuiltinId
- Get the Q32 WASM signature from the BuiltinId
- Emit the appropriate WASM import + call

Should the BuiltinId resolution happen in the emitter, or should LPIR
carry a BuiltinId directly?

Options:

- A) Emitter resolves by name/signature (keeps LPIR backend-agnostic).
- B) LPIR ImportDecl carries an optional BuiltinId hint.

**Answer:** A — emitter resolves by name. Keeps LPIR backend-agnostic.
The emitter already depends on `lps-builtin-ids`.

### Q3: Shadow stack for slots

LPIR functions may have slots (for LPFX out-pointers, arrays, etc.).
These need WASM linear memory. The roadmap suggests a `$sp` global.

Options:

- A) Mutable WASM global `$sp`. Prologue: `$sp -= frame_size`. Epilogue:
  `$sp += frame_size`. Only emitted for functions with slots.
- B) Fixed scratch base (like current `LPFX_SCRATCH_BASE = 65536`).

**Answer:** A — mutable `$sp` global. Prologue/epilogue only for
functions with slots. Clean and standard.

### Q4: Old emitter — keep or delete?

The current Naga-direct emitter works and all filetests pass against it.
The new LPIR-based emitter replaces it.

Options:

- A) Delete old code immediately in this stage.
- B) Keep old path behind a feature flag for comparison.
- C) Delete old code in Stage VI after filetests are confirmed passing.

**Answer:** A — delete now. It's a dead end and it's in git. Keep things
clean.

### Q5: File organization

The current emitter is one large `emit.rs` (1970 lines). User preference
is small, targeted modules in directory modules.

Proposed structure:

```
lps-wasm/src/
  emit/
    mod.rs          # entry: emit_module()
    func.rs         # per-function: locals, prologue/epilogue
    ops.rs          # non-control Op → WASM instruction(s)
    q32.rs          # Q32 inline expansion (add_sat, mul, div, etc.)
    control.rs      # structured control flow (if, loop, switch)
    memory.rs       # slot_addr, load, store, memcpy, shadow stack
    imports.rs      # import resolution (std.math, lpfn)
  lib.rs
  module.rs
  options.rs
```

Deletions: `emit.rs`, `emit_vec.rs`, `locals.rs`, `lpfn.rs`, `types.rs`.

**Answer:** Agreed. Small targeted modules in a directory module.
