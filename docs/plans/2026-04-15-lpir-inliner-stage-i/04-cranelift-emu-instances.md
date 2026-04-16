# Phase 4 — lpvm-cranelift, lpvm-emu, remaining instances

## Scope of phase

Update `lpvm-cranelift` (`module_lower.rs`, `emit/call.rs`, `call.rs`, `lpvm_instance.rs`) and `lpvm-emu` / any remaining `ir.functions[usize]` paths. Disambiguate **`cranelift_module::FuncId`** vs **`lpir::FuncId`** using imports (`use lpir::FuncId as LpirFuncId` or fully qualified paths).

## Code organization reminders

- `LpirFuncEmitOrder::Source` today means vec order; redefine as **sorted `FuncId` order** (matches monotonic assignment) or explicit vec of ids—**document in code comment** so JIT/object order stays deterministic.

## Implementation details

- **`module_lower.rs`:** `indices: Vec<usize>` becomes `Vec<FuncId>` or `Vec<(FuncId, usize)>`; `ir.functions[i]` → `ir.functions.get(&id)`; `id_at_ir` keyed by something stable—may become `BTreeMap<LpirFuncId, cranelift_module::FuncId>` or vec indexed by emit order with parallel `LpirFuncId` list.
- **`emit/call.rs`:** local callee index → `FuncId` + map lookup.
- **`lpvm-emu` / instances:** same patterns as `rt_emu` (phase 3); ensure name→IR lookup still works.

## Tests to write

```bash
cargo test -p lpvm-cranelift
cargo test -p lpvm-emu
```

## Validate

When applicable:

```bash
cargo test -p lpvm-cranelift
cargo test -p lpvm-emu
cargo test -p lpvm
```
