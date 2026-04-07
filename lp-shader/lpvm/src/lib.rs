//! LPVM runtime — traits, VM context, and execution abstractions.
//!
//! Core traits:
//! - [`LpvmEngine`] — compile LPIR and expose shared memory ([`LpvmMemory`])
//! - [`LpvmModule`] — compiled artifact + [`LpvmModule::instantiate`]
//! - [`LpvmInstance`] — call functions by name with [`LpsValueF32`] args
//! - [`LpvmMemory`] / [`ShaderPtr`] — host/guest shared heap
//!
//! Logical types ([`LpsType`], [`StructMember`], [`LayoutRules`]) and path
//! helpers come from [`lps_shared`]. This crate adds [`LpvmDataQ32`] and
//! [`VmContext`].

#![no_std]

extern crate alloc;

mod buffer;
mod data_error;
mod engine;
mod instance;
mod lpvm_data_q32;
mod memory;
mod module;
mod vmcontext;

pub use buffer::{LpvmBuffer, LpvmPtr};
pub use data_error::DataError;
pub use engine::LpvmEngine;
pub use instance::LpvmInstance;
pub use lps_shared::layout::{array_stride, round_up, type_alignment, type_size};
pub use lps_shared::lps_value_f32::LpsValueF32;
pub use lps_shared::path::{parse_path, LpsPathSeg, PathParseError};
pub use lps_shared::path_resolve::{LpsTypePathExt, PathError};
pub use lps_shared::value_path::{LpsValuePathError, LpsValuePathExt};
pub use lps_shared::{LayoutRules, LpsType, StructMember};
pub use lpvm_data_q32::LpvmDataQ32;
pub use memory::{AllocError, BumpLpvmMemory, LpvmMemory};
pub use module::LpvmModule;
pub use vmcontext::{
    minimal_vmcontext, VmContext, VmContextHeader, DEFAULT_VMCTX_FUEL,
    VMCTX_HEADER_SIZE, VMCTX_OFFSET_FUEL, VMCTX_OFFSET_METADATA, VMCTX_OFFSET_TRAP_HANDLER,
};
