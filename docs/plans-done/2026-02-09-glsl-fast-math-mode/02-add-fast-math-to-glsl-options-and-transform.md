# Phase 2: Add fast_math to GlslOptions and Wire Through Compiler

## Scope of phase

Add `fast_math: bool` to `GlslOptions`, pass it from compile functions to `Q32Transform`, and add the field to `Q32Transform` so it can be passed to the q32 arithmetic converters in phase 3.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Add fast_math to GlslOptions

**File**: `lp-glsl/lp-glsl-compiler/src/exec/executable.rs`

```rust
pub struct GlslOptions {
    pub run_mode: RunMode,
    pub decimal_format: DecimalFormat,
    /// Use inline iadd/isub for q32 add/sub (wrapping) instead of saturating builtins
    pub fast_math: bool,
}
```

Update all constructors:

- `GlslOptions { run_mode, decimal_format, fast_math: false }` at minimum
- `jit()`, `emulator()`, `emu_riscv32_imac()` - add `fast_math: false`

### 2. Pass fast_math to Q32Transform in frontend

**File**: `lp-glsl/lp-glsl-compiler/src/frontend/mod.rs`

In `compile_glsl_to_gl_module_jit` and `compile_glsl_to_gl_module_object`, when building Q32Transform:

```rust
// Before
let transform = Q32Transform::new(FixedPointFormat::Fixed16x16);

// After
let transform = Q32Transform::new(FixedPointFormat::Fixed16x16)
    .with_fast_math(options.fast_math);
```

Or add a constructor: `Q32Transform::new_with_options(format, fast_math)` - whichever fits the API better.

### 3. Add fast_math to Q32Transform

**File**: `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/transform.rs`

- Add `fast_math: bool` field to `Q32Transform`
- Update `new(format)` to set `fast_math: false`
- Add `pub fn with_fast_math(mut self, fast_math: bool) -> Self { self.fast_math = fast_math; self }`
- Or add `pub fn new_with_options(format: FixedPointFormat, fast_math: bool) -> Self`
- In `transform_function`, pass `self.fast_math` into the instruction converter (via the closure or through `transform_function_body`). The closure currently captures `format`; add `fast_math` similarly.

Check how `format` flows: it's captured in the closure and passed to `convert_all_instructions`. We need to also pass `fast_math`. Update the closure to pass `(format, fast_math)` or add `fast_math` as a separate capture.

### 4. Update convert_all_instructions signature

**File**: `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/instructions.rs`

- Add `fast_math: bool` parameter to `convert_all_instructions`
- Pass it to `convert_instruction`, which passes it to `convert_fadd` and `convert_fsub`

The call site is in `transform.rs` - the closure calls `convert_all_instructions(..., format, ..., fast_math)`.

For phase 2, we only need to add the parameter and pass it through. Phase 3 will use it in the arithmetic converters. So in phase 2:
- Add `fast_math` to the `convert_all_instructions` and `convert_instruction` signatures
- Pass it to `convert_fadd` and `convert_fsub` (they will need the new param)

### 5. Update convert_fadd and convert_fsub signatures

**File**: `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/converters/arithmetic.rs`

Add `fast_math: bool` parameter. For now, ignore it (phase 3 implements the behavior). Just pass it through from the instruction router.

## Validate

```bash
cargo build -p lp-glsl-compiler
cargo test -p lp-glsl-compiler
```
