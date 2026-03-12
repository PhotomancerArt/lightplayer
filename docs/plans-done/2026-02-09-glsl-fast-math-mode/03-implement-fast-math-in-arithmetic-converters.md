# Phase 3: Implement Fast Math in Q32 Arithmetic Converters

## Scope of phase

When `fast_math` is true, `convert_fadd` and `convert_fsub` emit inline `iadd`/`isub` instead of calling the saturating builtins. When false, keep existing builtin call behavior.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. convert_fadd - fast math path

**File**: `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/converters/arithmetic.rs`

When `fast_math` is true:
- Extract operands: `let (arg1_old, arg2_old) = extract_binary_operands(...)?`
- Map operands: `map_operand` as currently done
- Emit: `let result = builder.ins().iadd(arg1, arg2);`
- Map result: `value_map.insert(old_result, result);`
- Return `Ok(())`

No need for `func_id_map` when fast_math is true.

Structure:

```rust
pub(crate) fn convert_fadd(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<...>,
    _format: FixedPointFormat,
    fast_math: bool,
    func_id_map: &HashMap<String, FuncId>,
) -> Result<(), GlslError> {
    let (arg1_old, arg2_old) = extract_binary_operands(old_func, old_inst)?;
    let arg1 = map_operand(old_func, value_map, arg1_old)?;
    let arg2 = map_operand(old_func, value_map, arg2_old)?;

    if fast_math {
        let result = builder.ins().iadd(arg1, arg2);
        let old_result = get_first_result(old_func, old_inst);
        value_map.insert(old_result, result);
        return Ok(());
    }

    // Existing builtin call path ...
}
```

### 2. convert_fsub - fast math path

Same pattern: when `fast_math`, use `builder.ins().isub(arg1, arg2)` instead of the builtin call.

### 3. Update call sites in instructions.rs

Ensure `convert_fadd` and `convert_fsub` are called with the `fast_math` parameter from the instruction router.

## Validate

```bash
cargo build -p lp-glsl-compiler
cargo test -p lp-glsl-compiler
scripts/glsl-filetests.sh
```

Run filetests to ensure existing shaders still produce correct results (with fast_math=false by default). Consider adding a filetest that uses fast_math and checks for iadd/isub in the output if such a test framework exists.
