# Phase 3: Update Transform to Use Builtins

## Goal

Update `convert_fadd`, `convert_fsub`, and `convert_fdiv` to use builtin functions instead of
generating inline code.

## Tasks

### 3.1 Update convert_fadd

In `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/converters/arithmetic.rs`:

- Replace inline saturation code (~20 instructions) with builtin call
- Follow the pattern from `convert_fmul`:
    - Get FuncId from func_id_map for `"__lp_q32_add"`
    - Create signature: (i32, i32) -> i32
    - Create UserExternalName with FuncId
    - Import external function (colocated: false)
    - Call builtin with mapped arguments
- Remove all inline saturation logic

### 3.2 Update convert_fsub

In the same file:

- Replace inline saturation code (~20 instructions) with builtin call
- Follow the same pattern as `convert_fadd`:
    - Get FuncId from func_id_map for `"__lp_q32_sub"`
    - Create signature: (i32, i32) -> i32
    - Create UserExternalName with FuncId
    - Import external function (colocated: false)
    - Call builtin with mapped arguments
- Remove all inline saturation logic

### 3.3 Update convert_fdiv

In the same file:

- Replace inline division code (~30 instructions) with builtin call
- Follow the same pattern as `convert_fmul`:
    - Get FuncId from func_id_map for `"__lp_q32_div"`
    - Create signature: (i32, i32) -> i32
    - Create UserExternalName with FuncId
    - Import external function (colocated: false)
    - Call builtin with mapped arguments
- Remove all inline division logic (zero checks, small divisor handling, etc.)

### 3.4 Update Function Signature

Ensure `convert_fadd` and `convert_fsub` accept `func_id_map` parameter:

- Add `func_id_map: &HashMap<String, FuncId>` parameter (like `convert_fmul` has)
- Update call sites in `instructions.rs` to pass func_id_map

## Success Criteria

- `convert_fadd` uses `__lp_q32_add` builtin
- `convert_fsub` uses `__lp_q32_sub` builtin
- `convert_fdiv` uses `__lp_q32_div` builtin
- All inline saturation/division code removed
- Code compiles without errors
- Code formatted with `cargo +nightly fmt`

## Code Organization

- Place helper utility functions at the bottom of files
- Place more abstract things, entry points, and tests first
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented
