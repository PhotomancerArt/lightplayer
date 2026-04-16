## Scope of Phase

Add `AbiInfo` struct to `abi.rs` for per-function ABI classification from `LpsFnSig`.

## Implementation Details

### Add to `abi.rs`

```rust
/// Per-function ABI information derived from LpsFnSig.
/// Used by both caller (instance.rs) and emission (emit.rs).
#[derive(Debug, Clone)]
pub struct AbiInfo {
    /// Return classification (Direct or Sret)
    pub return_class: ReturnClass,
    /// Physical registers for arguments (may be shifted for sret)
    pub arg_regs: Vec<PhysReg>,
    /// Size of sret buffer if applicable (bytes)
    pub sret_size: Option<u32>,
    /// Scalar count of return type
    pub return_scalar_count: u32,
}

impl AbiInfo {
    /// Derive ABI info from LPIR function and signature.
    pub fn from_ir_and_sig(func: &IrFunction, sig: &LpsFnSig) -> Self {
        let return_class = ReturnClass::from_lps_types(&sig.return_type);
        let return_scalar_count = scalar_count_of_lps_type(&sig.return_type);
        
        let (arg_regs, sret_size) = match &return_class {
            ReturnClass::Sret { .. } => {
                // Sret: buffer ptr in a0, real args start at a1
                (ARG_REGS[1..].to_vec(), Some(return_scalar_count * 4))
            }
            ReturnClass::Direct { .. } => {
                // Direct: normal arg layout starting at a0
                (ARG_REGS.to_vec(), None)
            }
        };
        
        Self {
            return_class,
            arg_regs,
            sret_size,
            return_scalar_count,
        }
    }
}

/// Convert LpsType to scalar count.
fn scalar_count_of_lps_type(ty: &LpsType) -> u32 {
    use lps_shared::LpsType;
    match ty {
        LpsType::Void => 0,
        LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => 1,
        LpsType::Vec2 | LpsType::IVec2 | LpsType::UVec2 | LpsType::BVec2 => 2,
        LpsType::Vec3 | LpsType::IVec3 | LpsType::UVec3 | LpsType::BVec3 => 3,
        LpsType::Vec4 | LpsType::IVec4 | LpsType::UVec4 | LpsType::BVec4 => 4,
        LpsType::Mat2 => 4,   // 2x2
        LpsType::Mat3 => 9,   // 3x3
        LpsType::Mat4 => 16,  // 4x4
        _ => 1, // Conservative fallback
    }
}

/// Extend ReturnClass to work with LpsType.
impl ReturnClass {
    pub fn from_lps_types(ty: &LpsType) -> Self {
        let scalar_count = scalar_count_of_lps_type(ty);
        if scalar_count > 4 {
            ReturnClass::Sret { ptr_reg: A0 }
        } else {
            let regs = RET_REGS.iter().copied().take(scalar_count as usize).collect();
            ReturnClass::Direct { regs }
        }
    }
}
```

## Tests to Write

```rust
#[test]
fn abi_info_mat4_is_sret() {
    use lps_shared::{LpsType, LpsFnSig};
    
    let sig = LpsFnSig {
        name: String::from("test"),
        return_type: LpsType::Mat4,
        parameters: vec![],
    };
    
    let abi = AbiInfo::from_lps_sig(&sig);
    assert!(matches!(abi.return_class, ReturnClass::Sret { ptr_reg: A0 }));
    assert_eq!(abi.sret_size, Some(64)); // 16 scalars * 4 bytes
    assert_eq!(abi.arg_regs[0], A1); // First real arg in a1
}

#[test]
fn abi_info_vec4_is_direct() {
    use lps_shared::{LpsType, LpsFnSig};
    
    let sig = LpsFnSig {
        name: String::from("test"),
        return_type: LpsType::Vec4,
        parameters: vec![],
    };
    
    let abi = AbiInfo::from_lps_sig(&sig);
    assert!(matches!(abi.return_class, ReturnClass::Direct { .. }));
    assert_eq!(abi.sret_size, None);
    assert_eq!(abi.arg_regs[0], A0); // First arg in a0
}
```

## Validate

```bash
cargo test -p lpvm-native abi::tests::abi_info
cargo check -p lpvm-native
```
