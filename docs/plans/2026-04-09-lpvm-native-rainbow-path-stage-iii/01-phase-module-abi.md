# Phase 1: Create ModuleAbi

## Scope of Phase

Create the `ModuleAbi` struct that pre-computes ABI information for all functions in a module. This enables efficient lookup of callee ABI info during lowering and emission.

## Code Organization Reminders

- Place the struct definition and primary `impl` block at the top of the file
- Place helper functions at the bottom
- Add tests in `mod tests` at the bottom
- Keep related functionality grouped together

## Implementation Details

### File: `lp-shader/lpvm-native/src/abi/func_abi.rs`

Add `ModuleAbi` struct after the `FuncAbi` impl:

```rust
/// Pre-computed ABI information for all functions in a module.
/// Used for efficient callee lookup during lowering and emission.
#[derive(Clone, Debug)]
pub struct ModuleAbi {
    /// ABI per function (by name)
    func_abis: BTreeMap<String, FuncAbi>,
    /// Maximum sret buffer size needed for any callee (for caller-side pre-allocation)
    max_callee_sret_bytes: u32,
}

impl ModuleAbi {
    /// Build ModuleAbi from LpsModuleSig.
    /// Computes FuncAbi for each function and tracks max sret size.
    pub fn from_lps_module_sig(sig: &LpsModuleSig) -> Self {
        use crate::isa::rv32::abi::func_abi_rv32;
        use crate::abi::classify::entry_param_scalar_count;
        
        let mut func_abis = BTreeMap::new();
        let mut max_sret_bytes = 0u32;
        
        for fn_sig in &sig.functions {
            let n_params = entry_param_scalar_count(fn_sig);
            let func_abi = func_abi_rv32(fn_sig, n_params);
            
            // Track max sret size for callees
            if let Some(word_count) = func_abi.sret_word_count() {
                let bytes = word_count * 4;
                if bytes > max_sret_bytes {
                    max_sret_bytes = bytes;
                }
            }
            
            func_abis.insert(fn_sig.name.clone(), func_abi);
        }
        
        Self {
            func_abis,
            max_callee_sret_bytes: max_sret_bytes,
        }
    }
    
    /// Get the FuncAbi for a function by name.
    pub fn func_abi(&self, name: &str) -> Option<&FuncAbi> {
        self.func_abis.get(name)
    }
    
    /// Maximum sret buffer size needed for any callee, in bytes.
    /// Returns 0 if no callees use sret.
    pub fn max_callee_sret_bytes(&self) -> u32 {
        self.max_callee_sret_bytes
    }
}
```

### File: `lp-shader/lpvm-native/src/abi/mod.rs`

Export `ModuleAbi`:

```rust
pub use func_abi::{FuncAbi, ModuleAbi};  // UPDATE: add ModuleAbi
```

### Tests to Add

Add to `func_abi.rs` in `mod tests`:

```rust
#[test]
fn module_abi_empty() {
    let sig = LpsModuleSig { functions: vec![] };
    let abi = ModuleAbi::from_lps_module_sig(&sig);
    assert_eq!(abi.max_callee_sret_bytes(), 0);
    assert!(abi.func_abi("anything").is_none());
}

#[test]
fn module_abi_tracks_max_sret() {
    use lps_shared::{FnParam, LpsFnSig, LpsType, ParamQualifier};
    
    let sig = LpsModuleSig {
        functions: vec![
            LpsFnSig {
                name: "f".into(),
                return_type: LpsType::Vec4,  // 4 words = 16 bytes sret
                parameters: vec![],
            },
            LpsFnSig {
                name: "g".into(),
                return_type: LpsType::Mat4,  // 16 words = 64 bytes sret
                parameters: vec![],
            },
        ],
    };
    let abi = ModuleAbi::from_lps_module_sig(&sig);
    assert_eq!(abi.max_callee_sret_bytes(), 64);  // max of 16 and 64
    
    // Can look up individual function ABIs
    let f_abi = abi.func_abi("f").expect("f exists");
    assert!(f_abi.is_sret());
    assert_eq!(f_abi.sret_word_count(), Some(4));
    
    let g_abi = abi.func_abi("g").expect("g exists");
    assert!(g_abi.is_sret());
    assert_eq!(g_abi.sret_word_count(), Some(16));
}

#[test]
fn module_abi_no_sret_when_all_direct() {
    let sig = LpsModuleSig {
        functions: vec![
            LpsFnSig {
                name: "f".into(),
                return_type: LpsType::Float,  // direct
                parameters: vec![],
            },
            LpsFnSig {
                name: "g".into(),
                return_type: LpsType::Vec2,  // direct
                parameters: vec![],
            },
        ],
    };
    let abi = ModuleAbi::from_lps_module_sig(&sig);
    assert_eq!(abi.max_callee_sret_bytes(), 0);
}
```

## Validate

```bash
cargo test -p lpvm-native module_abi
cargo check -p lpvm-native
```

Ensure:
- Tests pass
- No compiler warnings
- `ModuleAbi` is properly exported from `abi` module
