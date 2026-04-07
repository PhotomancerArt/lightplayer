//! Link LPIR object with the builtins executable (feature `riscv32-object`).

use alloc::string::String;
use alloc::vec::Vec;

use lp_riscv_elf::ElfLoadInfo;
use lps_builtin_ids::BuiltinId;

use crate::error::{CompileError, CompilerError};

mod builtins_exe {
    include!(concat!(env!("OUT_DIR"), "/lp_builtins_lib.rs"));
}

/// Embedded `lps-builtins-emu-app` bytes (empty if missing at build time).
pub fn builtins_executable_bytes() -> &'static [u8] {
    builtins_exe::LP_BUILTINS_EXE_BYTES
}

/// Load builtins ELF, merge relocatable `object_elf`, verify `BuiltinId` symbols.
pub fn link_object_with_builtins(object_elf: &[u8]) -> Result<ElfLoadInfo, CompilerError> {
    let builtins_exe_bytes = builtins_executable_bytes();
    if builtins_exe_bytes.is_empty() {
        return Err(CompilerError::Codegen(CompileError::unsupported(
            "lps-builtins-emu-app is empty or was not found at build time; run scripts/build-builtins.sh from the workspace root",
        )));
    }

    let mut load_info = lp_riscv_elf::load_elf(builtins_exe_bytes).map_err(|e| {
        CompilerError::Codegen(CompileError::cranelift(format!(
            "load builtins executable: {e}"
        )))
    })?;

    lp_riscv_elf::load_object_file(
        object_elf,
        &mut load_info.code,
        &mut load_info.ram,
        &mut load_info.symbol_map,
    )
    .map_err(|e| {
        CompilerError::Codegen(CompileError::cranelift(format!(
            "load object into image: {e}"
        )))
    })?;

    let mut missing_symbols = Vec::new();
    let mut undefined_symbols = Vec::new();

    for builtin in BuiltinId::all() {
        let symbol_name = builtin.name();
        match load_info.symbol_map.get(symbol_name) {
            Some(&0) => undefined_symbols.push(String::from(symbol_name)),
            Some(_) => {}
            None => missing_symbols.push(String::from(symbol_name)),
        }
    }

    if !undefined_symbols.is_empty() {
        return Err(CompilerError::Codegen(CompileError::cranelift(format!(
            "builtin symbols undefined after link: {undefined_symbols:?}"
        ))));
    }
    if !missing_symbols.is_empty() {
        return Err(CompilerError::Codegen(CompileError::cranelift(format!(
            "builtin symbols missing after link: {missing_symbols:?}"
        ))));
    }

    Ok(load_info)
}
