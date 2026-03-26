# Phase 3: Q32 Type Mapping and Constant Encoding

## Scope

Implement Q32-aware type mapping (`IrType::F32` → Cranelift `I32`) and Q32
constant encoding (`Fconst` → `iconst` with Q16.16 value). Update
`signature_for_ir_func` and variable declaration to respect `FloatMode`.
Add basic Q32 arithmetic test using builtins.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Create `src/q32.rs`

```rust
//! Q16.16 fixed-point helpers for Q32 emission.

const Q32_SHIFT: i64 = 16;
const Q32_SCALE: f64 = 65536.0;
const Q32_MAX: i64 = 0x7FFF_FFFF;
const Q32_MIN: i64 = i32::MIN as i64;

/// Encode an f32 constant as a Q16.16 fixed-point i32.
pub(crate) fn q32_encode(value: f32) -> i32 {
    let scaled = (value as f64 * Q32_SCALE).round();
    if scaled > Q32_MAX as f64 {
        Q32_MAX as i32
    } else if scaled < Q32_MIN as f64 {
        i32::MIN
    } else {
        scaled as i32
    }
}
```

Add unit tests for `q32_encode`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_basics() {
        assert_eq!(q32_encode(0.0), 0);
        assert_eq!(q32_encode(1.0), 65536);
        assert_eq!(q32_encode(-1.0), -65536);
        assert_eq!(q32_encode(1.5), 98304);
        assert_eq!(q32_encode(0.5), 32768);
    }

    #[test]
    fn encode_saturation() {
        assert_eq!(q32_encode(40000.0), 0x7FFF_FFFF);
        assert_eq!(q32_encode(-40000.0), i32::MIN);
    }
}
```

### 2. Update `emit/mod.rs` — Q32-aware `ir_type`

The shared `ir_type` function needs `FloatMode`:

```rust
pub(crate) fn ir_type_for_mode(t: IrType, mode: FloatMode) -> types::Type {
    match (t, mode) {
        (IrType::I32, _) => types::I32,
        (IrType::F32, FloatMode::F32) => types::F32,
        (IrType::F32, FloatMode::Q32) => types::I32,
    }
}
```

Update `signature_for_ir_func` to use `ir_type_for_mode`:

```rust
pub fn signature_for_ir_func(
    func: &IrFunction,
    call_conv: CallConv,
    mode: FloatMode,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    for i in 0..func.param_count as usize {
        sig.params.push(AbiParam::new(ir_type_for_mode(func.vreg_types[i], mode)));
    }
    for t in &func.return_types {
        sig.returns.push(AbiParam::new(ir_type_for_mode(*t, mode)));
    }
    sig
}
```

Update variable declarations in `translate_function`:

```rust
for ty in &func.vreg_types {
    vars.push(builder.declare_var(ir_type_for_mode(*ty, ctx.float_mode)));
}
```

### 3. Update `emit/scalar.rs` — Q32 constant encoding

```rust
Op::FconstF32 { dst, value } => {
    match ctx.float_mode {
        FloatMode::F32 => {
            def_v_expr(builder, vars, *dst, |bd| bd.ins().f32const(*value))
        }
        FloatMode::Q32 => {
            let encoded = crate::q32::q32_encode(*value);
            def_v_expr(builder, vars, *dst, |bd| {
                bd.ins().iconst(types::I32, i64::from(encoded))
            })
        }
    }
}
```

### 4. Update `jit_module.rs`

Pass `mode` to `signature_for_ir_func`:

```rust
let sig = emit::signature_for_ir_func(f, call_conv, mode);
```

### 5. Test

**`test_q32_constant_roundtrip`** — encode and return a Q32 constant:
```
func @const_half() -> f32 {
  v0:f32 = fconst 0.5
  return v0
}
```
In Q32 mode, this should compile. Call the function, verify the returned i32
equals `q32_encode(0.5)` which is `32768`.

```rust
#[test]
fn jit_q32_constant() {
    let ir = parse_module("func @const_half() -> f32 {\n  v0:f32 = fconst 0.5\n  return v0\n}\n")
        .expect("parse");
    let (jit, ids) = jit_from_ir(&ir, FloatMode::Q32).expect("jit");
    let f: extern "C" fn() -> i32 = unsafe { mem::transmute(jit.get_finalized_function(ids[0])) };
    assert_eq!(f(), 32768); // 0.5 in Q16.16
}
```

**`test_q32_identity`** — pass a Q32 value through and get it back:
```
func @identity(v0:f32) -> f32 {
  return v0
}
```
```rust
#[test]
fn jit_q32_identity() {
    let ir = parse_module("func @identity(v0:f32) -> f32 {\n  return v0\n}\n")
        .expect("parse");
    let (jit, ids) = jit_from_ir(&ir, FloatMode::Q32).expect("jit");
    let f: extern "C" fn(i32) -> i32 = unsafe { mem::transmute(jit.get_finalized_function(ids[0])) };
    assert_eq!(f(65536), 65536); // 1.0 in, 1.0 out
}
```

## Validate

```
cargo check -p lpir-cranelift
cargo test -p lpir-cranelift
```
