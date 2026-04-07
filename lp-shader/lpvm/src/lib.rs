//! LPVM runtime — traits, VM context, and execution abstractions.
//!
//! Core traits:
//! - [`LpvmEngine`] — compile LPIR and expose shared memory ([`LpvmMemory`])
//! - [`LpvmModule`] — compiled artifact + [`LpvmModule::instantiate`]
//! - [`LpvmInstance`] — call functions by name with [`LpsValue`] args
//! - [`LpvmMemory`] / [`ShaderPtr`] — host/guest shared heap
//!
//! Logical types ([`LpsType`], [`StructMember`], [`LayoutRules`]) and path
//! helpers come from [`lps_shared`]. This crate adds [`LpvmData`] and
//! [`VmContext`].

#![no_std]

extern crate alloc;

mod data;
mod data_error;
mod engine;
mod instance;
mod memory;
mod module;
mod shader_ptr;
mod vmcontext;

pub use data::LpvmData;
pub use data_error::DataError;
pub use engine::LpvmEngine;
pub use instance::LpvmInstance;
pub use lps_shared::layout::{array_stride, round_up, type_alignment, type_size};
pub use lps_shared::lps_value::LpsValue;
pub use lps_shared::path::{LpsPathSeg, PathParseError, parse_path};
pub use lps_shared::path_resolve::{LpsTypePathExt, PathError};
pub use lps_shared::value_path::{LpsValuePathError, LpsValuePathExt};
pub use lps_shared::{LayoutRules, LpsType, StructMember};
pub use memory::{AllocError, BumpLpvmMemory, LpvmMemory};
pub use module::LpvmModule;
pub use shader_ptr::ShaderPtr;
pub use vmcontext::{
    DEFAULT_VMCTX_FUEL, VMCTX_HEADER_SIZE, VMCTX_OFFSET_FUEL, VMCTX_OFFSET_METADATA,
    VMCTX_OFFSET_TRAP_HANDLER, VmContext, VmContextHeader, minimal_vmcontext,
};
