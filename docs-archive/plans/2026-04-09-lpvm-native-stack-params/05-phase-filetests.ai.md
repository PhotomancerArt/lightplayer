## Phase 5: File Tests

## Scope

Create GLSL filetests to verify stack parameter passing works correctly end-to-end.

## Code Organization Reminders

- Place tests in `lp-shader/lps-filetests/filetests/lpvm/native/`
- Follow existing test format with `// run:` assertions
- Use descriptive function and variable names

## Implementation Details

### 1. param-many.glsl (NEW)

**File**: `lp-shader/lps-filetests/filetests/function/param-many.glsl`

Comprehensive test with various "many parameter" scenarios:

- 9 scalar params (2 on stack)
- mat3 with scalar (10 slots, 2 on stack)
- mat4 alone (16 slots, 8 on stack)
- Two mat2s (9 slots, 1 on stack) - matches call-nested.glsl case
- Vector combinations crossing the 8-slot boundary
- Nested calls with many params
- Chain of calls passing through many params

### 2. param-mixed.glsl (UPDATED)

**File**: `lp-shader/lps-filetests/filetests/function/param-mixed.glsl`

Added mat4 test cases with mixed qualifiers:

- `extract_diagonal(in mat4, out vec4)` - large input with output
- `scale_mat4(inout mat4, in float)` - inout with large type
- `process_mat4(in, out, inout)` - all three qualifiers with mat4

These exercise the stack param path with in/out/inout semantics.

### 3. call-nested.glsl (EXISTING)

**File**: `lp-shader/lps-filetests/filetests/function/call-nested.glsl`

The `combine_transforms_nested(mat2 a, mat2 b)` test has 9 slots. This should now pass.

## Validate

```bash
cd /Users/yona/dev/photomancer/feature/lightplayer-native

# Run new stack param tests
scripts/filetests.sh lpvm/native/stack-params-simple.glsl --target rv32lp.q32
scripts/filetests.sh lpvm/native/stack-params-mixed.glsl --target rv32lp.q32

# Run the previously failing test
scripts/filetests.sh function/call-nested.glsl --target rv32lp.q32
```

Expected: All tests pass, showing correct stack parameter handling.

## Edge Cases to Consider

1. **Sret + stack params**: Function returning mat4 (sret) + 6 scalar params = a0 for sret, a1-a7 for 7 args, 8th arg on stack
2. **Nested calls with different arg counts**: Function with 5 params calling one with 10 params
3. **All params on stack**: 20+ params (unlikely but should work)

These can be added as additional tests if needed.
