//! ELF parsing and validation utilities.

use alloc::format;
use alloc::string::String;
use object::Object;

/// Parse an ELF file using the object crate.
pub fn parse_elf(elf_data: &[u8]) -> Result<object::File<'_>, String> {
    debug!("=== Parsing ELF file ===");
    debug!("ELF size: {} bytes", elf_data.len());

    let obj = object::File::parse(elf_data).map_err(|e| format!("Failed to parse ELF: {e}"))?;

    Ok(obj)
}

/// Validate that the ELF is RISC-V 32-bit.
pub fn validate_elf(obj: &object::File) -> Result<(), String> {
    debug!("=== Validating ELF ===");

    // Check architecture
    match obj.architecture() {
        object::Architecture::Riscv32 => {
            debug!("Architecture: RISC-V 32-bit");
        }
        arch => {
            return Err(format!(
                "Unsupported architecture: {arch:?}. Expected RISC-V 32-bit"
            ));
        }
    }

    // Check endianness (should be little-endian for RISC-V)
    match obj.endianness() {
        object::Endianness::Little => {
            debug!("Endianness: Little-endian");
        }
        endian => {
            return Err(format!(
                "Unsupported endianness: {endian:?}. Expected little-endian"
            ));
        }
    }

    Ok(())
}

/// Extract the entry point address from the ELF.
pub fn extract_entry_point(obj: &object::File) -> u32 {
    let entry = obj.entry();
    debug!("Entry point: 0x{:x}", entry);
    entry as u32
}
