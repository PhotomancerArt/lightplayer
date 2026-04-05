//! GLSL ABI (Application Binary Interface) - types, values, layout, and paths.
//!
//! Logical types ([`LpsType`], [`StructMember`], [`LayoutRules`]), layout, and path helpers are
//! re-exported from [`lps_shared`]. This crate adds runtime byte buffers ([`LpvmData`]), module
//! metadata, and VM context for JIT / WASM.

#![no_std]

extern crate alloc;

mod data;
mod data_error;
mod vmcontext;

pub use data::LpvmData;
pub use data_error::DataError;
pub use lps_shared::layout::{array_stride, round_up, type_alignment, type_size};
pub use lps_shared::lps_value::LpsValue;
pub use lps_shared::path::{LpsPathSeg, PathParseError, parse_path};
pub use lps_shared::path_resolve::{LpsTypePathExt, PathError};
pub use lps_shared::value_path::{LpsValuePathError, LpsValuePathExt};
pub use lps_shared::{LayoutRules, LpsType, StructMember};
pub use vmcontext::{
    DEFAULT_VMCTX_FUEL, VMCTX_HEADER_SIZE, VMCTX_OFFSET_FUEL, VMCTX_OFFSET_METADATA,
    VMCTX_OFFSET_TRAP_HANDLER, VmContext, VmContextHeader, minimal_vmcontext,
};
