# Phase 1 — LPIR core: types, module, builder

## Scope of phase

Introduce `ImportId`, `FuncId`, and `CalleeRef` enum in `lpir`. Replace `LpirModule::functions: Vec<IrFunction>` with `BTreeMap<FuncId, IrFunction>`. Extend `ModuleBuilder` with monotonic `FuncId` allocation (`add_function`). Update `callee_*` helpers to the new model. Print/parse/validate/interp are **phase 2**; it is OK if **`lpir` does not compile** until those files are updated—no requirement to stub just to keep the build green mid-plan.

## Code organization reminders

- One concept per file where it already exists (`types.rs`, `lpir_module.rs`, `builder.rs`).
- Entry points: public types and `LpirModule` / `ModuleBuilder` APIs first.
- Helper constructors (`CalleeRef::import`, `local`) at bottom if useful.

## Implementation details

- **`FuncId` / `ImportId`:** `#[repr(transparent)]` `u16` (or plain newtype); implement `Debug`, `Display`, `Ord`, `FromStr` not needed for ids.
- **`CalleeRef`:** `Import(ImportId)` | `Local(FuncId)`; derive `Copy`, `Eq`, `Hash`.
- **`LpirModule`:** `functions: BTreeMap<FuncId, IrFunction>`; remove flat `CalleeRef` index helpers; add:
  - `fn local_function(&self, id: FuncId) -> Option<&IrFunction>`
  - iterators as needed for phase 2 (`functions.values()`, `functions.iter()`).
- **`function_count`:** `self.functions.len()` as `u32`.
- **`ModuleBuilder`:** field `next_func_id: u32` (or u16 with overflow check); `add_function`: `let id = FuncId(...); self.functions.insert(id, func);` return `CalleeRef::Local(id)`.
- **`lib.rs`:** `pub use types::{ImportId, FuncId, CalleeRef, ...}`.
## Tests to write

- (Defer to phase 2 if core lands first without a compiling `lpir` crate.) Unit tests on builder: two `add_function` calls receive distinct `FuncId`s and both appear in the finished module’s map.

## Validate

Optional until the crate compiles again (usually after phase 2):

```bash
cargo test -p lpir
```
