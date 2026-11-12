# Phase 7: Update lpvm_data_q32

## Scope

Add `from_q32`/`to_q32` methods to `LpvmDataQ32` for explicit Q32 value support.

## Implementation

Add to `lpvm/src/lpvm_data_q32.rs`:

```rust
impl LpvmDataQ32 {
    // Existing F32 methods
    pub fn from_value_f32(ty: &LpsType, v: &LpsValueF32) -> Result<Self, DataError>
    pub fn to_value_f32(&self) -> Result<LpsValueF32, DataError>
    
    // New Q32 methods
    pub fn from_value_q32(ty: &LpsType, v: &LpsValueQ32) -> Result<Self, DataError> {
        // Similar to from_value_f32 but Q32→f32→bytes
        let f32_val = lps_shared::q32_to_lps_value(ty, v.clone())?;
        Self::from_value_f32(ty, &f32_val)
    }
    
    pub fn to_value_q32(&self) -> Result<LpsValueQ32, DataError> {
        // F32→Q32
        let f32_val = self.to_value_f32()?;
        lps_shared::lps_value_to_q32(&self.ty, &f32_val)
            .map_err(|e| DataError::TypeMismatch(e))
    }
}
```

## Validate

```bash
cargo check -p lpvm
cargo test -p lpvm --lib
```
