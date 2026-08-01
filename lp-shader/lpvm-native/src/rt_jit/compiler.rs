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
    // Classic-ESP32 firmware (`xt-placed-code`): every module goes through
    // the fixed-region placed path — the heap this Vec would execute from
    // in place has no I-bus view on that chip. This is THE chokepoint: all
    // three engine entry points (compile, compile_with_config, the async
    // compile job) land here.
    #[cfg(all(feature = "xt-placed-code", target_arch = "xtensa"))]
    {
        return link_compiled_module_jit_placed_global(compiled, builtin_table, isa);
    }

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
    // The profiler symbolizes sampled program counters against this base, so it
    // is an execute address, not the write address of the buffer.
    let base = unsafe { buffer.exec_ptr(0) } as usize as u32;
    emit_jit_symbols(base, buffer_len, &linked.entries);

    Ok((buffer, linked.entries))
}

/// The `xt-placed-code` firmware arm of [`link_compiled_module_jit`]: link
/// at a span from the boot-installed global arena, install through the
/// device's mirrored-write sink, and return a buffer that frees its span on
/// drop. Failing to reserve or install is a clean `NativeError` — on this
/// chip "code region full" is the real capacity edge, and it must surface
/// as a compile error, never a wild write.
#[cfg(all(feature = "xt-placed-code", target_arch = "xtensa"))]
fn link_compiled_module_jit_placed_global(
    compiled: CompiledModule,
    builtin_table: &BuiltinTable,
    isa: IsaTarget,
) -> Result<(JitBuffer, VecMap<String, usize>), NativeError> {
    use crate::codemem_esp32::{self, DeviceCodeSink, global};

    let total: usize = compiled.functions.iter().map(|f| f.code.len()).sum();
    let total_u32 = u32::try_from(total)
        .map_err(|_| NativeError::Internal("JIT image length does not fit u32".into()))?;

    let placed = global::with(
        |arena| -> Result<(JitBuffer, VecMap<String, usize>), NativeError> {
            let exec_base = arena
                .alloc(total_u32)
                .map_err(|e| NativeError::Internal(format!("JIT code placement failed: {e}")))?;

            let place = |arena: &mut codemem_esp32::CodeArena| {
                lp_perf::emit_begin!(EVENT_SHADER_LINK);
                let link_result = crate::link::link_jit_at(&compiled, isa, exec_base, |sym| {
                    builtin_table.lookup(sym).map(|addr| addr as u32)
                });
                lp_perf::emit_end!(EVENT_SHADER_LINK);
                let linked = link_result
                    .map_err(|e| NativeError::Internal(format!("JIT link failed: {e}")))?;

                let mut sink = DeviceCodeSink;
                codemem_esp32::install(arena.region(), exec_base, &linked.code, &mut sink)
                    .map_err(|e| NativeError::Internal(format!("JIT code install failed: {e}")))?;
                crate::codemem_esp32::CodeSink::sync(&mut sink);

                let buffer = JitBuffer::from_placed_global(exec_base as usize, linked.code.len());
                let buffer_len = u32::try_from(buffer.len()).map_err(|_| {
                    NativeError::Internal("JIT buffer length does not fit u32".into())
                })?;
                emit_jit_symbols(exec_base, buffer_len, &linked.entries);
                Ok((buffer, linked.entries))
            };
            match place(arena) {
                Ok(ok) => Ok(ok),
                Err(e) => {
                    arena.free(exec_base, total_u32);
                    Err(e)
                }
            }
        },
    )
    .map_err(|e| NativeError::Internal(format!("JIT code placement failed: {e}")))?;
    placed
}

/// [`compile_module_jit`] for the classic-ESP32 **fixed-region** placement:
/// reserve a span in `arena`, link intra-module call targets against the
/// span's I-bus base, install the staged image through the mirrored D-bus
/// walk, sync, and return a `Placed` buffer.
///
/// The span is freed back to the arena on any post-reservation failure; on
/// success it stays reserved and the CALLER owns its lifetime (free it with
/// `(exec_ptr(0), len)` when the module is dropped — the engine wiring for
/// that lands with the classic firmware, M3 of the classic bring-up plan).
pub fn compile_module_jit_placed(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    builtin_table: &BuiltinTable,
    options: &NativeCompileOptions,
    isa: IsaTarget,
    arena: &mut crate::codemem_esp32::CodeArena,
    sink: &mut impl crate::codemem_esp32::CodeSink,
) -> Result<(JitBuffer, VecMap<String, usize>), NativeError> {
    let compiled = compile_module(ir, sig, options.float_mode, options.clone(), isa)?;

    let total: usize = compiled.functions.iter().map(|f| f.code.len()).sum();
    let total_u32 = u32::try_from(total)
        .map_err(|_| NativeError::Internal("JIT image length does not fit u32".into()))?;
    let exec_base = arena
        .alloc(total_u32)
        .map_err(|e| NativeError::Internal(format!("JIT code placement failed: {e}")))?;

    let mut place = || -> Result<(JitBuffer, VecMap<String, usize>), NativeError> {
        lp_perf::emit_begin!(EVENT_SHADER_LINK);
        let link_result = crate::link::link_jit_at(&compiled, isa, exec_base, |sym| {
            builtin_table.lookup(sym).map(|addr| addr as u32)
        });
        lp_perf::emit_end!(EVENT_SHADER_LINK);
        let linked =
            link_result.map_err(|e| NativeError::Internal(format!("JIT link failed: {e}")))?;

        crate::codemem_esp32::install(arena.region(), exec_base, &linked.code, sink)
            .map_err(|e| NativeError::Internal(format!("JIT code install failed: {e}")))?;
        sink.sync();

        let buffer = JitBuffer::from_placed(exec_base as usize, linked.code.len());
        let buffer_len = u32::try_from(buffer.len())
            .map_err(|_| NativeError::Internal("JIT buffer length does not fit u32".into()))?;
        emit_jit_symbols(exec_base, buffer_len, &linked.entries);
        Ok((buffer, linked.entries))
    };
    match place() {
        Ok(ok) => Ok(ok),
        Err(e) => {
            arena.free(exec_base, total_u32);
            Err(e)
        }
    }
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
