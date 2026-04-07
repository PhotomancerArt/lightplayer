//! LPVM runtime — traits, VM context, and execution abstractions.
//!
//! Core traits:
//! - [`LpvmEngine`] — compile LPIR and expose shared memory ([`LpvmMemory`])
//! - [`LpvmModule`] — compiled artifact + [`LpvmModule::instantiate`]
//! - [`LpvmInstance`] — call functions by name ([`LpsValueF32`] or flat Q32 words via [`LpvmInstance::call_q32`])
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
mod lpvm_abi;
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
pub use lps_shared::lps_value_q32::{LpsValueQ32, lps_value_f32_to_q32, q32_to_lps_value_f32};
pub use lps_shared::path::{LpsPathSeg, PathParseError, parse_path};
pub use lps_shared::path_resolve::{LpsTypePathExt, PathError};
pub use lps_shared::value_path::{LpsValuePathError, LpsValuePathExt};
pub use lps_shared::{LayoutRules, LpsType, StructMember};
pub use lpvm_abi::{
    CallError, CallResult, GlslReturn, decode_q32_return, flat_q32_words_from_f32_args,
    flatten_q32_arg, flatten_q32_return, glsl_component_count, unflatten_q32_args,
};
pub use lpvm_data_q32::LpvmDataQ32;
pub use memory::{AllocError, BumpLpvmMemory, LpvmMemory};
pub use module::LpvmModule;
pub use vmcontext::{
    DEFAULT_VMCTX_FUEL, VMCTX_HEADER_SIZE, VMCTX_OFFSET_FUEL, VMCTX_OFFSET_METADATA,
    VMCTX_OFFSET_TRAP_HANDLER, VmContext, VmContextHeader, minimal_vmcontext,
};
