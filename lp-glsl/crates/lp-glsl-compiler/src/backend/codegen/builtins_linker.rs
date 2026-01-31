//! Builtins library linking and verification utilities

#[cfg(all(feature = "std", feature = "emulator"))]
extern crate std;

#[cfg(all(feature = "std", feature = "emulator"))]
use crate::error::{ErrorCode, GlslError};
#[cfg(all(feature = "std", feature = "emulator"))]
use alloc::vec::Vec;

/// Link builtins executable with object file and verify symbols are defined
///
/// # Arguments
/// * `elf_bytes` - The ELF object file bytes to load into the executable
/// * `builtins_exe_bytes` - The lp-builtins-app executable bytes
///
/// # Returns
/// * `Ok(ElfLoadInfo)` - The loaded ELF info with object file loaded
/// * `Err(GlslError)` - If loading or verification fails
#[cfg(all(feature = "std", feature = "emulator"))]
pub fn link_and_verify_builtins(
    elf_bytes: &[u8],
    builtins_exe_bytes: &[u8],
) -> Result<lp_riscv_tools::ElfLoadInfo, GlslError> {
    use crate::backend::builtins::registry::BuiltinId;

    crate::debug!("=== Loading object file into builtins executable ===");
    crate::debug!(
        "Builtins executable size: {} bytes",
        builtins_exe_bytes.len()
    );
    crate::debug!("Object file size: {} bytes", elf_bytes.len());

    if builtins_exe_bytes.is_empty() {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "lp-builtins-app executable is empty or not available. \
             Build it with: scripts/build-builtins.sh",
        ));
    }

    // Load the base executable
    crate::debug!("Loading base executable...");
    let mut load_info = lp_riscv_tools::load_elf(builtins_exe_bytes).map_err(|e| {
        GlslError::new(
            ErrorCode::E0400,
            format!(
                "Failed to load base executable: {}. \
                     Ensure lp-builtins-app is correctly compiled.",
                e
            ),
        )
    })?;

    crate::debug!("Base executable loaded successfully!");

    // Load the object file into the base executable
    crate::debug!("Loading object file...");
    let _obj_info = lp_riscv_tools::elf_loader::load_object_file(
        elf_bytes,
        &mut load_info.code,
        &mut load_info.ram,
        &mut load_info.symbol_map,
    )
    .map_err(|e| {
        GlslError::new(
            ErrorCode::E0400,
            format!(
                "Failed to load object file: {}. \
                 Ensure the object file is correctly compiled.",
                e
            ),
        )
    })?;

    crate::debug!("Object file loaded successfully!");

    // Verify that builtin symbols are present and defined in the merged symbol map
    let mut missing_symbols = Vec::new();
    let mut undefined_symbols = Vec::new();

    crate::debug!("Checking for builtin symbols in merged symbol map...");
    crate::debug!("Symbol map contains {} symbols", load_info.symbol_map.len());

    for builtin in BuiltinId::all() {
        let symbol_name = builtin.name();
        crate::debug!("Checking for builtin symbol: {}", symbol_name);

        if let Some(&address) = load_info.symbol_map.get(symbol_name) {
            if address == 0 {
                crate::debug!(
                    "  -> Symbol {} found but address is 0 (undefined)",
                    symbol_name
                );
                undefined_symbols.push(symbol_name);
            } else {
                crate::debug!(
                    "  -> Symbol {} found at address 0x{:x} (defined)",
                    symbol_name,
                    address
                );
            }
        } else {
            crate::debug!("  -> Symbol {} not found in symbol map", symbol_name);
            missing_symbols.push(symbol_name);
        }
    }

    crate::debug!("undefined_symbols: {:?}", undefined_symbols);
    crate::debug!("missing_symbols: {:?}", missing_symbols);

    if !undefined_symbols.is_empty() {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "Builtin symbols are undefined after loading: {:?}. \
                 These symbols were declared but not resolved. \
                 Ensure lp-builtins library is built and linked correctly.",
                undefined_symbols
            ),
        ));
    }

    if !missing_symbols.is_empty() {
        return Err(GlslError::new(
            ErrorCode::E0400,
            format!(
                "Builtin symbols not found after loading: {:?}. \
                 Ensure lp-builtins library is built and contains these symbols.",
                missing_symbols
            ),
        ));
    }

    Ok(load_info)
}
