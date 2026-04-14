//! Concatenate emitted functions, record relocations, patch auipc+jalr at finalize.

use alloc::collections::BTreeMap;
use alloc::string::String;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;

use crate::compile::compile_module;
use crate::error::NativeError;
use crate::link::link_jit;
use crate::native_options::NativeCompileOptions;
use lpvm::ModuleDebugInfo;

use super::buffer::JitBuffer;
use super::builtins::BuiltinTable;

/// Emit and link a full module for JIT execution.
///
/// This is the main entry point for JIT compilation:
/// 1. Compile all functions (LPIR → VInst → PInst → bytes)
/// 2. Link with builtins (resolve relocations)
/// 3. Produce executable JitBuffer
///
/// # Arguments
/// * `ir` - LPIR module to compile
/// * `sig` - Module signatures
/// * `builtin_table` - Table of builtin function addresses
/// * `float_mode` - Floating point mode
/// * `alloc_trace` - Enable allocation tracing (TODO)
///
/// # Returns
/// (JitBuffer with executable code, entry offset map, debug info)
pub fn compile_module_jit(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    builtin_table: &BuiltinTable,
    float_mode: lpir::FloatMode,
    _alloc_trace: bool,
) -> Result<(JitBuffer, BTreeMap<String, usize>, ModuleDebugInfo), NativeError> {
    let options = NativeCompileOptions {
        float_mode,
        debug_info: false,
        emu_trace_instructions: false,
        alloc_trace: false,
    };

    // 1. Compile module
    let compiled = compile_module(ir, sig, float_mode, options)?;

    // 2. Build ModuleDebugInfo from compiled functions
    let mut debug_info = ModuleDebugInfo::new();
    for func in &compiled.functions {
        debug_info.add_function(func.debug_info.clone());
    }

    // 3. Link JIT image with builtin resolution
    let linked = link_jit(&compiled, |sym| {
        // First check builtins
        if let Some(addr) = builtin_table.lookup(sym) {
            return Some(addr as u32);
        }
        // Functions are resolved during link phase
        None
    })
    .map_err(|e| NativeError::Internal(format!("JIT link failed: {e}")))?;

    // 4. Create JitBuffer from linked code
    let buffer = JitBuffer::from_code(linked.code);

    Ok((buffer, linked.entries, debug_info))
}
