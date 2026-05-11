# Phase 3: Remove Old F64 Files and Update Exports

## Scope

Remove the old `lps_value_f64.rs` and `lps_value_f64_convert.rs` files, and update `lps-shared/src/lib.rs` to export the new types and modules.

## Implementation

### Delete Files

1. Delete `lp-shader/lps-shared/src/lps_value_f64.rs`
2. Delete `lp-shader/lps-shared/src/lps_value_f64_convert.rs`

### Update lib.rs

```rust
//! Core GLSL type and function-signature shapes (no parser, no codegen).

#![no_std]

extern crate alloc;

pub mod layout;
pub mod lps_value_f32;
pub mod lps_value_q32;  // NEW
pub mod path;
pub mod path_resolve;
mod sig;
mod types;
pub mod value_path;

pub use layout::{array_stride, round_up, type_alignment, type_size};
pub use lps_value_f32::LpsValueF32;
pub use lps_value_q32::LpsValueQ32;  // NEW
pub use lps_value_q32::{lps_value_to_q32, q32_to_lps_value};  // NEW
pub use sig::{FnParam, LpsFnSig, LpsModuleSig, ParamQualifier};
pub use types::{LayoutRules, LpsType, StructMember};
```

## Validate

```bash
cargo check -p lps-shared 2>&1 | head -30
```

Fix any compilation errors from missing imports in other crates (will be addressed in subsequent phases).
