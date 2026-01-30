//! Simple ELF file generator for RISC-V 32-bit.
//!
//! This module creates minimal ELF files that can be transpiled by embive.

use alloc::vec::Vec;

/// Generate a simple RISC-V 32-bit ELF file containing the given machine code.
///
/// This creates a minimal executable ELF with:
/// - ELF header
/// - Program header for the code segment
/// - The machine code itself
#[allow(dead_code)] // Reserved for future use
pub fn generate_simple_elf(code: &[u8]) -> Vec<u8> {
    let mut elf = Vec::new();

    // ELF Header (52 bytes for 32-bit)
    elf.extend_from_slice(b"\x7fELF"); // Magic number
    elf.push(1); // 32-bit
    elf.push(1); // Little endian
    elf.push(1); // ELF version
    elf.push(0); // SYSV ABI
    elf.extend_from_slice(&[0; 8]); // Padding

    elf.extend_from_slice(&2u16.to_le_bytes()); // e_type: ET_EXEC
    elf.extend_from_slice(&0xf3u16.to_le_bytes()); // e_machine: EM_RISCV
    elf.extend_from_slice(&1u32.to_le_bytes()); // e_version
    elf.extend_from_slice(&0x10000u32.to_le_bytes()); // e_entry: entry point address
    elf.extend_from_slice(&52u32.to_le_bytes()); // e_phoff: program header offset
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_shoff: section header offset (none)
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    elf.extend_from_slice(&52u16.to_le_bytes()); // e_ehsize: ELF header size
    elf.extend_from_slice(&32u16.to_le_bytes()); // e_phentsize: program header size
    elf.extend_from_slice(&1u16.to_le_bytes()); // e_phnum: number of program headers
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize: section header size
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum: number of section headers
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx: section name string table index

    // Program Header (32 bytes for 32-bit)
    let code_offset = 52 + 32; // After ELF header and program header
    let code_vaddr = 0x10000u32; // Virtual address

    elf.extend_from_slice(&1u32.to_le_bytes()); // p_type: PT_LOAD
    elf.extend_from_slice(&(code_offset as u32).to_le_bytes()); // p_offset
    elf.extend_from_slice(&code_vaddr.to_le_bytes()); // p_vaddr
    elf.extend_from_slice(&code_vaddr.to_le_bytes()); // p_paddr
    elf.extend_from_slice(&(code.len() as u32).to_le_bytes()); // p_filesz
    elf.extend_from_slice(&(code.len() as u32).to_le_bytes()); // p_memsz
    elf.extend_from_slice(&7u32.to_le_bytes()); // p_flags: R+W+X
    elf.extend_from_slice(&0x1000u32.to_le_bytes()); // p_align

    // Code section
    elf.extend_from_slice(code);

    elf
}
