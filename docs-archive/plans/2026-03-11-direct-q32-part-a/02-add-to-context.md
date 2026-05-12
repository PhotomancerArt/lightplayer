# Phase 2: Add NumericMode to CodegenContext

## Changes to `frontend/codegen/context.rs`

Add a `numeric` field to `CodegenContext`:

```rust
use crate::frontend::codegen::numeric::NumericMode;

pub struct CodegenContext<'a, M: Module> {
    pub builder: FunctionBuilder<'a>,
    pub gl_module: &'a mut GlModule<M>,
    pub numeric: NumericMode,  // NEW
    // ... rest unchanged
}
```

Update the constructor:

```rust
pub fn new(
    builder: FunctionBuilder<'a>,
    gl_module: &'a mut GlModule<M>,
    source_map: &'a mut GlSourceMap,
    current_file_id: GlFileId,
    numeric: NumericMode,  // NEW parameter
) -> Self {
    Self {
        builder,
        gl_module,
        numeric,
        // ... rest unchanged
    }
}
```

## Changes to `frontend/glsl_compiler.rs`

Update `compile_function_to_clif_impl` (the single construction site)
to pass `NumericMode::Float(FloatStrategy)`:

```rust
let mut codegen_ctx = CodegenContext::new(
    builder,
    gl_module,
    source_map,
    file_id,
    NumericMode::Float(FloatStrategy),
);
```

For now, this is hardcoded. In Plan D, it will be parameterized based on
`DecimalFormat`.

## Convenience methods on CodegenContext

Add delegation methods so call sites stay concise:

```rust
impl<'a, M: Module> CodegenContext<'a, M> {
    pub fn emit_float_const(&mut self, val: f32) -> Value {
        self.numeric.emit_const(val, &mut self.builder)
    }
    pub fn emit_float_add(&mut self, a: Value, b: Value) -> Value {
        self.numeric.emit_add(a, b, &mut self.builder)
    }
    // ... etc for all numeric operations
    pub fn float_type(&self) -> Type {
        self.numeric.scalar_type()
    }
}
```

These are optional but help keep the call sites clean. The alternative is
`ctx.numeric.emit_add(a, b, &mut ctx.builder)` at every call site, which
has a borrow-checker issue: `ctx.numeric` and `ctx.builder` are both
borrowed from `ctx`.

The convenience methods solve this by taking `&mut self` and accessing
both fields within the method body. This is the recommended approach.

If builtins need to call these from `&mut self` methods on CodegenContext
(which they do — `emit_builtin_call` is `&mut self`), the convenience
methods work naturally.

## No behavioral change

At this point, the numeric field exists but nothing uses it yet. All
existing code continues to call `builder.ins().fadd(...)` directly.
The compiler output is unchanged.
