# Phase 5: Validator

## Scope

Implement well-formedness checks for `IrModule` and `IrFunction` per the rules
in `docs/lpir/07-text-format.md` (Well-formedness sections). The validator
should catch construction errors early and produce clear error messages. Both
positive tests (valid IR passes) and negative tests (malformed IR rejected with
the right error).

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation details

### 1. src/validate.rs — Public API

```rust
use alloc::string::String;
use alloc::vec::Vec;
use crate::module::*;
use crate::types::*;

#[derive(Debug)]
pub struct ValidationError {
    pub message: String,
    pub op_index: Option<usize>,   // index into body, if applicable
    pub func_name: Option<String>,
}

pub fn validate_module(module: &IrModule) -> Result<(), Vec<ValidationError>>;
pub fn validate_function(
    func: &IrFunction,
    module: &IrModule,
) -> Result<(), Vec<ValidationError>>;
```

The validator collects all errors (does not stop at the first) and returns them.

### 2. Module-level checks

1. **At most one `entry func`**: scan `functions`, count those with
   `is_entry == true`. Error if more than one.

2. **Unique function names**: no two functions share the same `name`. Error
   with both names/indices on duplicate.

3. **Unique import names**: no two imports share the same
   `(module_name, func_name)` pair.

4. **No name collision between imports and functions**: an import
   `@mod::name` and a local function `@name` are distinct (different
   namespaces due to `::` syntax), but two local functions cannot share a
   name.

5. **All call targets exist**: for every `Op::Call { callee, .. }` in every
   function, `callee` must be a valid index into the combined callee table
   (imports then functions).

### 3. Function-level checks

Walk the flat op stream. Maintain state:
- `defined: Vec<bool>` — which VRegs have been defined so far (parameters
  start as defined).
- `vreg_types: &[IrType]` — reference to the function's type array.
- `block_stack: Vec<BlockKind>` — tracks nesting (`If`, `Loop`, `Switch`,
  `Case`, `Default`).
- `loop_depth: usize` — counts enclosing loops.

#### VReg checks

1. **Defined before use**: every VReg operand on the right-hand side of an op
   must already be defined. Mark VRegs as defined when they appear as `dst`
   of an assignment op, or as function parameters.

   Note: non-SSA reassignment is allowed, so a VReg can be defined more
   than once. The check is that it's defined *at least once* on some path
   before use. For Stage II, a simple linear scan (mark on define, check on
   use) is sufficient — it doesn't do path-sensitive analysis, which is fine
   since the spec says "defined earlier in the function (by parameter list,
   or by a prior defining assignment on some control-flow path)." A linear
   scan catches the common case; path-sensitive analysis is future work.

2. **Type consistency**: if a VReg is redefined, the new type must match the
   original type in `vreg_types`.

3. **VReg index in range**: all VReg indices must be `< vreg_types.len()`.

#### Control flow checks

4. **Proper nesting**: `IfStart`/`Else`/`End`, `LoopStart`/`End`,
   `SwitchStart`/`CaseStart`/`DefaultStart`/`End` must be properly nested.
   The `block_stack` tracks open blocks. `End` pops the top. Mismatches are
   errors.

5. **`break` / `continue` / `br_if_not` inside loop**: these ops are only
   valid when `loop_depth > 0`. Error if they appear outside any loop.

6. **`Else` after `IfStart`**: `Else` is only valid when the top of
   `block_stack` is `If`. Error otherwise.

7. **`CaseStart` / `DefaultStart` inside `SwitchStart`**: only valid when
   the top of `block_stack` is `Switch` (or a previous `Case`/`Default` that
   gets closed first).

8. **Switch case uniqueness**: within a single `switch`, no two `CaseStart`
   ops share the same `value`. At most one `DefaultStart`.

9. **Offset validity**: `IfStart.else_offset`, `IfStart.end_offset`,
   `LoopStart.end_offset`, etc. must point to valid indices within `body`
   and to the correct op type (`Else` or `End`).

#### Memory checks

10. **`SlotAddr` references a declared slot**: `SlotAddr { slot }` must have
    `slot.0 < func.slots.len()`.

11. **VRegPool ranges in bounds**: for every `Call` and `Return`, the
    `VRegRange` must satisfy `start + count <= vreg_pool.len()`.

#### Call checks

12. **Call arity**: the number of args in the `VRegRange` must match the
    callee's declared parameter count. The number of results must match the
    callee's declared return count.

13. **Call arg/result types**: arg VReg types must match callee param types.
    Result VReg types must match callee return types.

#### Return checks

14. **Return arity**: `Return { values }` must have `values.count` matching
    `func.return_types.len()`. For void functions, `count == 0`.

15. **Return value types**: each returned VReg's type must match the
    corresponding `return_types` entry.

### 4. Tests

#### Positive tests (valid IR passes)

```rust
#[test]
fn validate_simple_module() {
    let ir = "func @add(v0:f32, v1:f32) -> f32 {\n  v2:f32 = fadd v0, v1\n  return v2\n}\n";
    let module = parse_module(ir).unwrap();
    validate_module(&module).unwrap();
}
```

Test that all spec examples pass validation.

#### Negative tests (malformed IR rejected)

Group by category:

**VReg errors:**
- `validate_err_vreg_undefined` — use a VReg before defining it
- `validate_err_vreg_type_mismatch` — redefine with different type
- `validate_err_vreg_out_of_range` — VReg index beyond vreg_types

**Control flow errors:**
- `validate_err_break_outside_loop` — `break` not in a loop
- `validate_err_continue_outside_loop` — `continue` not in a loop
- `validate_err_br_if_not_outside_loop` — `br_if_not` not in a loop
- `validate_err_duplicate_case` — two `case 0` in same switch

**Call errors:**
- `validate_err_call_unknown_callee` — CalleeRef out of range
- `validate_err_call_arity` — wrong number of args
- `validate_err_call_type` — arg type mismatch

**Return errors:**
- `validate_err_return_arity` — return too many or too few values
- `validate_err_return_type` — returned VReg has wrong type

**Module errors:**
- `validate_err_duplicate_entry` — two entry functions
- `validate_err_duplicate_func_name` — two functions with same name

**Memory errors:**
- `validate_err_slot_addr_invalid` — references nonexistent slot
- `validate_err_vreg_pool_out_of_bounds` — VRegRange exceeds pool size

For negative tests, build the malformed IR directly using the builder (bypass
the parser, which might reject it) or construct `IrFunction`/`IrModule` by
hand.

## Validate

```
cargo check -p lpir
cargo test -p lpir
cargo +nightly fmt -- --check
```
