# Phase 4: Interpreter Tests — Control Flow, Memory, Calls, Stack Overflow, Error Paths

## Scope

Complete interpreter test coverage with control flow constructs, memory
operations, function calls (local and import), recursion, the stack overflow
guard, and error paths.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation details

All tests go in `tests/interp.rs`.

### Control flow — if / else

```rust
#[test]
fn interp_if_true_branch() {
    let ir = "\
func @f(v0:i32) -> i32 {
  v1:i32 = iconst.i32 10
  v2:i32 = iconst.i32 20
  if v0 {
    v1 = iconst.i32 99
  }
  return v1
}
";
    assert_eq!(run_i32(ir, "f", &[Value::I32(1)]), 99);
}

#[test]
fn interp_if_false_branch() {
    // same IR as above, v0=0 → v1 stays 10
    assert_eq!(run_i32(ir, "f", &[Value::I32(0)]), 10);
}
```

- `interp_if_true_branch`: enters then-arm
- `interp_if_false_branch`: skips then-arm
- `interp_if_else_true`: enters then-arm, skips else
- `interp_if_else_false`: enters else-arm
- `interp_if_else_return`: the `@max` example from spec — fgt then return
  from the appropriate branch

### Control flow — loop

- `interp_loop_sum_to_n`: the `@sum_to_n` spec example (loop with br_if_not
  and continue), verify sum_to_n(10) = 45
- `interp_loop_break`: loop that immediately breaks, verify no infinite loop
- `interp_loop_continue`: loop with continue that skips ops after continue
- `interp_nested_loops`: the `@nested` spec example, verify
  nested(3, 4) = 0+1+2+3 + 0+1+2+3 + 0+1+2+3 = 18

### Control flow — switch

- `interp_switch_case_match`: `@dispatch` spec example with selector=1 → 2.0
- `interp_switch_default`: selector=99 hits default → -1.0
- `interp_switch_no_default`: switch with only cases, selector matches none
  → falls through, original value preserved

### Control flow — early return

- `interp_early_return`: `@early_return` spec example — negative input returns
  negated value from inside the if

### Control flow — br_if_not

- `interp_br_if_not_exits`: br_if_not with cond=0 exits the loop
- `interp_br_if_not_continues`: br_if_not with cond=1 falls through

### Memory

```rust
#[test]
fn interp_slot_store_load() {
    let ir = "\
func @f(v0:f32) -> f32 {
  slot ss0, 4
  v1:i32 = slot_addr ss0
  store v1, 0, v0
  v2:f32 = load v1, 0
  return v2
}
";
    let r = run_f32(ir, "f", &[Value::F32(42.5)]);
    assert!((r - 42.5).abs() < 1e-6);
}
```

- `interp_slot_store_load`: store f32 → load f32 round-trips
- `interp_slot_store_load_i32`: store i32 → load i32 round-trips
- `interp_slot_offset`: store at offset 4, load at offset 4
- `interp_memcpy`: write to one slot region, memcpy to another, read back
- `interp_dynamic_index`: the `@arr_dyn` spec example — store 4 values, read
  back by dynamic index

### Calls — local

- `interp_local_call`: module with `@helper` and `@main`, main calls helper
- `interp_local_call_multi_return`: callee returns (f32, f32), caller receives
  both

### Calls — import (mock)

Add a `MockMathImports` handler that implements a few `@std.math` functions:

```rust
struct MockMathImports;

impl ImportHandler for MockMathImports {
    fn call(
        &mut self,
        module_name: &str,
        func_name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, InterpError> {
        match (module_name, func_name) {
            ("std.math", "fabs") => {
                let v = args[0].as_f32().unwrap();
                Ok(vec![Value::F32(v.abs())])
            }
            ("std.math", "fmax") => {
                let a = args[0].as_f32().unwrap();
                let b = args[1].as_f32().unwrap();
                Ok(vec![Value::F32(a.max(b))])
            }
            _ => Err(InterpError::Import(
                alloc::format!("unknown {module_name}::{func_name}"),
            )),
        }
    }
}
```

- `interp_import_call_unary`: call @std.math::fabs(-3.0) → 3.0
- `interp_import_call_binary`: call @std.math::fmax(1.0, 5.0) → 5.0
- `interp_import_error`: call unknown import → InterpError::Import

### Calls — recursion

- `interp_factorial`: recursive factorial function, verify factorial(5) = 120

```
func @fact(v0:i32) -> i32 {
  v1:i32 = ieq_imm v0, 0
  if v1 {
    v2:i32 = iconst.i32 1
    return v2
  }
  v3:i32 = iconst.i32 1
  v4:i32 = isub v0, v3
  v5:i32 = call @fact(v4)
  v6:i32 = imul v0, v5
  return v6
}
```

### Stack overflow

- `interp_stack_overflow`: a function that calls itself unconditionally with a
  low max_depth (e.g. 4), verify `InterpError::StackOverflow`.

```rust
#[test]
fn interp_stack_overflow() {
    let ir = "\
func @inf(v0:i32) -> i32 {
  v1:i32 = call @inf(v0)
  return v1
}
";
    let m = parse_module(ir).unwrap();
    let err = interpret_with_depth(&m, "inf", &[Value::I32(0)], &mut NoImports, 4)
        .unwrap_err();
    assert!(matches!(err, InterpError::StackOverflow));
}
```

### Error paths

- `interp_err_function_not_found`: call a nonexistent function name →
  `InterpError::FunctionNotFound`
- `interp_err_arg_arity`: pass wrong number of args →
  `InterpError::Internal` (arity check in `exec_func`)

## Validate

```
cargo test -p lpir
cargo check -p lpir
cargo +nightly fmt -- --check
```
