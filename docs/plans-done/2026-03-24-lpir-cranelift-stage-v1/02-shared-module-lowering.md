## Scope of phase

**Agreed approach:** Refactor **`jit_module.rs`** so the declare/define loop for
LPIR functions is shared between **`JITModule`** and (next phase)
**`ObjectModule`** via a generic `M: Module` helper — not a second hand-copied
lowering path.

Refactor **`jit_module.rs`** accordingly.

Introduce an internal API along the lines of:

```rust
fn define_lpir_functions<M: Module>(
    module: &mut M,
    ir: &IrModule,
    options: CompileOptions,
    // import ids, lpir builtin ids, call_conv, pointer_type — plumbed in
) -> Result<DefinedFuncs, CompilerError>
```

where `DefinedFuncs` holds `func_ids`, `func_names`, `signatures`, etc., as
needed by `JitModule` and optionally by the object path.

## Code organization reminders

- Keep `JitModule` public API unchanged unless a small additive change is
  unavoidable.
- Helpers specific to this refactor live at the **bottom** of the module or in a
  small `module_lower.rs` if `jit_module.rs` grows too large.

## Implementation details

- **Imports:** `builtins::declare_module_imports` and
  `builtins::declare_lpir_opcode_builtins` already take `&mut impl Module` — reuse
  as-is.
- **Per-function loop:** identical for JIT and object: `declare_function` all,
  then `define_function` each with `translate_function` + `EmitCtx`.
- **JIT-only tail:** `finalize_definitions()`, wrap `JITModule` in `JitModule`.
- **Object-only tail** (phase 03): `finish` on `ObjectModule` — not in this phase.

Preserve **sorted declaration order** if the object path will require stable
symbol ordering (match `lps-cranelift` `emu.rs` sort-by-name if Cranelift
object mapping is order-sensitive).

## Tests

- Run **all existing** `lpir-cranelift` unit tests unchanged — behavior must be
  identical for JIT.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lps && cargo test -p lpir-cranelift
```

`cargo +nightly fmt`.
