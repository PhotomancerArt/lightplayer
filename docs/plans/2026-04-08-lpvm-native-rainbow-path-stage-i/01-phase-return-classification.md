## Scope of Phase

Implement return value classification: determine whether a function returns values in registers (Direct) or via sret pointer (Sret).

## Code Organization Reminders

- Place `ReturnClass` enum and `classify_return()` function early in `abi.rs`
- Unit tests at the bottom of the file
- Keep classification logic separate from emission

## Implementation Details

### ReturnClass enum

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnClass {
    /// Return in registers a0-a3 (up to 4 scalars / 16 bytes)
    Direct { regs: Vec<PhysReg> },
    /// Return via pointer in a0 (caller-allocated buffer)
    Sret { ptr_reg: PhysReg },
}

impl ReturnClass {
    /// Classify based on LPIR return type count and size
    /// - ≤4 scalars (16 bytes): Direct in a0-a3
    /// - >4 scalars: Sret pointer in a0
    pub fn from_types(return_types: &[IrType]) -> Self {
        let scalar_count: usize = return_types.iter().map(|t| t.scalar_count()).sum();
        if scalar_count > 4 {
            ReturnClass::Sret { ptr_reg: A0 }
        } else {
            let regs = RET_REGS.iter().copied().take(scalar_count).collect();
            ReturnClass::Direct { regs }
        }
    }
}
```

### Helper on IrType

Add to `types.rs` or use inline:
```rust
impl IrType {
    fn scalar_count(&self) -> usize {
        match self {
            IrType::I32 | IrType::F32 => 1,
            IrType::Vec2 => 2,
            IrType::Vec3 => 3,
            IrType::Vec4 => 4,
            // Mat4 = 16 scalars (4x4)
            IrType::Mat4 => 16,
            _ => 1,
        }
    }
}
```

## Tests to Write

```rust
#[test]
fn classify_scalar_is_direct() {
    let rc = ReturnClass::from_types(&[IrType::I32]);
    assert!(matches!(rc, ReturnClass::Direct { regs } if regs == vec![A0]));
}

#[test]
fn classify_vec4_is_direct() {
    let rc = ReturnClass::from_types(&[IrType::Vec4]);
    assert!(matches!(rc, ReturnClass::Direct { regs } if regs == vec![A0, A1, A2, A3]));
}

#[test]
fn classify_mat4_is_sret() {
    let rc = ReturnClass::from_types(&[IrType::Mat4]);
    assert!(matches!(rc, ReturnClass::Sret { ptr_reg: A0 }));
}

#[test]
fn classify_vec2_vec2_is_sret() {
    // 2 + 2 = 4 scalars, still direct
    let rc = ReturnClass::from_types(&[IrType::Vec2, IrType::Vec2]);
    assert!(matches!(rc, ReturnClass::Direct { regs } if regs.len() == 4));
}

#[test]
fn classify_vec4_scalar_is_sret() {
    // 4 + 1 = 5 scalars, exceeds 4
    let rc = ReturnClass::from_types(&[IrType::Vec4, IrType::I32]);
    assert!(matches!(rc, ReturnClass::Sret { .. }));
}
```

## Validate

```bash
cargo test -p lpvm-native abi::tests::classify_
```

All classification tests should pass before proceeding.
