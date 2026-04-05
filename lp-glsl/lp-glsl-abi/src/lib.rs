//! GLSL ABI (Application Binary Interface) - types, values, layout, and paths.
//!
//! Logical types ([`LpsType`], [`StructMember`], [`LayoutRules`]) come from
//! [`lps_types`]. This crate adds runtime values, byte layout, path resolution,
//! and module metadata for JIT / WASM.

#![no_std]

extern crate alloc;

mod data;
mod data_error;
mod layout;
mod lps_value;
mod metadata;
mod path;
mod path_resolve;
mod value_path;
mod vmcontext;

pub use data::LpvmData;
pub use data_error::DataError;
pub use layout::{array_stride, round_up, type_alignment, type_size};
pub use lps_types::{LayoutRules, LpsType, StructMember};
pub use lps_value::LpsValue;
pub use metadata::{GlslFunctionMeta, GlslModuleMeta, GlslParamMeta, GlslParamQualifier};
pub use path::{PathParseError, PathSegment, parse_path};
pub use path_resolve::{LpsTypePathExt, PathError};
pub use value_path::LpsValuePathError;
pub use vmcontext::{
    DEFAULT_VMCTX_FUEL, VMCTX_HEADER_SIZE, VMCTX_OFFSET_FUEL, VMCTX_OFFSET_METADATA,
    VMCTX_OFFSET_TRAP_HANDLER, VmContext, VmContextHeader, minimal_vmcontext,
};
