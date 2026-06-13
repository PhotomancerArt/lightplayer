//! Concatenate emitted functions, record relocations, patch auipc+jalr at finalize.

use alloc::string::String;
use alloc::vec::Vec;
use lp_collection::VecMap;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;

use crate::compile::{CompiledModule, compile_module};
use crate::error::NativeError;
use crate::isa::IsaTarget;
use crate::jit_symbol_sizes::{derive_sizes, sort_by_offset};
use crate::link::link_jit;
use crate::native_options::NativeCompileOptions;
use lp_perf::{EVENT_SHADER_LINK, JitSymbolEntry, emit_jit_map_load};

use super::buffer::JitBuffer;
use super::builtins::BuiltinTable;

/// Emit and link a full module for JIT execution.
///
/// This is the main entry point for JIT compilation:
/// 1. Compile all functions (LPIR → VInst → machine code)
/// 2. Link with builtins (resolve relocations)
/// 3. Produce executable JitBuffer
///
/// # Arguments
/// * `ir` - LPIR module to compile
/// * `sig` - Module signatures
/// * `builtin_table` - Table of builtin function addresses
/// * `options` - Native compile options (float mode, LPIR [`lpir::CompilerConfig`], etc.)
///
/// # Returns
/// (JitBuffer with executable code, entry offset map, debug info)
pub fn compile_module_jit(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    builtin_table: &BuiltinTable,
    options: &NativeCompileOptions,
    isa: IsaTarget,
) -> Result<(JitBuffer, VecMap<String, usize>), NativeError> {
    let float_mode = options.float_mode;

    // 1. Compile module
    log::debug!(
        "[native-fa] compile_module_jit: starting compile_module with {} functions",
        ir.functions.len()
    );
    let compiled = compile_module(ir, sig, float_mode, options.clone(), isa)?;
    log::debug!(
        "[native-fa] compile_module_jit: compile_module complete, {} functions compiled",
        compiled.functions.len()
    );

    link_compiled_module_jit(compiled, builtin_table, isa)
}

pub(crate) fn link_compiled_module_jit(
    compiled: CompiledModule,
    builtin_table: &BuiltinTable,
    isa: IsaTarget,
) -> Result<(JitBuffer, VecMap<String, usize>), NativeError> {
    // 2. Link JIT image with builtin resolution
    lp_perf::emit_begin!(EVENT_SHADER_LINK);
    let link_result = link_jit(&compiled, isa, |sym| {
        // First check builtins
        if let Some(addr) = builtin_table.lookup(sym) {
            return Some(addr as u32);
        }
        // Functions are resolved during link phase
        None
    });
    lp_perf::emit_end!(EVENT_SHADER_LINK);
    let linked = link_result.map_err(|e| NativeError::Internal(format!("JIT link failed: {e}")))?;

    // 3. Create JitBuffer from linked code
    let buffer = JitBuffer::from_code(linked.code);

    let buffer_len = u32::try_from(buffer.len())
        .map_err(|_| NativeError::Internal("JIT buffer length does not fit u32".into()))?;
    let base = unsafe { buffer.entry_ptr(0) } as usize as u32;
    emit_jit_symbols(base, buffer_len, &linked.entries);

    Ok((buffer, linked.entries))
}

/// Builds [`JitSymbolEntry`] records (names in `name_buf`) and notifies the profiler sink.
fn emit_jit_symbols(buffer_base: u32, buffer_len: u32, entry_offsets: &VecMap<String, usize>) {
    if entry_offsets.is_empty() {
        return;
    }

    let sorted = sort_by_offset(entry_offsets);
    let offsets: Vec<u32> = sorted.iter().map(|(_, o)| *o).collect();
    let sizes = derive_sizes(&offsets, buffer_len);

    let mut name_buf: Vec<u8> = Vec::new();
    let mut name_locs: Vec<(u32, u32)> = Vec::with_capacity(sorted.len());
    for (name, _) in &sorted {
        let off = name_buf.len() as u32;
        name_buf.extend_from_slice(name.as_bytes());
        name_locs.push((off, name.len() as u32));
    }
    let name_buf_base = name_buf.as_ptr() as usize as u32;

    let mut entries: Vec<JitSymbolEntry> = Vec::with_capacity(sorted.len());
    for (i, (_, off)) in sorted.iter().enumerate() {
        let (name_off, name_len) = name_locs[i];
        entries.push(JitSymbolEntry {
            offset: *off,
            size: sizes[i],
            name_ptr: name_buf_base.wrapping_add(name_off),
            name_len,
        });
    }

    emit_jit_map_load(buffer_base, buffer_len, &entries);
}
