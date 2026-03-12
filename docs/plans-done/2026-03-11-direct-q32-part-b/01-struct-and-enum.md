# Phase 1: Q32Strategy Struct and Constants

## Add Q32Strategy struct

In `frontend/codegen/numeric.rs`:

```rust
use crate::backend::transform::q32::options::Q32Options;
use crate::backend::transform::q32::types::float_to_fixed16x16;
use lp_model::glsl_opts::{AddSubMode, DivMode, MulMode};
use cranelift_codegen::ir::condcodes::IntCC;

pub struct Q32Strategy {
    pub opts: Q32Options,
}

impl Q32Strategy {
    pub fn new(opts: Q32Options) -> Self {
        Self { opts }
    }
}
```

## Add Q32 variant to NumericMode

```rust
pub enum NumericMode {
    Float(FloatStrategy),
    Q32(Q32Strategy),
}
```

Update every `match self` in `NumericMode` impl to add
`NumericMode::Q32(s) => s.method(...)` arms. There are 18 methods — each
gets a new arm.

## Implement emit_const

Extracted from `converters/constants.rs::convert_f32const`.

The transform version parses an InstructionData::UnaryIeee32 from old IR.
The strategy version just takes an f32 directly:

```rust
fn emit_const(&self, val: f32, builder: &mut FunctionBuilder) -> Value {
    let fixed = float_to_fixed16x16(val);
    builder.ins().iconst(types::I32, fixed as i64)
}
```

Uses `float_to_fixed16x16` from `backend/transform/q32/types.rs` — already
handles clamping and rounding.

## Implement scalar_type

```rust
fn scalar_type(&self) -> Type {
    types::I32
}
```

## Validate

```bash
cargo check -p lp-glsl-compiler --features std
```

The enum arms for Q32 will initially not compile until all methods are
implemented. If doing this incrementally, add `todo!()` stubs for
unimplemented methods first, then fill them in.
